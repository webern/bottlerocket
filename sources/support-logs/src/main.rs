#![deny(rust_2018_idioms)]

mod error;

use crate::error::Result;

use std::process::{Command, Stdio};
use std::fs::File;
use snafu::ResultExt;
use std::path::PathBuf;
use std::str::FromStr;

fn main() -> Result<()> {
    run(CommandInfo {
        command: "echo",
        args: vec!("arg1", "arg2"),
        output_filename: "x.log",
        output_dir: PathBuf::from_str("/tmp").unwrap(),
    })
}

struct CommandInfo<'a> {
    command: &'a str,
    args: Vec<&'a str>,
    output_filename: &'a str,
    output_dir: PathBuf,
}

fn run(command_info: CommandInfo<'_>) -> Result<()> {
    let opath = command_info.output_dir.join(command_info.output_filename);
    let opath_str = opath.to_str().ok_or(
        error::Error::ErrorMessage {
            message: format!("Unable to build filepath for '{}'",
                             command_info.output_filename)
        }
    )?;
    let ofile = File::create(opath_str)
        .context(crate::error::FileError { path: opath_str.to_string() })?;
    let efile = ofile.try_clone()
        .context(crate::error::FileError { path: opath_str.to_string() })?;
    Command::new(command_info.command)
        .args(command_info.args)
        .stdout(Stdio::from(ofile))
        .stderr(Stdio::from(efile))
        .spawn().context(crate::error::IoError {})?
        .wait_with_output()
        .context(crate::error::IoError {})?;
    Ok(())
}