// Copyright 2019 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::error;
use std::path::PathBuf;

pub fn create_tarball(tempdir: &PathBuf, outfile: &PathBuf) -> crate::error::Result<()> {
    // TODO - run many actual commands instead of this single echo command
    Err(crate::error::Error::ErrorMessage { message: "not implemented".into() })
}
