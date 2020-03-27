// Copyright 2019 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: MIT OR Apache-2.0

/*!
# Introduction

`logdog` is a program that gathers logs from various places on a Bottlerocket host and combines them
into a tarball for easy export.

Usage example:
```
$ logdog
logs are at: /tmp/bottlerocket-logs.tar.gz
```
*/

#![deny(rust_2018_idioms)]

mod create_tarball;
mod error;
mod exec_to_file;
mod temp_dir;

use std::fs::remove_dir_all;
use std::path::{Path, PathBuf};
use std::{env, process};

use create_tarball::create_tarball;
use error::{FileError, IoError, Result};
use exec_to_file::{run_commands, ExecToFile};
use temp_dir::TempDir;

use snafu::{ErrorCompat, ResultExt};

const ERROR_FILENAME: &str = "logdog.errors";
const OUTPUT_FILENAME: &str = "bottlerocket-logs.tar.gz";
const TARBALL_DIRNAME: &str = "bottlerocket-logs";
const TEMPDIR_NAME: &str = "logdog-temp";

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
fn run_program(output: &PathBuf) -> Result<()> {
    let temp_dir_path = env::temp_dir().join(TEMPDIR_NAME);
    if Path::new(&temp_dir_path).exists() {
        remove_dir_all(&temp_dir_path).context(FileError {
            path: temp_dir_path.clone(),
        })?;
    }
    let temp_dir = TempDir::new(temp_dir_path).context(IoError {})?;
    run_commands(create_commands(), &temp_dir.path())?;
    create_tarball(&temp_dir.path(), &output)?;
    println!("logs are at: {}", output.to_string_lossy());
    Ok(())
}

/// Produces the list of commands that we will run on the Bottlerocket host.
fn create_commands() -> Vec<ExecToFile> {
    vec![
        // Get a copy of os-release to tell us the version and build of Bottlerocket.
        ExecToFile {
            command: "cat",
            args: vec!["/etc/os-release"],
            output_filename: "os-release",
        },
        // Get a list of boots that journalctl knows about.
        ExecToFile {
            command: "journalctl",
            args: vec!["--list-boots", "--no-pager"],
            output_filename: "journalctl-list-boots",
        },
        // Get errors only from journalctl.
        ExecToFile {
            command: "journalctl",
            args: vec!["-p", "err", "-a", "--no-pager"],
            output_filename: "journalctl.errors",
        },
        // Get all log lines from journalctl.
        ExecToFile {
            command: "journalctl",
            args: vec!["-a", "--no-pager"],
            output_filename: "journalctl.log",
        },
        // Get signpost status to tell us the status of grub and the boot partitions.
        ExecToFile {
            command: "signpost",
            args: vec!["status"],
            output_filename: "signpost",
        },
        // Get Bottlerocket settings using the apiclient.
        ExecToFile {
            command: "apiclient",
            args: vec!["--method", "GET", "--uri", "/settings"],
            output_filename: "settings.json",
        },
        // Get networking status with wicked.
        ExecToFile {
            command: "wicked",
            args: vec!["show", "all"],
            output_filename: "wicked",
        },
        // Get configuration info from containerd.
        ExecToFile {
            command: "containerd",
            args: vec!["config", "dump"],
            output_filename: "containerd-config",
        },
        // Get the status of kubelet and other kube processes from systemctl.
        ExecToFile {
            command: "systemctl",
            args: vec!["status", "kube*", "-l", "--no-pager"],
            output_filename: "kube-status",
        },
        // Get the kernel message buffer with dmesg.
        ExecToFile {
            command: "dmesg",
            args: vec!["--color=never", "--nopager"],
            output_filename: "dmesg",
        },
        // Get firewall filtering information with iptables.
        ExecToFile {
            command: "iptables",
            args: vec!["-nvL", "-t", "filter"],
            output_filename: "iptables-filter",
        },
        // Get firewall nat information with iptables.
        ExecToFile {
            command: "iptables",
            args: vec!["-nvL", "-t", "nat"],
            output_filename: "iptables-nat",
        },
    ]
}

fn main() -> ! {
    let output = parse_args(env::args());
    process::exit(match run_program(&output) {
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
    use uuid::Uuid;

    #[test]
    fn test_program() {
        let output_tempdir =
            TempDir::new(std::env::temp_dir().join(Uuid::new_v4().to_string())).unwrap();
        let output_filepath = output_tempdir.path().join("logstest");

        // This should work on any system, even if the underlying programs being called are absent.
        run_program(&output_filepath).unwrap();

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
