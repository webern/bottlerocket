// Copyright 2019 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::path::PathBuf;
use std::fs::File;
use snafu::ResultExt;
use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub(crate) struct ExecToFile {
    pub command: &'static str,
    pub args: Vec<&'static str>,
    pub output_filename: &'static str,
}

impl ExecToFile {
    pub(crate) fn run(&self, tempdir: &PathBuf) -> crate::error::Result<()> {
        let opath = tempdir.join(self.output_filename);
        // let opath_str = opath.to_str().ok_or(
        //     crate::error::Error::ErrorMessage {
        //         message: format!("Unable to build filepath for '{}'",
        //                          ex_info.output_filename)
        //     }
        // )?;
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

// pub(crate) fn exec_to_file(ex_info: ExecToFile<'_>, tempdir: &PathBuf) -> crate::error::Result<()> {
//     let opath = tempdir.join(ex_info.output_filename);
//     let opath_str = opath.to_str().ok_or(
//         crate::error::Error::ErrorMessage {
//             message: format!("Unable to build filepath for '{}'",
//                              ex_info.output_filename)
//         }
//     )?;
//     let ofile = File::create(opath_str)
//         .context(crate::error::FileError { path: opath_str.to_string() })?;
//     let efile = ofile.try_clone()
//         .context(crate::error::FileError { path: opath_str.to_string() })?;
//     Command::new(ex_info.command)
//         .args(ex_info.args)
//         .stdout(Stdio::from(ofile))
//         .stderr(Stdio::from(efile))
//         .spawn().context(crate::error::IoError {})?
//         .wait_with_output()
//         .context(crate::error::IoError {})?;
//     Ok(())
// }