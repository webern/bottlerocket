/*!
# test_data

This crate provides strongly-typed access to the test data files that multiple Bottlerocket
components use during testing. This allows us to move crates around without change string constant
file paths in multiple places, and also allows us to use the compiler to know which files are being
used by which tests.

*/
use std::path::PathBuf;

/// Represents a manifest.json file used in testing.
pub enum ManifestFile {
    BadBound,
    DuplicateBound,
    Example,
    Example2,
    Example3,
    Migrations,
    Multiple,
    SingleWave,
}

/// Represents a waves.toml file used in testing.
pub enum WavesFile {
    DefaultWaves,
}

/// Represents a release.toml file used in testing.
pub enum ReleaseFile {
    Release,
}

/// Returns the path to a test manifest.json file.
pub fn manifest_filepath(variant: ManifestFile) -> PathBuf {
    test_data().join(manifest_filename(variant))
}

/// Returns the path to a test waves.toml file.
pub fn waves_filepath(variant: WavesFile) -> PathBuf {
    test_data().join(waves_filename(variant))
}

/// Returns the path to a release.toml file.
pub fn release_filepath(variant: ReleaseFile) -> PathBuf {
    test_data().join(release_filename(variant))
}

/// Provides the string constant filename for a manifest file.
fn manifest_filename(variant: ManifestFile) -> &'static str {
    match variant {
        ManifestFile::BadBound => "bad-bound.json",
        ManifestFile::DuplicateBound => "duplicate-bound.json",
        ManifestFile::Example => "example.json",
        ManifestFile::Example2 => "example_2.json",
        ManifestFile::Example3 => "example_3.json",
        ManifestFile::Migrations => "migrations.json",
        ManifestFile::Multiple => "multiple.json",
        ManifestFile::SingleWave => "single_wave.json",
    }
}

/// Provides the string constant filename for a waves file.
fn waves_filename(variant: WavesFile) -> &'static str {
    match variant {
        WavesFile::DefaultWaves => "default_waves.toml",
    }
}

/// Provides the string constant filename for a release file.
fn release_filename(variant: ReleaseFile) -> &'static str {
    match variant {
        ReleaseFile::Release => "release.toml",
    }
}

/// Returns the path to our test data directory. If the crate is moved, this is the only function
/// that should need to be changed.
fn test_data() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.join("test_files").join("tests").join("data")
}
