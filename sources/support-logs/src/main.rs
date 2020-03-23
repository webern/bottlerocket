#![deny(rust_2018_idioms)]

mod error;

use crate::error::Result;
// use crate::error::Result;

use std::process::{Command, Stdio};
use std::fs::File;
use snafu::ResultExt;
// use crate::error::FileError;
use std::path::PathBuf;
use std::str::FromStr;
use std::fs;

// const TEST_SCRIPT: &str = r####"#!/bin/bash
// echo "1 stdout"
// >&2 echo "2 stderr"
// echo "3 stdout"
// >&2 echo "4 stderr"
// echo "5 stdout"
// "####;

fn main() -> Result<()> {
    // let outputs = File::create("out.txt").context(crate::error::FileError { path: "out.txt".to_string() })?;
    // let errors = outputs.try_clone().context(crate::error::FileError { path: "out.txt".to_string() })?;

    let pbuf = PathBuf::from_str("./test-script.sh").unwrap();
    let pbuf = fs::canonicalize(&pbuf).unwrap();

    // Command::new("/bin/bash")
    //     .args(&["-c", pbuf.to_str().unwrap()])
    //     .stdout(Stdio::from(outputs))
    //     .stderr(Stdio::from(errors))
    //     .spawn().context(crate::error::FileError { path: "out.txt".to_string() })?
    //     .wait_with_output().context(crate::error::FileError { path: "out.txt".to_string() })?;

    run(CommandInfo {
        command: "/bin/bash",
        args: vec!("-c", pbuf.to_str().unwrap()),
        output_filename: "x.log",
        output_dir: PathBuf::from_str(".").unwrap(),
    })
    // Ok(())
}

struct CommandInfo<'a> {
    command: &'a str,
    args: Vec<&'a str>,
    output_filename: &'a str,
    output_dir: PathBuf,
}

// const PATH_MESSAGE: &str = "Unable to build filepath for '{}'";

fn run<'a>(command_info: CommandInfo<'a>) -> Result<()> {
    let opath = command_info.output_dir.join(command_info.output_filename);
    // let opath = opath.canonicalize().context(error::FileError { path: opath.to_str().unwrap() })?;
    let opath_str = opath.to_str().ok_or(error::Error::ErrorMessage { message: format!("Unable to build filepath for '{}'", command_info.output_filename).to_string() })?;
    let ofile = File::create(opath_str).context(crate::error::FileError { path: opath_str.to_string() })?;
    let efile = ofile.try_clone().context(crate::error::FileError { path: opath_str.to_string() })?;
    Command::new(command_info.command)
        .args(command_info.args)
        .stdout(Stdio::from(ofile))
        .stderr(Stdio::from(efile))
        .spawn().context(crate::error::IoError {})?
        .wait_with_output()
        .context(crate::error::IoError {})?;
    Ok(())
}