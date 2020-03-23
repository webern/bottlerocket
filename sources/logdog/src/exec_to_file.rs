// Copyright 2019 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::path::PathBuf;
use std::fs::File;
use snafu::ResultExt;
use std::process::{Command, Stdio};

pub struct ExecToFile<'a> {
    pub command: &'a str,
    pub args: Vec<&'a str>,
    pub output_filename: &'a str,
    pub output_dir: PathBuf,
}

pub fn exec_to_file(ex_info: ExecToFile<'_>) -> crate::error::Result<()> {
    let opath = ex_info.output_dir.join(ex_info.output_filename);
    let opath_str = opath.to_str().ok_or(
        crate::error::Error::ErrorMessage {
            message: format!("Unable to build filepath for '{}'",
                             ex_info.output_filename)
        }
    )?;
    let ofile = File::create(opath_str)
        .context(crate::error::FileError { path: opath_str.to_string() })?;
    let efile = ofile.try_clone()
        .context(crate::error::FileError { path: opath_str.to_string() })?;
    Command::new(ex_info.command)
        .args(ex_info.args)
        .stdout(Stdio::from(ofile))
        .stderr(Stdio::from(efile))
        .spawn().context(crate::error::IoError {})?
        .wait_with_output()
        .context(crate::error::IoError {})?;
    Ok(())
}