// Copyright 2019 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::path::PathBuf;
use std::fs::File;
use snafu::ResultExt;
use std::process::{Command, Stdio};
use crate::error::Result;
use std::io::Write;

#[derive(Debug, Clone)]
pub(crate) struct ExecToFile {
    pub command: &'static str,
    pub args: Vec<&'static str>,
    pub output_filename: &'static str,
}

impl ExecToFile {
    pub(crate) fn run(&self, tempdir: &PathBuf) -> crate::error::Result<()> {
        let opath = tempdir.join(self.output_filename);
        let ofile = File::create(&opath)
            .context(crate::error::FileError { path: opath.clone() })?;
        let efile = ofile.try_clone()
            .context(crate::error::FileError { path: opath.clone() })?;
        Command::new(self.command)
            .args(&self.args)
            .stdout(Stdio::from(ofile))
            .stderr(Stdio::from(efile))
            .spawn().context(crate::error::IoError {})?
            .wait_with_output()
            .context(crate::error::IoError {})?;
        Ok(())
    }
}

pub(crate) fn run_commands(commands: Vec<crate::exec_to_file::ExecToFile>, outdir: &PathBuf) -> Result<()> {
    let error_path = outdir.join(crate::ERROR_FILENAME);
    let mut error_file = File::create(&error_path)
        .context(crate::error::FileError { path: error_path.clone() })?;
    for ex in commands.iter() {
        if let Err(e) = ex.run(&outdir) {
            error_file.write(
                format!(
                    "Error running command '{:?}': '{}'",
                    ex.clone(),
                    e
                ).into_bytes().as_slice()
            ).context(crate::error::FileError { path: error_path.clone() })?;
        }
    }
    Ok(())
}