// Copyright 2019 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: MIT OR Apache-2.0

#![deny(rust_2018_idioms)]

mod error;
mod exec_to_file;
mod create_tarball;

use crate::error::Result;
use snafu::ErrorCompat;
use std::path::PathBuf;
use std::{env, process};

pub struct ProgramArgs {
    output: PathBuf,
    tempdir: PathBuf,
}

/// Print a usage message in the event a bad arg is passed
fn usage() -> ! {
    let program_name = env::args().next().unwrap_or_else(|| "program".to_string());
    eprintln!(
        r"Usage: {}
            [ --tempdir PATH directory to write logs to temporarily before zipping ]
            [ --output PATH file to write zipped logs to ]
",
        program_name,
    );
    process::exit(2);
}

/// Prints a more specific message before exiting through usage().
fn usage_msg(msg: &str) -> ! {
    eprintln!("{}\n", msg);
    usage();
}

/// Parses the command line arguments and provides defaults for those that are optional.
fn parse_args(args: env::Args) -> ProgramArgs {
    let mut tempdir_arg_str = None;
    let mut output_arg_str = None;

    let mut iter = args.skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_ref() {
            "--tempdir" => {
                tempdir_arg_str = Some(
                    iter.next()
                        .unwrap_or_else(|| usage_msg("Did not give argument to --tempdir")),
                )
            }

            "--output" => {
                output_arg_str = Some(
                    iter.next()
                        .unwrap_or_else(|| usage_msg("Did not give argument to --output")),
                )
            }

            _ => usage(),
        }
    }

    // TODO - handle None for these with default values
    ProgramArgs {
        tempdir: PathBuf::from(tempdir_arg_str.unwrap()),
        output: PathBuf::from(output_arg_str.unwrap()),
    }
}

fn main() -> ! {
    let args = parse_args(env::args());
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