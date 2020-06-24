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
use lazy_static::lazy_static;
use nix::{dir::Dir, fcntl::OFlag, sys::stat::Mode, unistd::fsync};
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use semver::Version;
use simplelog::{Config as LogConfig, TermLogger, TerminalMode};
use snafu::{ensure, OptionExt, ResultExt};
use std::collections::HashSet;
use std::env;
use std::fs::{self, File, Permissions};
use std::os::unix::fs::{symlink, PermissionsExt};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use tempfile::TempDir;
use tough::{ExpirationEnforcement, Limits};
use update_metadata::{load_manifest, MIGRATION_FILENAME_RE};

mod args;
mod direction;
mod error;

lazy_static! {
    /// This is the last version of Bottlerocket that supports *only* unsigned migrations.
    static ref LAST_UNSIGNED_MIGRATIONS_VERSION: Version = Version::new(0, 3, 4);
}

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

fn are_migrations_signed(from_version: &Version) -> bool {
    from_version.gt(&LAST_UNSIGNED_MIGRATIONS_VERSION)
}

fn find_and_run_unsigned_migrations<P1, P2>(
    migrations_directory: P1,
    datastore_path: P2,
    current_version: &Version,
    migrate_to_version: &Version,
    direction: &Direction,
) -> Result<()>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>,
{
    let migration_directories = vec![migrations_directory];
    let migrations =
        find_unsigned_migrations(&migration_directories, &current_version, migrate_to_version)?;

    if migrations.is_empty() {
        // Not all new OS versions need to change the data store format.  If there's been no
        // change, we can just link to the last version rather than making a copy.
        // (Note: we link to the fully resolved directory, args.datastore_path,  so we don't
        // have a chain of symlinks that could go past the maximum depth.)
        flip_to_new_version(migrate_to_version, datastore_path)?;
    } else {
        let copy_path =
            run_unsigned_migrations(direction, &migrations, &datastore_path, &migrate_to_version)?;
        flip_to_new_version(migrate_to_version, &copy_path)?;
    }

    Ok(())
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

fn run(args: &Args) -> Result<()> {
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

    // DEPRECATED CODE BEGIN ///////////////////////////////////////////////////////////////////////
    // check if the `from_version` supports signed migrations. if not, run the 'old'
    // unsigned migrations code and return.
    if !are_migrations_signed(&current_version) {
        return find_and_run_unsigned_migrations(
            &args.migration_directory,
            &args.datastore_path, // TODO(brigmatt) make sure this is correct
            &current_version,
            &args.migrate_to_version,
            &direction,
        );
    }
    // DEPRECATED CODE END /////////////////////////////////////////////////////////////////////////

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

/// Returns a list of all unsigned migrations found on disk.
fn find_unsigned_migrations_on_disk<P>(dir: P) -> Result<Vec<PathBuf>>
where
    P: AsRef<Path>,
{
    let dir = dir.as_ref();
    let mut result = Vec::new();

    trace!("Looking for potential migrations in {}", dir.display());
    let entries = fs::read_dir(dir).context(error::ListMigrations { dir })?;
    for entry in entries {
        let entry = entry.context(error::ReadMigrationEntry)?;
        let path = entry.path();

        // Just check that it's a file; other checks to determine whether we should actually run
        // a file we find are done by select_migrations.
        let file_type = entry
            .file_type()
            .context(error::PathMetadata { path: &path })?;
        if !file_type.is_file() {
            debug!(
                "Skipping non-file in migration directory: {}",
                path.display()
            );
            continue;
        }

        trace!("Found potential migration: {}", path.display());
        result.push(path);
    }

    Ok(result)
}

/// Returns the sublist of the given migrations that should be run, in the returned order, to move
/// from the 'from' version to the 'to' version.
fn select_unsigned_migrations<P: AsRef<Path>>(
    from: &Version,
    to: &Version,
    paths: &[P],
) -> Result<Vec<PathBuf>> {
    // Intermediate result where we also store the version and name, needed for sorting
    let mut sortable: Vec<(Version, String, PathBuf)> = Vec::new();

    for path in paths {
        let path = path.as_ref();

        // We pull the applicable version and the migration name out of the filename.
        let file_name = path
            .file_name()
            .context(error::Internal {
                msg: "Found '/' as migration",
            })?
            .to_str()
            .context(error::MigrationNameNotUTF8 { path: &path })?;
        // this will not match signed migrations because we used consistent snapshots and the signed
        // files will have a sha prefix.
        let captures = match MIGRATION_FILENAME_RE.captures(&file_name) {
            Some(captures) => captures,
            None => {
                debug!(
                    "Skipping non-migration (bad name) in migration directory: {}",
                    path.display()
                );
                continue;
            }
        };

        let version_match = captures.name("version").context(error::Internal {
            msg: "Migration name matched regex but we don't have a 'version' capture",
        })?;
        let version = Version::parse(version_match.as_str())
            .context(error::InvalidMigrationVersion { path: &path })?;

        let name_match = captures.name("name").context(error::Internal {
            msg: "Migration name matched regex but we don't have a 'name' capture",
        })?;
        let name = name_match.as_str().to_string();

        // We don't want to include migrations for the "from" version we're already on.
        // Note on possible confusion: when going backward it's the higher version that knows
        // how to undo its changes and take you to the lower version.  For example, the v2
        // migration knows what changes it made to go from v1 to v2 and therefore how to go
        // back from v2 to v1.  See tests.
        let applicable = if to > from && version > *from && version <= *to {
            info!(
                "Found applicable forward migration '{}': {} < ({}) <= {}",
                file_name, from, version, to
            );
            true
        } else if to < from && version > *to && version <= *from {
            info!(
                "Found applicable backward migration '{}': {} >= ({}) > {}",
                file_name, from, version, to
            );
            true
        } else {
            debug!(
                "Migration '{}' doesn't apply when going from {} to {}",
                file_name, from, to
            );
            false
        };

        if applicable {
            sortable.push((version, name, path.to_path_buf()));
        }
    }

    // Sort the migrations using the metadata we stored -- version first, then name so that
    // authors have some ordering control if necessary.
    sortable.sort_unstable();

    // For a Backward migration process, reverse the order.
    if to < from {
        sortable.reverse();
    }

    debug!(
        "Sorted migrations: {:?}",
        sortable
            .iter()
            // Want filename, which always applies for us, but fall back to name just in case
            .map(|(_version, name, path)| path
                .file_name()
                .map(|osstr| osstr.to_string_lossy().into_owned())
                .unwrap_or_else(|| name.to_string()))
            .collect::<Vec<String>>()
    );

    // Get rid of the name; only needed it as a separate component for sorting
    let result: Vec<PathBuf> = sortable
        .into_iter()
        .map(|(_version, _name, path)| path)
        .collect();

    Ok(result)
}

/// Given the versions we're migrating from and to, this will return an ordered list of paths to
/// migration binaries we should run to complete the migration on a data store.
/// This separation allows for easy testing of select_migrations.
fn find_unsigned_migrations<P>(paths: &[P], from: &Version, to: &Version) -> Result<Vec<PathBuf>>
where
    P: AsRef<Path>,
{
    let mut candidates = Vec::new();
    for path in paths {
        candidates.extend(find_unsigned_migrations_on_disk(path)?);
    }

    select_unsigned_migrations(from, to, &candidates)
}

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

/// Runs the given migrations in their given order.  The given direction is passed to each
/// migration so it knows which direction we're migrating.
///
/// The given data store is used as a starting point; each migration is given the output of the
/// previous migration, and the final output becomes the new data store.
fn run_unsigned_migrations<P1, P2>(
    direction: &Direction,
    migrations: &[P1],
    source_datastore: P2,
    new_version: &Version,
) -> Result<PathBuf>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>,
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
        // Ensure the migration is executable.
        fs::set_permissions(migration.as_ref(), Permissions::from_mode(0o755)).context(
            error::SetPermissions {
                path: migration.as_ref(),
            },
        )?;

        let mut command = Command::new(migration.as_ref());

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
                std::str::from_utf8(&output.stdout).unwrap_or("<invalid UTF-8>")
            );
        } else {
            debug!("No migration stdout");
        }
        if !output.stderr.is_empty() {
            let stderr = std::str::from_utf8(&output.stderr).unwrap_or("<invalid UTF-8>");
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

// =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    #[allow(unused_variables)]
    fn select_migrations_works() {
        // Migration paths for use in testing
        let m00_1 = Path::new("migrate_v0.0.0_001");
        let m01_1 = Path::new("migrate_v0.0.1_001");
        let m01_2 = Path::new("migrate_v0.0.1_002");
        let m02_1 = Path::new("migrate_v0.0.2_001");
        let m03_1 = Path::new("migrate_v0.0.3_001");
        let m04_1 = Path::new("migrate_v0.0.4_001");
        let m04_2 = Path::new("migrate_v0.0.4_002");
        let all_migrations = vec![&m00_1, &m01_1, &m01_2, &m02_1, &m03_1, &m04_1, &m04_2];

        // Versions for use in testing
        let v00 = Version::new(0, 0, 0);
        let v01 = Version::new(0, 0, 1);
        let v02 = Version::new(0, 0, 2);
        let v03 = Version::new(0, 0, 3);
        let v04 = Version::new(0, 0, 4);
        let v05 = Version::new(0, 0, 5);

        // Test going forward one minor version
        assert_eq!(
            select_unsigned_migrations(&v01, &v02, &all_migrations).unwrap(),
            vec![m02_1]
        );

        // Test going backward one minor version
        assert_eq!(
            select_unsigned_migrations(&v02, &v01, &all_migrations).unwrap(),
            vec![m02_1]
        );

        // Test going forward a few minor versions
        assert_eq!(
            select_unsigned_migrations(&v01, &v04, &all_migrations).unwrap(),
            vec![m02_1, m03_1, m04_1, m04_2]
        );

        // Test going backward a few minor versions
        assert_eq!(
            select_unsigned_migrations(&v04, &v01, &all_migrations).unwrap(),
            vec![m04_2, m04_1, m03_1, m02_1]
        );

        // Test no matching migrations
        assert!(select_unsigned_migrations(&v04, &v05, &all_migrations)
            .unwrap()
            .is_empty());
    }
}

// =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=

#[cfg(test)]
mod test {
    use super::*;
    use std::io::{Read, Write};
    use tempfile::TempDir;

    /// Provides the path to a folder where test data files reside.
    pub fn test_data() -> PathBuf {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.pop();
        p.join("migrator").join("tests").join("data")
    }

    /// Returns the filepath to a `root.json` file stored in tree for testing. This file declares
    /// an expiration date of `1970-01-01` to ensure success with an expired TUF repository.
    fn root() -> PathBuf {
        test_data()
            .join("expired-root.json")
            .canonicalize()
            .unwrap()
    }

    /// Returns the filepath to a private key, stored in tree and used only for testing.
    fn pem() -> PathBuf {
        test_data().join("snakeoil.pem").canonicalize().unwrap()
    }

    /// Returns the filepath to a Rust file that defines a small test migration program.
    fn migration_rs() -> PathBuf {
        test_data().join("migration.rs").canonicalize().unwrap()
    }

    /// The name of a test migration. The prefix `b-` ensures we are not alphabetically sorting.
    const FIRST_MIGRATION: &str = "b-first-migration";

    /// The name of a test migration. The prefix `a-` ensures we are not alphabetically sorting.
    const SECOND_MIGRATION: &str = "a-second-migration";

    /// Creates a migration executable binary by compiling a small Rust main program. Returns the
    /// binary as bytes.
    fn create_test_migration<S: AsRef<str>>(migration_name: S) -> Vec<u8> {
        let sourcecode = std::fs::read_to_string(migration_rs())
            .unwrap()
            .replace("migration-name-replaceme", migration_name.as_ref());
        let tempdir = TempDir::new().unwrap();
        let source_file = tempdir.path().join("migration.rs");
        std::fs::write(&source_file, sourcecode.as_bytes()).unwrap();
        let compile_command_result = Command::new("rustc")
            .args(&[source_file.to_str().unwrap()])
            .current_dir(tempdir.path())
            .output();
        match compile_command_result {
            Err(e) => panic!("failed to compile the test migration binary: {:?}", e),
            Ok(output) => {
                if !output.status.success() {
                    panic!(
                        "compiling the test migration binary exited non-zero: {}",
                        std::str::from_utf8(output.stderr.as_slice()).unwrap()
                    )
                }
            }
        }
        let binary = tempdir.path().join("migration");
        let mut f = File::open(&binary).expect("test migration binary not found");
        let metadata = fs::metadata(&binary).unwrap();
        let mut buffer = vec![0; metadata.len() as usize];
        f.read(&mut buffer).unwrap();
        buffer
    }

    /// Holds the lifetime of a `TempDir` inside which a datastore directory and links are held for
    /// testing.
    struct TestDatastore {
        tmp: TempDir,
        datastore: PathBuf,
    }

    impl TestDatastore {
        /// Creates a `TempDir`, sets up the datastore links needed to represent the `from_version`
        /// and returns a `TestDatastore` populated with this information.
        fn new(from_version: &Version) -> Self {
            let tmp = TempDir::new().unwrap();
            let datastore = Self::create_datastore_links(&tmp, &from_version);
            TestDatastore { tmp, datastore }
        }

        /// Migrator relies on the datastore symlink structure to determine the 'from' version.
        /// This function sets up the directory and symlinks to mock the datastore for migrator.
        fn create_datastore_links(tmp: &TempDir, from_version: &Version) -> PathBuf {
            let datastore = tmp.path().join(format!(
                "v{}.{}.{}_xyz",
                from_version.major, from_version.minor, from_version.patch
            ));
            let datastore_version = tmp.path().join(format!(
                "v{}.{}.{}",
                from_version.major, from_version.minor, from_version.patch
            ));
            let datastore_minor = tmp
                .path()
                .join(format!("v{}.{}", from_version.major, from_version.minor));
            let datastore_major = tmp.path().join(format!("v{}", from_version.major));
            let datastore_current = tmp.path().join("current");
            fs::create_dir_all(&datastore).unwrap();
            std::os::unix::fs::symlink(&datastore, &datastore_version).unwrap();
            std::os::unix::fs::symlink(&datastore_version, &datastore_minor).unwrap();
            std::os::unix::fs::symlink(&datastore_minor, &datastore_major).unwrap();
            std::os::unix::fs::symlink(&datastore_major, &datastore_current).unwrap();
            datastore
        }
    }

    /// Represents a TUF repository, which is held in a tempdir.
    struct TestRepo {
        /// This field preserves the lifetime of the TempDir even though we never read it. When
        /// `TestRepo` goes out of scope, `TempDir` will remove the temporary directory.
        _tuf_dir: TempDir,
        metadata_path: PathBuf,
        targets_path: PathBuf,
    }

    /// LZ4 compresses `source` bytes to a new file at `destination`.
    fn compress(source: &[u8], destination: &Path) {
        let output_file = File::create(destination).unwrap();
        let mut encoder = lz4::EncoderBuilder::new()
            .level(4)
            .build(output_file)
            .unwrap();
        encoder.write_all(source).unwrap();
        let (_output, result) = encoder.finish();
        result.unwrap()
    }

    /// Creates a test repository with a couple of versions defined in the manifest and a couple of
    /// migrations. See the test description for for more info.
    fn create_test_repo() -> TestRepo {
        // This is where the signed TUF repo will exist when we are done. It is the
        // root directory of the `TestRepo` we will return when we are done.
        let test_repo_dir = TempDir::new().unwrap();
        let metadata_path = test_repo_dir.path().join("metadata");
        let targets_path = test_repo_dir.path().join("targets");

        // This is where we will stage the TUF repository targets prior to signing them. It happens
        // to be the same dir as the root of the tuf_outdir because RepositoryEditor signing uses
        // symlinks, so we need the tuf_indir and tuf_outdir/targets to both stick around for the
        // duration of the test.
        let tuf_indir = test_repo_dir.path();

        // Create a Manifest and save it to the tuftool_indir for signing.
        let mut manifest = update_metadata::Manifest::default();
        // insert the following migrations to the manifest. note that the first migration would sort
        // later than the second migration alphabetically. this is to help ensure that migrations
        // are running in their listed order (rather than sorted order as in previous
        // implementations).
        manifest.migrations.insert(
            (Version::new(0, 99, 0), Version::new(0, 99, 1)),
            vec![FIRST_MIGRATION.into(), SECOND_MIGRATION.into()],
        );
        update_metadata::write_file(tuf_indir.join("manifest.json").as_path(), &manifest).unwrap();

        // Create an executable binary that we can use as the 'migration' that migrator will run.
        // this program will write its name and arguments to a file named results.txt in the
        // directory that is the parent of `--source-datastore`. results.txt can then be used to see
        // what migrations ran, and in what order. Note that this program is sensitive to the order
        // and number of arguments passed. If `--source-datastore` is given at a different position
        // then the tests will fail and the program will need to be updated.
        let migration_a = create_test_migration(FIRST_MIGRATION);
        let migration_b = create_test_migration(SECOND_MIGRATION);

        // Save lz4 compressed copies of this bash script into the tuftool_indir to match the
        // migration specifications in the manifest.
        compress(migration_a.as_slice(), &tuf_indir.join(FIRST_MIGRATION));
        compress(migration_b.as_slice(), &tuf_indir.join(SECOND_MIGRATION));

        // Create and sign the TUF repository.
        let mut editor = tough::editor::RepositoryEditor::new(root()).unwrap();
        let long_ago: chrono::DateTime<chrono::Utc> =
            chrono::DateTime::parse_from_rfc3339("1970-01-01T00:00:00Z")
                .unwrap()
                .into();
        let one = std::num::NonZeroU64::new(1).unwrap();
        editor
            .targets_version(one)
            .targets_expires(long_ago)
            .snapshot_version(one)
            .snapshot_expires(long_ago)
            .timestamp_version(one)
            .timestamp_expires(long_ago);

        fs::read_dir(tuf_indir)
            .unwrap()
            .filter(|dir_entry_result| {
                if let Ok(dir_entry) = dir_entry_result {
                    return dir_entry.path().is_file();
                }
                false
            })
            .for_each(|dir_entry_result| {
                let dir_entry = dir_entry_result.unwrap();
                editor.add_target(
                    dir_entry.file_name().to_str().unwrap().into(),
                    tough::schema::Target::from_path(dir_entry.path()).unwrap(),
                );
            });
        let signed_repo = editor
            .sign(&[Box::new(tough::key_source::LocalKeySource { path: pem() })])
            .unwrap();
        signed_repo.link_targets(tuf_indir, &targets_path).unwrap();
        signed_repo.write(&metadata_path).unwrap();

        TestRepo {
            _tuf_dir: test_repo_dir,
            metadata_path,
            targets_path,
        }
    }

    /// Tests the migrator program end-to-end using the `run` function.
    /// The test uses a locally stored tuf repo at `migrator/tests/data/repository`.
    /// In the `manifest.json` we have specified the following migrations:
    /// ```
    ///     "(0.99.0, 0.99.1)": [
    ///       "b-first-migration",
    ///       "a-second-migration"
    ///     ]
    /// ```
    ///
    /// The two 'migrations' are instances of the same bash script (see `create_test_repo`) which
    /// writes its name (i.e. the migration name) and its arguments to a file at `./result.txt`
    /// (i.e. since migrations run in the context of the datastore directory, `result.txt` is
    /// written one directory above the datastore.) We can then inspect the contents of `result.txt`
    /// to see that the expected migrations ran in the correct order.
    #[test]
    fn migrate_forward() {
        let from_version = Version::parse("0.99.0").unwrap();
        let to_version = Version::parse("0.99.1").unwrap();
        let test_datastore = TestDatastore::new(&from_version);
        let test_repo = create_test_repo();
        let args = Args {
            datastore_path: test_datastore.datastore.clone(),
            log_level: log::LevelFilter::Info,
            migration_directory: test_repo.targets_path.clone(),
            migrate_to_version: to_version,
            root_path: root(),
            metadata_directory: test_repo.metadata_path.clone(),
        };
        run(&args).unwrap();
        // the migrations should write to a file named result.txt.
        let output_file = test_datastore.tmp.path().join("result.txt");
        let contents = std::fs::read_to_string(&output_file).unwrap();
        let lines: Vec<&str> = contents.split('\n').collect();
        assert_eq!(lines.len(), 3);
        let first_line = *lines.get(0).unwrap();
        assert!(first_line.contains(format!("{}: --forward", FIRST_MIGRATION).as_str()));
        let second_line = *lines.get(1).unwrap();
        assert!(second_line.contains(format!("{}: --forward", SECOND_MIGRATION).as_str()));
    }

    /// This test ensures that migrations run when migrating from a newer to an older version.
    /// See `migrate_forward` for a description of how these tests work.
    #[test]
    fn migrate_backward() {
        let from_version = Version::parse("0.99.1").unwrap();
        let to_version = Version::parse("0.99.0").unwrap();
        let test_datastore = TestDatastore::new(&from_version);
        let test_repo = create_test_repo();
        let args = Args {
            datastore_path: test_datastore.datastore.clone(),
            log_level: log::LevelFilter::Info,
            migration_directory: test_repo.targets_path.clone(),
            migrate_to_version: to_version,
            root_path: root(),
            metadata_directory: test_repo.metadata_path.clone(),
        };
        run(&args).unwrap();
        let output_file = test_datastore.tmp.path().join("result.txt");
        let contents = std::fs::read_to_string(&output_file).unwrap();
        let lines: Vec<&str> = contents.split('\n').collect();
        assert_eq!(lines.len(), 3);
        let first_line = *lines.get(0).unwrap();
        assert!(first_line.contains(format!("{}: --backward", SECOND_MIGRATION).as_str()));
        let second_line = *lines.get(1).unwrap();
        assert!(second_line.contains(format!("{}: --backward", FIRST_MIGRATION).as_str()));
    }
}
