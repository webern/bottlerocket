// Copyright 2019 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::error::{Result, FileError, IoError};
use snafu::ResultExt;

/// Aggregates the information needed to run a shell command and pipe it to a file.
#[derive(Debug, Clone)]
pub(crate) struct ExecToFile {
    pub command: &'static str,
    pub args: Vec<&'static str>,
    pub output_filename: &'static str,
}

impl ExecToFile {
    /// Runs a shell command and pipes its output to a named file in the specified outdir.
    pub(crate) fn run(&self, outdir: &PathBuf) -> Result<()> {
        let opath = outdir.join(self.output_filename);
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

/// Runs a list of commands and pipes all of them into the same outdir. If a command raises an error,
/// `run_commands` pipes that error to a file and continues without failing or returning the error.
pub(crate) fn run_commands(commands: Vec<ExecToFile>, outdir: &PathBuf) -> Result<()> {
    // if a command fails, we will pipe its error here and continue.
    let error_path = outdir.join(crate::ERROR_FILENAME);
    let mut error_file = File::create(&error_path)
        .context(FileError { path: error_path.clone() })?;

    for ex in commands.iter() {
        if let Err(e) = ex.run(&outdir) {
            // ignore the error, but make note of it in the error file.
            error_file.write(
                format!("Error running command '{:?}': '{}'\n", ex.clone(), e)
                    .into_bytes().as_slice()
            ).context(FileError { path: error_path.clone() })?;
        }
    }
    Ok(())
}