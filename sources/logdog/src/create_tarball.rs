// Copyright 2019 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fs::File;
use std::path::PathBuf;

use flate2::write::GzEncoder;
use flate2::Compression;
use snafu::ResultExt;
use tar;

pub fn create_tarball(tempdir: &PathBuf, outfile: &PathBuf) -> crate::error::Result<()> {
    let tarfile = File::create(outfile).context(crate::error::FileError {
        path: outfile.to_str().unwrap(),
    })?;
    let encoder = GzEncoder::new(tarfile, Compression::default());
    let mut tarball = tar::Builder::new(encoder);
    tarball
        .append_dir_all(crate::TARBALL_DIRNAME, tempdir.to_str().unwrap())
        .context(crate::error::IoError {})
}
