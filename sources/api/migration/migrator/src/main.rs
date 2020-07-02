//! migrator is a tool to run migrations built with the migration-helpers library.
//!
//! It must be given:
//! * a data store to migrate
//! * a version to migrate it to
//! * where to find migration binaries
//!
//! Given those, it will:
//! * confirm that the given data store has the appropriate versioned symlink structure
//! * find the version of the given data store
//! * find migrations between the two versions
//! * if there are migrations:
//!   * run the migrations; the transformed data becomes the new data store
//! * if there are *no* migrations:
//!   * just symlink to the old data store
//! * do symlink flips so the new version takes the place of the original
//!
//! To understand motivation and more about the overall process, look at the migration system
//! documentation, one level up.

#![deny(rust_2018_idioms)]

#[macro_use]
extern crate log;

use args::Args;
use direction::Direction;
use error::Result;
use nix::{dir::Dir, fcntl::OFlag, sys::stat::Mode, unistd::fsync};
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use semver::Version;
use simplelog::{Config as LogConfig, TermLogger, TerminalMode};
use snafu::{ensure, OptionExt, ResultExt};
use std::collections::HashSet;
use std::env;
use std::fs::{self, File};
use std::os::unix::fs::symlink;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process;
use tempfile::TempDir;
use tough::{ExpirationEnforcement, Limits};
use update_metadata::load_manifest;

mod args;
mod direction;
mod error;
#[cfg(test)]
mod test;

// Returning a Result from main makes it print a Debug representation of the error, but with Snafu
// we have nice Display representations of the error, so we wrap "main" (run) and print any error.
// https://github.com/shepmaster/snafu/issues/110
fn main() {
    let args = Args::from_env(env::args());
    // TerminalMode::Mixed will send errors to stderr and anything less to stdout.
    if let Err(e) = TermLogger::init(args.log_level, LogConfig::default(), TerminalMode::Mixed) {
        eprintln!("{}", e);
        process::exit(1);
    }
    if let Err(e) = run(&args) {
        eprintln!("{}", e);
        process::exit(1);
    }
}

fn get_current_version<P>(datastore_dir: P) -> Result<Version>
where
    P: AsRef<Path>,
{
    let datastore_dir = datastore_dir.as_ref();

    // Find the current patch version link, which contains our full version number
    let current = datastore_dir.join("current");
    let major =
        datastore_dir.join(fs::read_link(&current).context(error::LinkRead { link: current })?);
    let minor = datastore_dir.join(fs::read_link(&major).context(error::LinkRead { link: major })?);
    let patch = datastore_dir.join(fs::read_link(&minor).context(error::LinkRead { link: minor })?);

    // Pull out the basename of the path, which contains the version
    let version_os_str = patch
        .file_name()
        .context(error::DataStoreLinkToRoot { path: &patch })?;
    let mut version_str = version_os_str
        .to_str()
        .context(error::DataStorePathNotUTF8 { path: &patch })?;

    // Allow 'v' at the start so the links have clearer names for humans
    if version_str.starts_with('v') {
        version_str = &version_str[1..];
    }

    Version::parse(version_str).context(error::InvalidDataStoreVersion { path: &patch })
}

pub(crate) fn run(args: &Args) -> Result<()> {
    // Get the directory we're working in.
    let datastore_dir = args
        .datastore_path
        .parent()
        .context(error::DataStoreLinkToRoot {
            path: &args.datastore_path,
        })?;

    let current_version = get_current_version(&datastore_dir)?;
    let direction = Direction::from_versions(&current_version, &args.migrate_to_version)
        .unwrap_or_else(|| {
            info!(
                "Requested version {} matches version of given datastore at '{}'; nothing to do",
                args.migrate_to_version,
                args.datastore_path.display()
            );
            process::exit(0);
        });

    // Prepare to load the locally cached TUF repository to obtain the manifest. Part of using a
    // `TempDir` is disabling timestamp checking, because we want an instance to still come up and
    // run migrations regardless of the how the system time relates to what we have cached (for
    // example if someone runs an update, then shuts down the instance for several weeks, beyond the
    // expiration of at least the cached timestamp.json before booting it back up again). We also
    // use a `TempDir` because see no value in keeping a datastore around. The latest  known
    // versions of the repository metadata will always be the versions of repository metadata we
    // have cached on the disk. More info at `ExpirationEnforcement::Unsafe` below.
    let tough_datastore = TempDir::new().context(error::CreateToughTempDir)?;
    let metadata_url = url::Url::from_directory_path(&args.metadata_directory).map_err(|_| {
        error::Error::DirectoryUrl {
            path: args.metadata_directory.clone(),
        }
    })?;
    let migrations_url =
        url::Url::from_directory_path(&args.migration_directory).map_err(|_| {
            error::Error::DirectoryUrl {
                path: args.migration_directory.clone(),
            }
        })?;
    // Failure to load the TUF repo at the expected location is a serious issue because updog should
    // always create a TUF repo that contains at least the manifest, even if there are no migrations.
    let repo = tough::Repository::load(
        &tough::FilesystemTransport,
        tough::Settings {
            root: File::open(&args.root_path).context(error::OpenRoot {
                path: args.root_path.clone(),
            })?,
            datastore: tough_datastore.path(),
            metadata_base_url: metadata_url.as_str(),
            targets_base_url: migrations_url.as_str(),
            limits: Limits::default(),
            // The threats TUF mitigates are more than the threats we are attempting to mitigate
            // here by caching signatures for migrations locally and using them after a reboot but
            // prior to Internet connectivity. We are caching the TUF repo and use it while offline
            // after a reboot to mitigate binaries being added or modified in the migrations
            // directory; the TUF repo is simply a code signing method we already have in place,
            // even if it's not one that initially makes sense for this use case. So, we don't care
            // if the targets expired between updog downloading them and now.
            expiration_enforcement: ExpirationEnforcement::Unsafe,
        },
    )
    .context(error::RepoLoad)?;
    let manifest = load_manifest(&repo).context(error::LoadManifest)?;
    let migrations =
        update_metadata::find_migrations(&current_version, &args.migrate_to_version, &manifest)
            .context(error::FindMigrations)?;

    if migrations.is_empty() {
        // Not all new OS versions need to change the data store format.  If there's been no
        // change, we can just link to the last version rather than making a copy.
        // (Note: we link to the fully resolved directory, args.datastore_path,  so we don't
        // have a chain of symlinks that could go past the maximum depth.)
        flip_to_new_version(&args.migrate_to_version, &args.datastore_path)?;
    } else {
        let copy_path = run_migrations(
            &repo,
            direction,
            &migrations,
            &args.datastore_path,
            &args.migrate_to_version,
        )?;
        flip_to_new_version(&args.migrate_to_version, &copy_path)?;
    }
    Ok(())
}

// =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=

/// Generates a random ID, affectionately known as a 'rando', that can be used to avoid timing
/// issues and identify unique migration attempts.
fn rando() -> String {
    thread_rng().sample_iter(&Alphanumeric).take(16).collect()
}

/// Generates a path for a new data store, given the path of the existing data store,
/// the new version number, and a random "copy id" to append.
fn new_datastore_location<P>(from: P, new_version: &Version) -> Result<PathBuf>
where
    P: AsRef<Path>,
{
    let to = from
        .as_ref()
        .with_file_name(format!("v{}_{}", new_version, rando()));
    ensure!(
        !to.exists(),
        error::NewVersionAlreadyExists {
            version: new_version.clone(),
            path: to
        }
    );

    info!(
        "New data store is being built at work location {}",
        to.display()
    );
    Ok(to)
}

/// Runs the given migrations in their given order.  The given direction is passed to each
/// migration so it knows which direction we're migrating.
///
/// The given data store is used as a starting point; each migration is given the output of the
/// previous migration, and the final output becomes the new data store.
fn run_migrations<P, S>(
    repository: &tough::Repository<'_, tough::FilesystemTransport>,
    direction: Direction,
    migrations: &[S],
    source_datastore: P,
    new_version: &Version,
) -> Result<PathBuf>
where
    P: AsRef<Path>,
    S: AsRef<str>,
{
    // We start with the given source_datastore, updating this after each migration to point to the
    // output of the previous one.
    let mut source_datastore = source_datastore.as_ref();
    // We create a new data store (below) to serve as the target of each migration.  (Start at
    // source just to have the right type; we know we have migrations at this point.)
    let mut target_datastore = source_datastore.to_owned();
    // Any data stores we create that aren't the final one, i.e. intermediate data stores, will be
    // removed at the end.  (If we fail and return early, they're left for debugging purposes.)
    let mut intermediate_datastores = HashSet::new();

    for migration in migrations {
        let migration = migration.as_ref();
        // get the migration from the repo
        let lz4_bytes = repository
            .read_target(migration)
            .context(error::LoadMigration { migration })?
            .context(error::MigrationNotFound { migration })?;

        // Add an LZ4 decoder so the bytes will be deflated on read
        let mut reader = lz4::Decoder::new(lz4_bytes).context(error::Lz4Decode { migration })?;

        // Create a sealed command with pentacle, so we can run the verified bytes from memory
        let mut command =
            pentacle::SealedCommand::new(&mut reader).context(error::SealMigration)?;

        // Point each migration in the right direction, and at the given data store.
        command.arg(direction.to_string());
        command.args(&[
            "--source-datastore".to_string(),
            source_datastore.display().to_string(),
        ]);

        // Create a new output location for this migration.
        target_datastore = new_datastore_location(&source_datastore, &new_version)?;
        intermediate_datastores.insert(target_datastore.clone());

        command.args(&[
            "--target-datastore".to_string(),
            target_datastore.display().to_string(),
        ]);

        info!("Running migration command: {:?}", command);

        let output = command.output().context(error::StartMigration)?;

        if !output.stdout.is_empty() {
            debug!(
                "Migration stdout: {}",
                String::from_utf8_lossy(&output.stdout)
            );
        } else {
            debug!("No migration stdout");
        }
        if !output.stderr.is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // We want to see migration stderr on the console, so log at error level.
            error!("Migration stderr: {}", stderr);
        } else {
            debug!("No migration stderr");
        }

        ensure!(output.status.success(), error::MigrationFailure { output });
        source_datastore = &target_datastore;
    }

    // Remove the intermediate data stores
    intermediate_datastores.remove(&target_datastore);
    for intermediate_datastore in intermediate_datastores {
        // Even if we fail to remove an intermediate data store, we've still migrated
        // successfully, and we don't want to fail the upgrade - just let someone know for
        // later cleanup.
        trace!(
            "Removing intermediate data store at {}",
            intermediate_datastore.display()
        );
        if let Err(e) = fs::remove_dir_all(&intermediate_datastore) {
            error!(
                "Failed to remove intermediate data store at '{}': {}",
                intermediate_datastore.display(),
                e
            );
        }
    }

    Ok(target_datastore)
}

/// Atomically flips version symlinks to point to the given "to" datastore so that it becomes live.
///
/// This includes:
/// * pointing the new patch version to the given `to_datastore`
/// * pointing the minor version to the patch version
/// * pointing the major version to the minor version
/// * pointing the 'current' link to the major version
/// * fsyncing the directory to disk
fn flip_to_new_version<P>(version: &Version, to_datastore: P) -> Result<()>
where
    P: AsRef<Path>,
{
    // Get the directory we're working in.
    let to_dir = to_datastore
        .as_ref()
        .parent()
        .context(error::DataStoreLinkToRoot {
            path: to_datastore.as_ref(),
        })?;
    // We need a file descriptor for the directory so we can fsync after the symlink swap.
    let raw_dir = Dir::open(
        to_dir,
        // Confirm it's a directory
        OFlag::O_DIRECTORY,
        // (mode doesn't matter for opening a directory)
        Mode::empty(),
    )
    .context(error::DataStoreDirOpen { path: &to_dir })?;

    // Get a unique temporary path in the directory; we need this to atomically swap.
    let temp_link = to_dir.join(rando());
    // Build the path to the 'current' link; this is what we're atomically swapping from
    // pointing at the old major version to pointing at the new major version.
    // Example: /path/to/datastore/current
    let current_version_link = to_dir.join("current");
    // Build the path to the major version link; this is what we're atomically swapping from
    // pointing at the old minor version to pointing at the new minor version.
    // Example: /path/to/datastore/v1
    let major_version_link = to_dir.join(format!("v{}", version.major));
    // Build the path to the minor version link; this is what we're atomically swapping from
    // pointing at the old patch version to pointing at the new patch version.
    // Example: /path/to/datastore/v1.5
    let minor_version_link = to_dir.join(format!("v{}.{}", version.major, version.minor));
    // Build the path to the patch version link.  If this already exists, it's because we've
    // previously tried to migrate to this version.  We point it at the full `to_datastore`
    // path.
    // Example: /path/to/datastore/v1.5.2
    let patch_version_link = to_dir.join(format!(
        "v{}.{}.{}",
        version.major, version.minor, version.patch
    ));

    // Get the final component of the paths we're linking to, so we can use relative links instead
    // of absolute, for understandability.
    let to_target = to_datastore
        .as_ref()
        .file_name()
        .context(error::DataStoreLinkToRoot {
            path: to_datastore.as_ref(),
        })?;
    let patch_target = patch_version_link
        .file_name()
        .context(error::DataStoreLinkToRoot {
            path: to_datastore.as_ref(),
        })?;
    let minor_target = minor_version_link
        .file_name()
        .context(error::DataStoreLinkToRoot {
            path: to_datastore.as_ref(),
        })?;
    let major_target = major_version_link
        .file_name()
        .context(error::DataStoreLinkToRoot {
            path: to_datastore.as_ref(),
        })?;

    // =^..^=   =^..^=   =^..^=   =^..^=

    info!(
        "Flipping {} to point to {}",
        patch_version_link.display(),
        to_target.to_string_lossy(),
    );

    // Create a symlink from the patch version to the new data store.  We create it at a temporary
    // path so we can atomically swap it into the real path with a rename call.
    // This will point at, for example, /path/to/datastore/v1.5.2_0123456789abcdef
    symlink(&to_target, &temp_link).context(error::LinkCreate { path: &temp_link })?;
    // Atomically swap the link into place, so that the patch version link points to the new data
    // store copy.
    fs::rename(&temp_link, &patch_version_link).context(error::LinkSwap {
        link: &patch_version_link,
    })?;

    // =^..^=   =^..^=   =^..^=   =^..^=

    info!(
        "Flipping {} to point to {}",
        minor_version_link.display(),
        patch_target.to_string_lossy(),
    );

    // Create a symlink from the minor version to the new patch version.
    // This will point at, for example, /path/to/datastore/v1.5.2
    symlink(&patch_target, &temp_link).context(error::LinkCreate { path: &temp_link })?;
    // Atomically swap the link into place, so that the minor version link points to the new patch
    // version.
    fs::rename(&temp_link, &minor_version_link).context(error::LinkSwap {
        link: &minor_version_link,
    })?;

    // =^..^=   =^..^=   =^..^=   =^..^=

    info!(
        "Flipping {} to point to {}",
        major_version_link.display(),
        minor_target.to_string_lossy(),
    );

    // Create a symlink from the major version to the new minor version.
    // This will point at, for example, /path/to/datastore/v1.5
    symlink(&minor_target, &temp_link).context(error::LinkCreate { path: &temp_link })?;
    // Atomically swap the link into place, so that the major version link points to the new minor
    // version.
    fs::rename(&temp_link, &major_version_link).context(error::LinkSwap {
        link: &major_version_link,
    })?;

    // =^..^=   =^..^=   =^..^=   =^..^=

    info!(
        "Flipping {} to point to {}",
        current_version_link.display(),
        major_target.to_string_lossy(),
    );

    // Create a symlink from 'current' to the new major version.
    // This will point at, for example, /path/to/datastore/v1
    symlink(&major_target, &temp_link).context(error::LinkCreate { path: &temp_link })?;
    // Atomically swap the link into place, so that 'current' points to the new major version.
    fs::rename(&temp_link, &current_version_link).context(error::LinkSwap {
        link: &current_version_link,
    })?;

    // =^..^=   =^..^=   =^..^=   =^..^=

    // fsync the directory so the links point to the new version even if we crash right after
    // this.  If fsync fails, warn but continue, because we likely can't swap the links back
    // without hitting the same failure.
    fsync(raw_dir.as_raw_fd()).unwrap_or_else(|e| {
        warn!(
            "fsync of data store directory '{}' failed, update may disappear if we crash now: {}",
            to_dir.display(),
            e
        )
    });

    Ok(())
}
