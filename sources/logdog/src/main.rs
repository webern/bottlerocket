// Copyright 2019 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: MIT OR Apache-2.0

#![deny(rust_2018_idioms)]

mod error;
mod exec_to_file;
mod create_tarball;

use crate::error::Result;
use snafu::{ErrorCompat, ResultExt};
use std::path::PathBuf;
use std::{env, process};
use drop_dir;
use crate::exec_to_file::{ExecToFile, run_commands};

const TEMP_SUBDIR_NANE: &str = "logdog-temp";
const OUTPUT_FILENAME: &str = "bottlerocket-logs.tar.gz";
const ERROR_FILENAME: &str = "logdog.errors";
const TARBALL_DIRNAME: &str = "bottlerocket-logs";

/// Prints a usage message in the event a bad arg is passed
fn usage() -> ! {
    let program_name = env::args().next().unwrap_or_else(|| "program".to_string());
    eprintln!(
        r"Usage: {}
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

/// Parses the command line arguments.
fn parse_args(args: env::Args) -> PathBuf {
    let mut output_arg = None;
    let mut iter = args.skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_ref() {
            "--output" => {
                output_arg = Some(
                    iter.next()
                        .unwrap_or_else(|| usage_msg("Did not give argument to --output")),
                )
            }

            _ => usage(),
        }
    }

    match output_arg {
        Some(path) => PathBuf::from(path),
        None => std::env::temp_dir().as_path().join(OUTPUT_FILENAME),
    }
}

fn run_program(output: PathBuf) -> Result<()> {
    let temp_dir_path = std::env::temp_dir().join(TEMP_SUBDIR_NANE);
    if std::path::Path::new(&temp_dir_path).exists() {
        std::fs::remove_dir_all(&temp_dir_path)
            .context(crate::error::FileError { path: temp_dir_path.clone() })?;
    }
    let temp_dir = drop_dir::DropDir::new(temp_dir_path)
        .context(crate::error::IoError {})?;
    run_commands(create_commands(), &temp_dir.path())?;
    crate::create_tarball::create_tarball(&temp_dir.path(), &output)?;
    println!("logs are at: {}", output.to_string_lossy());
    Ok(())
}

// Produces the list of commands that we will run on the Bottlerocket host.
fn create_commands() -> Vec<ExecToFile> {
    vec!(
        ExecToFile {
            command: "cat",
            args: vec!["/etc/os-release"],
            output_filename: "os-release",
        },
        ExecToFile {
            command: "journalctl",
            args: vec!["-p", "3", "--no-pager"],
            output_filename: "journalctl-list-boots",
        },
        ExecToFile {
            command: "journalctl",
            args: vec!["-p", "err", "-a", "--no-pager"],
            output_filename: "journalctl.errors",
        },
        ExecToFile {
            command: "journalctl",
            args: vec!["-a", "--no-pager"],
            output_filename: "journalctl.log",
        },
        ExecToFile {
            command: "signpost",
            args: vec!["status"],
            output_filename: "signpost",
        },
        ExecToFile {
            command: "apiclient",
            args: vec!["--method", "GET", "--uri", "/settings"],
            output_filename: "settings.json",
        },
        ExecToFile {
            command: "wicked",
            args: vec!["show", "all"],
            output_filename: "wicked",
        },
        ExecToFile {
            command: "containerd",
            args: vec!["config", "dump"],
            output_filename: "containerd-config",
        },
        ExecToFile {
            command: "systemctl",
            args: vec!["status", "kube*", "-l", "--no-pager"],
            output_filename: "kube-status",
        },
        ExecToFile {
            command: "dmesg",
            args: vec!["--color", "never", "--nopager"],
            output_filename: "dmesg",
        }
    )
}


fn main() -> ! {
    let output = parse_args(env::args());
    std::process::exit(match run_program(output) {
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
