use std::path::PathBuf;
use semver::Version;
use assert_cmd::Command;
use tempfile::TempDir;

// pub fn test_data() -> PathBuf {
//     let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
//     p.pop();
//     p.join("migrator").join("tests").join("data")
// }
//
// #[test]
// fn migrate_forward() {
//     println!("{}", test_data().to_string_lossy());
//     let tmp = TempDir::new().unwrap();
//     let data = test_data();
//     let root = data.join("root.json");
//     let datastore = tmp.path().join("current");
//     // std::fs::copy(data.join("datastore.json"), &datastore).unwrap();
//     std::os::unix::fs::symlink(data.join("datastore.json"), &datastore).unwrap();
//     let x = tmp.path().to_str().unwrap();
//     println!("tmpdir: {}", x);
//     let output = Command::cargo_bin("migrator")
//         .unwrap()
//         .args(&[
//             "--datastore-path",
//             datastore.to_str().unwrap(),
//             "--migration-directory",
//             "/var/lib/bottlerocket-migrations",
//             "--root-path",
//             root.to_str().unwrap(),
//             "--metadata-directory",
//             "/var/cache/bottlerocket-metadata",
//             "--migrate-to-version",
//             "0.99.1",
//             "--log-level",
//             "trace",
//         ])
//         .output()
//         .unwrap();
//     let stdout = std::str::from_utf8(output.stdout.as_slice()).unwrap();
//     println!("stdout:\n{}", stdout);
//     let stderr = std::str::from_utf8(output.stderr.as_slice()).unwrap();
//     println!("stderr:\n{}", stderr);
//     assert_eq!(output.status.code().unwrap(), 0);
//     // .assert()
//     // .success();
//     //
//     // let args = crate::args::Args {
//     //     datastore_path: PathBuf::from(""),
//     //     log_level: LevelFilter,
//     //     migration_directory: PathBuf::from(""),
//     //     migrate_to_version: Version {
//     //         major: 0,
//     //         minor: 99,
//     //         patch: 1,
//     //         pre: vec![],
//     //         build: vec![],
//     //     },
//     //     root_path: PathBuf::from(""),
//     //     metadata_directory: PathBuf::from(""),
//     // };
// }

/*
/usr/bin/migrator -
--datastore-path
/var/lib/bottlerocket/datastore/current
--migration-directory
/var/lib/bottlerocket-migrations
--root-path
/usr/share/updog/root.json
--metadata-directory
/var/cache/bottlerocket-metadata
--migrate-to-version-from-os-release
--log-level
trace
 */
