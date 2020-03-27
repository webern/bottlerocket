// Copyright 2019 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fs::File;
use std::path::PathBuf;

use flate2::write::GzEncoder;
use flate2::Compression;
use snafu::ResultExt;
use tar;

// Creates a tarball with all the contents of directory `dir`.
// Outputs as file to `outfile`.
pub(crate) fn create_tarball(dir: &PathBuf, outfile: &PathBuf) -> crate::error::Result<()> {
    let tarfile = File::create(outfile).context(crate::error::FileError {
        path: outfile.to_str().unwrap(),
    })?;
    let encoder = GzEncoder::new(tarfile, Compression::default());
    let mut tarball = tar::Builder::new(encoder);
    tarball
        .append_dir_all(crate::TARBALL_DIRNAME, dir.to_str().unwrap())
        .context(crate::error::IoError {})
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use uuid::Uuid;

    use super::*;
    use flate2::read::GzDecoder;
    use std::io::Write;
    use tar::Archive;

    #[test]
    fn tarball_test() {
        let inpath = std::env::temp_dir().join(Uuid::new_v4().to_string());
        let _indir = crate::TempDir::new(inpath.clone()).unwrap();
        let outpath = std::env::temp_dir().join(Uuid::new_v4().to_string());
        let outdir = crate::TempDir::new(outpath).unwrap();
        let outfilepath = outdir.path().join("somefile.tar.gz");
        let mut file = File::create(inpath.join("hello.txt")).unwrap();
        file.write_all(b"Hello World!").unwrap();
        drop(file);
        create_tarball(&inpath, &outfilepath).unwrap();
        assert!(Path::new(&outfilepath).is_file());
        let tar_gz = File::open(outfilepath).unwrap();
        let tar = GzDecoder::new(tar_gz);
        let mut archive = Archive::new(tar);
        let mut entries = archive.entries().unwrap();

        let entry = entries.next().unwrap().unwrap();
        let actual_path = PathBuf::from(entry.path().unwrap());
        let expected_path = PathBuf::from(crate::TARBALL_DIRNAME);
        assert!(actual_path == expected_path);

        let entry = entries.next().unwrap().unwrap();
        let actual_path = PathBuf::from(entry.path().unwrap());
        let expected_path = PathBuf::from(crate::TARBALL_DIRNAME).join("hello.txt");
        assert!(actual_path == expected_path);
    }
}
