// Copyright 2019 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: MIT OR Apache-2.0

#![deny(rust_2018_idioms)]

mod error;
mod exec_to_file;
mod create_tarball;

use crate::error::Result;
use snafu::ErrorCompat;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct ProgramArgs {
    /// The compressed output file that will be written containing all of the support logs.
    #[structopt(short = "o", long = "output", default_value = "/tmp/logdog.tar.gz")]
    output: PathBuf,

    /// The temporary working directory where log files will be written before being aggregated.
    #[structopt(short = "t", long = "tempdir", default_value = "/tmp/logdog")]
    tempdir: PathBuf,
}

fn main() -> ! {
    let args = ProgramArgs::from_args();
    std::process::exit(match run_program(&args) {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("{}", err);
            if let Some(var) = std::env::var_os("RUST_BACKTRACE") {
                if var != "0" {
                    if let Some(backtrace) = err.backtrace() {
                        eprintln!("\n{:?}", backtrace);
                    }
                }
            }
            1
        }
    })
}

fn run_program(args: &ProgramArgs) -> Result<()> {
    // TODO - delete tempdir if exists
    // TODO - create tempdir (using a self-cleaning tempdir object)
    // TODO - run many actual commands instead of this single echo command
    crate::exec_to_file::exec_to_file(make_fake_command(&args.tempdir))?;
    crate::create_tarball::create_tarball(&args.tempdir, &args.output)
    // TODO - tell the customer where the tarball is
}

fn make_fake_command(tempdir: &PathBuf) -> crate::exec_to_file::ExecToFile<'static> {
    crate::exec_to_file::ExecToFile {
        command: "echo",
        args: vec!("arg1", "arg2"),
        output_filename: "fake-stuff.log",
        output_dir: tempdir.clone(),
    }
}