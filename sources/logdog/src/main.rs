// Copyright 2020 Amazon.com, Inc. or its affiliates. All Rights Reserved.

/*!
# Introduction

`logdog` is a program that gathers logs from various places on a Bottlerocket host and combines them
into a tarball for easy export.

Usage example:
```
$ logdog
logs are at: /tmp/bottlerocket-logs.tar.gz
```

# Logs

For the commands used to gather logs, please see [commands.rs](src/commands.rs).

*/

#![deny(rust_2018_idioms)]

mod commands;
mod create_tarball;
mod error;
mod exec_to_file;

use std::path::PathBuf;
use std::{env, process};

use commands::commands;
use create_tarball::create_tarball;
use error::Result;
use exec_to_file::run_commands;
use tempfile::TempDir;

use snafu::{ErrorCompat, ResultExt};

const ERROR_FILENAME: &str = "logdog.errors";
const OUTPUT_FILENAME: &str = "bottlerocket-logs.tar.gz";
const TARBALL_DIRNAME: &str = "bottlerocket-logs";

/// Prints a usage message in the event a bad arg is passed.
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
        None => env::temp_dir().as_path().join(OUTPUT_FILENAME),
    }
}

/// Runs the bulk of the program's logic, main wraps this.
fn run(output: &PathBuf) -> Result<()> {
    let temp_dir = TempDir::new().context(error::TempDirCreate)?;
    run_commands(commands(), &temp_dir.path().to_path_buf())?;
    create_tarball(&temp_dir.path().to_path_buf(), &output)?;
    println!("logs are at: {}", output.to_string_lossy());
    Ok(())
}

fn main() -> ! {
    let output = parse_args(env::args());
    process::exit(match run(&output) {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("{}", err);
            if let Some(var) = env::var_os("RUST_BACKTRACE") {
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

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::GzDecoder;
    use std::fs::File;
    use tar::Archive;

    #[test]
    fn test_program() {
        let output_tempdir = TempDir::new().unwrap();
        let output_filepath = output_tempdir.path().join("logstest");

        // This should work on any system, even if the underlying programs being called are absent.
        run(&output_filepath).unwrap();

        // Open the file and spot check that a couple of expected files exist inside it.
        // This function will panic if the path is not found in the tarball
        let find = |path_to_find: &PathBuf| {
            let tar_gz = File::open(&output_filepath).unwrap();
            let tar = GzDecoder::new(tar_gz);
            let mut archive = Archive::new(tar);
            let mut entries = archive.entries().unwrap();
            let _found = entries
                .find(|item| {
                    let entry = item.as_ref().clone().unwrap();
                    let path = entry.path().unwrap();
                    PathBuf::from(path) == PathBuf::from(path_to_find)
                })
                .unwrap()
                .unwrap();
        };

        // These assert that the provided paths exist in the tarball
        find(&PathBuf::from(TARBALL_DIRNAME));
        find(&PathBuf::from(TARBALL_DIRNAME).join("os-release"));
        find(&PathBuf::from(TARBALL_DIRNAME).join("journalctl.log"));
    }
}
