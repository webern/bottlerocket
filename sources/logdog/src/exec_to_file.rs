// Copyright 2020 Amazon.com, Inc. or its affiliates. All Rights Reserved.

use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::error;
use crate::error::Result;
use snafu::ResultExt;

/// Provides a structure and functions for running commands and saving the output to a file.

/// Aggregates the information needed to run a shell command and write its output to a file.
#[derive(Debug, Clone)]
pub(crate) struct ExecToFile {
    pub(crate) command: &'static str,
    pub(crate) args: Vec<&'static str>,
    pub(crate) output_filename: &'static str,
}

impl ExecToFile {
    /// Runs a command specified in an `ExecToFile` and writes its output to a file in the specified `outdir`.
    pub(crate) fn run<P: AsRef<Path>>(&self, outdir: P) -> Result<()> {
        let opath = outdir.as_ref().join(self.output_filename);
        let ofile = File::create(&opath).context(error::CommandOutputFile {
            path: opath.clone(),
        })?;
        let efile = ofile
            .try_clone()
            .context(error::CommandErrFile { path: opath })?;
        Command::new(self.command)
            .args(&self.args)
            .stdout(Stdio::from(ofile))
            .stderr(Stdio::from(efile))
            .spawn()
            .context(error::CommandSpawn {
                command: self.to_string(),
            })?
            .wait_with_output()
            .context(error::CommandFinish {
                command: self.to_string(),
            })?;
        Ok(())
    }
}

impl ToString for ExecToFile {
    fn to_string(&self) -> String {
        format!("{} {}", self.command, self.args.join(" "))
    }
}

/// Runs a list of commands and writes all of their output into files in the same `outdir`.  Any
/// failures are noted in the file named by ERROR_FILENAME.  This function ignores the commands'
/// return status and only fails if we can't save our own errors.
pub(crate) fn run_commands<P: AsRef<Path>>(commands: Vec<ExecToFile>, outdir: P) -> Result<()> {
    // if a command fails, we will pipe its error here and continue.
    let error_path = outdir.as_ref().join(crate::ERROR_FILENAME);
    let mut error_file = File::create(&error_path).context(error::ErrorFile {
        path: error_path.clone(),
    })?;

    for ex in commands.iter() {
        if let Err(e) = ex.run(outdir.as_ref()) {
            // ignore the error, but make note of it in the error file.
            write!(
                &mut error_file,
                "Error running command '{:?}': '{}'\n",
                ex.to_string(),
                e
            )
            .context(error::ErrorWrite {
                path: error_path.clone(),
            })?;
        }
    }
    Ok(())
}
