// Copyright 2019 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: MIT OR Apache-2.0

#![deny(rust_2018_idioms)]

mod error;
mod exec_to_file;

use crate::error::Result;
use snafu::ErrorCompat;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct ProgramArgs {
    /// The compressed output file that will be written containing all of the support logs.
    #[structopt(short = "o", long = "output", default_value = "/tmp/support-logs.tar.gz")]
    output: PathBuf,

    /// The temporary working directory where log files will be written before being aggregated.
    #[structopt(short = "t", long = "tempdir", default_value = "/tmp/support-logs")]
    tempdir: PathBuf,
}

// TODO: https://rust-lang-nursery.github.io/rust-cookbook/compression/tar.html

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

fn run_program(args: &ProgramArgs) -> Result<()> { crate::exec_to_file::exec_to_file(make_fake_command(args)) }

fn make_fake_command(args: &ProgramArgs) -> crate::exec_to_file::ExecToFile<'static> {
    crate::exec_to_file::ExecToFile {
        command: "echo",
        args: vec!("arg1", "arg2"),
        output_filename: "fake-stuff.log",
        output_dir: args.tempdir.clone(),
    }
}