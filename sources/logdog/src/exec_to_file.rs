// Copyright 2019 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::error::{Result, FileError, IoError};
use snafu::ResultExt;

#[derive(Debug, Clone)]
pub(crate) struct ExecToFile {
    pub command: &'static str,
    pub args: Vec<&'static str>,
    pub output_filename: &'static str,
}

impl ExecToFile {
    pub(crate) fn run(&self, tempdir: &PathBuf) -> Result<()> {
        let opath = tempdir.join(self.output_filename);
        let mut ofile = File::create(&opath)
            .context(FileError { path: opath.clone() })?;
        let efile = ofile.try_clone()
            .context(FileError { path: opath.clone() })?;
        ofile.write(format!("{:?}\n", self).into_bytes().as_slice())
            .context(FileError { path: opath.clone() })?;
        Command::new(self.command)
            .args(&self.args)
            .stdout(Stdio::from(ofile))
            .stderr(Stdio::from(efile))
            .spawn().context(IoError {})?
            .wait_with_output()
            .context(IoError {})?;
        Ok(())
    }
}

pub(crate) fn run_commands(commands: Vec<ExecToFile>, outdir: &PathBuf) -> Result<()> {
    let error_path = outdir.join(crate::ERROR_FILENAME);
    let mut error_file = File::create(&error_path)
        .context(FileError { path: error_path.clone() })?;
    for ex in commands.iter() {
        if let Err(e) = ex.run(&outdir) {
            error_file.write(
                format!(
                    "Error running command '{:?}': '{}'\n",
                    ex.clone(),
                    e
                ).into_bytes().as_slice()
            ).context(FileError { path: error_path.clone() })?;
        }
    }
    Ok(())
}