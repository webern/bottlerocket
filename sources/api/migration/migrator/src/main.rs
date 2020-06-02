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

use nix::{dir::Dir, fcntl::OFlag, sys::stat::Mode, unistd::fsync};
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use semver::Version;
use simplelog::{Config as LogConfig, TermLogger, TerminalMode};
use snafu::{ensure, OptionExt, ResultExt};
use std::collections::HashSet;
use std::env;
use std::fs::{self, File, OpenOptions, Permissions};
use std::os::unix::fs::{symlink, PermissionsExt};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{self, Command};

use update_metadata::{load_manifest, Direction, REPOSITORY_LIMITS};

mod args;
mod error;

use args::Args;
use error::Result;
use tough::ExpirationEnforcement;

const RUNDIR: &str = "rundir";
const TOUGH_DATASTORE: &str = "tough";

// Returning a Result from main makes it print a Debug representation of the error, but with Snafu
// we have nice Display representations of the error, so we wrap "main" (run) and print any error.
// https://github.com/shepmaster/snafu/issues/110
fn main() {
    let args = Args::from_env(env::args());
    if let Err(e) = run(&args) {
        eprintln!("{}", e);
        process::exit(1);
    }
}

fn run(args: &Args) -> Result<()> {
    // TerminalMode::Mixed will send errors to stderr and anything less to stdout.
    if let Err(e) = TermLogger::init(args.log_level, LogConfig::default(), TerminalMode::Mixed) {
        info!("Term logger init returned an error: {}", e)
    }

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

    // Prepare to load the locally cached tough repository to obtain the manifest.
    let tough_datastore = args.working_directory.join(TOUGH_DATASTORE);
    fs::create_dir_all(&tough_datastore).context(error::CreateDirectory {
        path: &tough_datastore,
    })?;
    let metadata_url = dir_url(&args.metadata_directory)?;
    let migrations_url = dir_url(&args.migration_directory)?;

    // Failure to load the tough repo at the expected location is a serious issue because updog
    // should always create a tough repo that contains at least the manifest, even if there are no
    // migrations.
    let repo = tough::Repository::load(
        &tough::FilesystemTransport {},
        tough::Settings {
            root: File::open(&args.root_path).context(error::OpenRoot {
                path: args.root_path.clone(),
            })?,
            datastore: &tough_datastore,
            metadata_base_url: metadata_url.as_str(),
            targets_base_url: migrations_url.as_str(),
            limits: REPOSITORY_LIMITS,
            // if metadata has expired since the time that updog downloaded them, we do not want to
            // fail the migration process, so we set expiration enforcement to unsafe.
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
        // Prepare directory to save migrations to before running them.
        // TODO - use pentacle instead of saving the migration binaries to disk before running them.
        let rundir = args.working_directory.join(RUNDIR);
        fs::create_dir_all(&rundir).context(error::CreateDirectory { path: &rundir })?;
        let copy_path = run_migrations(
            &repo,
            direction,
            &migrations,
            &args.datastore_path,
            &args.migrate_to_version,
            &rundir,
        )?;
        flip_to_new_version(&args.migrate_to_version, &copy_path)?;
        if let Err(err) = std::fs::remove_dir_all(&rundir) {
            error!("Error deleting directory '{}': {}", rundir.display(), err)
        }
    }
    if let Err(err) = std::fs::remove_dir_all(&tough_datastore) {
        error!(
            "Error deleting directory '{}': {}",
            tough_datastore.display(),
            err
        )
    }
    Ok(())
}

// =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=

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
fn run_migrations<P1, P2>(
    repository: &tough::Repository<'_, tough::FilesystemTransport>,
    direction: Direction,
    migrations: &[String],
    source_datastore: P1,
    new_version: &Version,
    migrations_rundir: P2,
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
        // get the migration from the repo
        let lz4_bytes = repository
            .read_target(migration.as_str())
            .context(error::LoadMigration {
                migration: migration.to_owned(),
            })?
            .context(error::MigrationNotFound {
                migration: migration.to_owned(),
            })?;

        // deflate with an lz4 decoder read
        let mut reader = lz4::Decoder::new(lz4_bytes).context(error::Lz4Decode { migration })?;

        // TODO - remove this use of the filesystem when we add pentacle
        let exec_path = migrations_rundir.as_ref().join(&migration);
        {
            let mut f = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&exec_path)
                .context(error::MigrationSave { path: &exec_path })?;
            let _ = std::io::copy(&mut reader, &mut f)
                .context(error::MigrationSave { path: &exec_path })?;
        }

        // Ensure the migration is executable.
        fs::set_permissions(&exec_path, Permissions::from_mode(0o755))
            .context(error::SetPermissions { path: &exec_path })?;

        let mut command = Command::new(&exec_path);

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

        let output = command
            .output()
            .context(error::StartMigration { command })?;

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

/// Converts a filepath into a URI formatted string
fn dir_url<P: AsRef<Path>>(path: P) -> Result<String> {
    let path_str = path.as_ref().to_str().context(error::PathUrl {
        path: path.as_ref(),
    })?;
    Ok(format!("file://{}", path_str))
}

// =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=

#[cfg(test)]
mod test {
    use super::*;
    use tempfile::TempDir;

    pub fn test_data() -> PathBuf {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.pop();
        p.join("migrator").join("tests").join("data")
    }

    struct MigrationTestInfo {
        tmp: TempDir,
        from_version: Version,
        to_version: Version,
        datastore: PathBuf,
    }

    impl MigrationTestInfo {
        fn new(from_version: Version, to_version: Version) -> Self {
            MigrationTestInfo {
                tmp: TempDir::new().unwrap(),
                from_version,
                to_version,
                datastore: PathBuf::default(),
            }
        }
    }

    /// Migrator relies on the datastore symlink structure to determine the 'from' version.
    /// This function sets up the directory and symlinks to mock the datastore for migrator.
    fn create_datastore_links(info: &mut MigrationTestInfo) {
        info.datastore = info.tmp.path().join(format!(
            "v{}.{}.{}_xyz",
            info.from_version.major, info.from_version.minor, info.from_version.patch
        ));
        let datastore_version = info.tmp.path().join(format!(
            "v{}.{}.{}",
            info.from_version.major, info.from_version.minor, info.from_version.patch
        ));
        let datastore_minor = info.tmp.path().join(format!(
            "v{}.{}",
            info.from_version.major, info.from_version.minor
        ));
        let datastore_major = info
            .tmp
            .path()
            .join(format!("v{}", info.from_version.major));
        let datastore_current = info.tmp.path().join("current");
        fs::create_dir_all(&info.datastore).unwrap();
        std::os::unix::fs::symlink(&info.datastore, &datastore_version).unwrap();
        std::os::unix::fs::symlink(&datastore_version, &datastore_minor).unwrap();
        std::os::unix::fs::symlink(&datastore_minor, &datastore_major).unwrap();
        std::os::unix::fs::symlink(&datastore_major, &datastore_current).unwrap();
    }

    fn root() -> PathBuf {
        test_data()
            .join("repository")
            .join("metadata")
            .join("1.root.json")
    }

    fn tuf_metadata() -> PathBuf {
        test_data().join("repository").join("metadata")
    }

    fn tuf_targets() -> PathBuf {
        test_data().join("repository").join("targets")
    }

    /// Tests the migrator program end-to-end using the `run` function.
    /// The test uses a locally stored tuf repo at `migrator/tests/data/repository`.
    /// In the `manifest.json` we have specified the following migrations:
    /// ```
    ///     "(0.99.0, 0.99.1)": [
    ///       "x-first-migration.lz4",
    ///       "a-second-migration.lz4"
    ///     ]
    /// ```
    ///
    /// The two 'migrations' are bash scripts with content like this:
    ///
    /// ```
    /// #!/bin/bash
    /// set -eo pipefail
    /// migration_name="x-first-migration"
    /// datastore_parent_dir="$(dirname "${3}")"
    /// outfile="${datastore_parent_dir}/result.txt"
    /// echo "${migration_name}: writing a message to '${outfile}'"
    /// echo "${migration_name}:" "${@}" >> "${outfile}"
    /// ```
    ///
    /// These 'migrations' use the --source-datastore path and take its parent.
    /// Into this parent directory they write lines to a file named result.txt.
    /// In the test we read the result.txt file to see that the migrations have been run in the
    /// expected order.
    ///
    /// This test ensures that migrations run when migrating from an older to a newer version.
    #[test]
    fn migrate_forward() {
        let from_version = Version::parse("0.99.0").unwrap();
        let to_version = Version::parse("0.99.1").unwrap();
        let mut info = MigrationTestInfo::new(from_version, to_version);
        create_datastore_links(&mut info);
        let args = Args {
            datastore_path: info.datastore.clone(),
            log_level: log::LevelFilter::Info,
            migration_directory: tuf_targets(),
            migrate_to_version: info.to_version.clone(),
            root_path: root(),
            metadata_directory: tuf_metadata(),
            working_directory: info.tmp.path().join("wrk"),
        };
        run(&args).unwrap();
        // the migrations should write to a file named result.txt.
        let output_file = info.tmp.path().join("result.txt");
        let contents = std::fs::read_to_string(&output_file).unwrap();
        let lines: Vec<&str> = contents.split('\n').collect();
        assert_eq!(lines.len(), 3);
        let first_line = *lines.get(0).unwrap();
        if !first_line.contains("x-first-migration: --forward") {
            panic!(format!(
                "Expected the migration 'x-first-migration.sh' to run first and write \
            a message containing 'x-first-migration: --forward' to the output file. Instead found \
            '{}'",
                first_line
            ));
        }
        let second_line = *lines.get(1).unwrap();
        if !second_line.contains("a-second-migration: --forward") {
            panic!(format!(
                "Expected the migration 'a-second-migration.sh' to run second and write \
            a message containing 'a-second-migration: --forward' to the output file. Instead found \
            '{}'",
                second_line
            ));
        }
    }

    /// This test ensures that migrations run when migrating from a newer to an older version.
    /// See `migrate_forward` for a description of how these tests work.
    #[test]
    fn migrate_backward() {
        let from_version = Version::parse("0.99.1").unwrap();
        let to_version = Version::parse("0.99.0").unwrap();
        let mut info = MigrationTestInfo::new(from_version, to_version);
        create_datastore_links(&mut info);
        let args = Args {
            datastore_path: info.datastore.clone(),
            log_level: log::LevelFilter::Info,
            migration_directory: tuf_targets(),
            migrate_to_version: info.to_version.clone(),
            root_path: root(),
            metadata_directory: tuf_metadata(),
            working_directory: info.tmp.path().join("wrk"),
        };
        run(&args).unwrap();
        let output_file = info.tmp.path().join("result.txt");
        let contents = std::fs::read_to_string(&output_file).unwrap();
        let lines: Vec<&str> = contents.split('\n').collect();
        assert_eq!(lines.len(), 3);
        let first_line = *lines.get(0).unwrap();
        if !first_line.contains("a-second-migration: --backward") {
            panic!(format!(
                "Expected the migration 'a-second-migration.sh' to run first and write \
            a message containing 'a-second-migration: --backward' to the output file. Instead \
            found '{}'",
                first_line
            ));
        }
        let second_line = *lines.get(1).unwrap();
        if !second_line.contains("x-first-migration: --backward") {
            panic!(format!(
                "Expected the migration 'x-first-migration.sh' to run second and write \
            a message containing 'x-first-migration: --backward' to the output file. Instead \
            found '{}'",
                second_line
            ));
        }
    }
}
