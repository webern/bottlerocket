//! This module owns the error type used by the migrator.

use semver::Version;
use snafu::{Backtrace, Snafu};
use std::io;
use std::path::PathBuf;
use std::process::{Command, Output};

/// Error contains the errors that can happen during migration.
#[derive(Debug, Snafu)]
#[snafu(visibility = "pub(crate)")]
pub(crate) enum Error {
    #[snafu(display("Internal error: {}", msg))]
    Internal { msg: String },

    #[snafu(display("Unable to create tempdir for migration binaries: '{}'", source))]
    CreateRunDir { source: std::io::Error },

    #[snafu(display("Unable to create tempdir for tough datastore: '{}'", source))]
    CreateToughTempDir { source: std::io::Error },

    #[snafu(display("Data store path '{}' contains invalid UTF-8", path.display()))]
    DataStorePathNotUTF8 { path: PathBuf },

    #[snafu(display("Unable to open data store directory '{}': {}", path.display(), source))]
    DataStoreDirOpen { path: PathBuf, source: nix::Error },

    #[snafu(display("Data store link '{}' points to /", path.display()))]
    DataStoreLinkToRoot { path: PathBuf },

    #[snafu(display("Unable to delete directory '{}': {}", path.display(), source))]
    DeleteDirectory {
        path: PathBuf,
        source: std::io::Error,
    },

    #[snafu(display("Failed to convert '{}' to a URL", path.display()))]
    DirectoryUrl { path: PathBuf, backtrace: Backtrace },

    #[snafu(display("Error finding migration: {}", source))]
    FindMigrations {
        source: update_metadata::error::Error,
        backtrace: Backtrace,
    },

    #[snafu(display("Data store path '{}' contains invalid version: {}", path.display(), source))]
    InvalidDataStoreVersion {
        path: PathBuf,
        source: semver::SemVerError,
    },

    #[snafu(display("Migration '{}' contains invalid version: {}", path.display(), source))]
    InvalidMigrationVersion {
        path: PathBuf,
        source: semver::SemVerError,
    },

    #[snafu(display("Data store for new version {} already exists at {}", version, path.display()))]
    NewVersionAlreadyExists { version: Version, path: PathBuf },

    #[snafu(display("Unable to start migration command {:?} - {}", command, source))]
    StartMigration { command: Command, source: io::Error },

    #[snafu(display("Migration returned '{}' - stderr: {}",
                    output.status.code()
                        .map(|i| i.to_string()).unwrap_or_else(|| "signal".to_string()),
                    std::str::from_utf8(&output.stderr)
                        .unwrap_or_else(|_e| "<invalid UTF-8>")))]
    MigrationFailure { output: Output },

    #[snafu(display("Failed to create symlink for new version at {}: {}", path.display(), source))]
    LinkCreate { path: PathBuf, source: io::Error },

    #[snafu(display("Failed to swap symlink at {} to new version: {}", link.display(), source))]
    LinkSwap { link: PathBuf, source: io::Error },

    #[snafu(display("Failed to read symlink at {} to find version: {}", link.display(), source))]
    LinkRead { link: PathBuf, source: io::Error },

    #[snafu(display("Failed listing migration directory '{}': {}", dir.display(), source))]
    ListMigrations { dir: PathBuf, source: io::Error },

    #[snafu(display("Error loading manifest: {}", source))]
    LoadManifest {
        source: update_metadata::error::Error,
        backtrace: Backtrace,
    },

    #[snafu(display("Error loading migration '{}': {}", migration, source))]
    LoadMigration {
        migration: String,
        source: tough::error::Error,
        backtrace: Backtrace,
    },

    #[snafu(display("Failed to decode LZ4-compressed migration {}: {}", migration, source))]
    Lz4Decode {
        migration: String,
        source: std::io::Error,
        backtrace: Backtrace,
    },

    #[snafu(display("Migration '{}' not found", migration))]
    MigrationNotFound {
        migration: String,
        backtrace: Backtrace,
    },

    #[snafu(display("Error saving migration '{}': {}", path.display(), source))]
    MigrationSave {
        path: PathBuf,
        source: std::io::Error,
        backtrace: Backtrace,
    },

    #[snafu(display("Failed to open trusted root metadata file {}: {}", path.display(), source))]
    OpenRoot {
        path: PathBuf,
        source: std::io::Error,
        backtrace: Backtrace,
    },

    #[snafu(display("Unable to create URL from path '{}'", path.display()))]
    PathUrl { path: PathBuf, backtrace: Backtrace },

    #[snafu(display("Failed reading migration directory entry: {}", source))]
    ReadMigrationEntry { source: io::Error },

    #[snafu(display("Failed to load TUF repo: {}", source))]
    RepoLoad {
        source: tough::error::Error,
        backtrace: Backtrace,
    },

    #[snafu(display("Failed reading metadata of '{}': {}", path.display(), source))]
    PathMetadata { path: PathBuf, source: io::Error },

    #[snafu(display("Failed setting permissions of '{}': {}", path.display(), source))]
    SetPermissions { path: PathBuf, source: io::Error },

    #[snafu(display("Migration path '{}' contains invalid UTF-8", path.display()))]
    MigrationNameNotUTF8 { path: PathBuf },

    #[snafu(display("Unable to create directory '{}'", path.display()))]
    UnsignedMigrationsCreateDir {
        path: PathBuf,
        source: std::io::Error,
    },

    #[snafu(display("Unable to list directory '{}'", path.display()))]
    UnsignedMigrationsListDir {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// Result alias containing our Error type.
pub(crate) type Result<T> = std::result::Result<T, Error>;
