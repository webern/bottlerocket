// Copyright 2020 Amazon.com, Inc. or its affiliates. All Rights Reserved.

use crate::error;
use crate::error::Result;
use std::fs::File;
use std::path::Path;

use flate2::write::GzEncoder;
use flate2::Compression;
use snafu::ResultExt;
use tar;

// Creates a tarball with all the contents of directory `dir`.
// Outputs as file to `outfile`.
pub(crate) fn create_tarball<P1, P2>(dir: P1, outfile: P2) -> Result<()>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>,
{
    let tarfile = File::create(outfile.as_ref()).context(error::TarballFileCreate {
        path: outfile.as_ref(),
    })?;
    let encoder = GzEncoder::new(tarfile, Compression::default());
    let mut tarball = tar::Builder::new(encoder);
    tarball
        .append_dir_all(crate::TARBALL_DIRNAME, dir.as_ref())
        .context(error::TarballWrite {
            path: outfile.as_ref(),
        })
    // TODO correctly close the tarball file
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Write;
    use std::path::{Path, PathBuf};

    use flate2::read::GzDecoder;
    use tar::Archive;
    use tempfile::TempDir;

    #[test]
    fn tarball_test() {
        // create an input directory with one file in it.
        let indir = TempDir::new().unwrap();
        let mut file = File::create(indir.path().to_path_buf().join("hello.txt")).unwrap();
        file.write_all(b"Hello World!").unwrap();
        drop(file);

        // create an output directory into which our function will produce a tarball.
        let outdir = TempDir::new().unwrap();
        let outfilepath = outdir.path().join("somefile.tar.gz");

        // run the function under test.
        create_tarball(&indir.path().to_path_buf(), &outfilepath).unwrap();

        // assert that the output tarball exists.
        assert!(Path::new(&outfilepath).is_file());

        // open the output tarball and check that it has the expected top level directory in it.
        let tar_gz = File::open(outfilepath).unwrap();
        let tar = GzDecoder::new(tar_gz);
        let mut archive = Archive::new(tar);
        let mut entries = archive.entries().unwrap();
        let entry = entries.next().unwrap().unwrap();
        let actual_path = PathBuf::from(entry.path().unwrap());
        let expected_path = PathBuf::from(crate::TARBALL_DIRNAME);
        assert!(actual_path == expected_path);

        // check that the tarball also contains our hello.txt file.
        let entry = entries.next().unwrap().unwrap();
        let actual_path = PathBuf::from(entry.path().unwrap());
        let expected_path = PathBuf::from(crate::TARBALL_DIRNAME).join("hello.txt");
        assert!(actual_path == expected_path);
    }
}
