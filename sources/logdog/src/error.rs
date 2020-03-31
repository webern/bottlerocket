// Copyright 2020 Amazon.com, Inc. or its affiliates. All Rights Reserved.

use std::io;
use std::path::PathBuf;

use snafu::{Backtrace, Snafu};

#[derive(Debug, Snafu)]
#[snafu(visibility = "pub")]
pub enum Error {
    #[snafu(display("Error creating the tarball file '{}': {}", path.display(), source))]
    TarballFileCreate {
        source: io::Error,
        path: PathBuf,
        backtrace: Backtrace,
    },
    #[snafu(display("Error creating the command stdout file '{}': {}", path.display(), source))]
    CommandOutputFile {
        source: io::Error,
        path: PathBuf,
        backtrace: Backtrace,
    },
    #[snafu(display("Error creating the command stderr file '{}': {}", path.display(), source))]
    CommandErrFile {
        source: io::Error,
        path: PathBuf,
        backtrace: Backtrace,
    },
    #[snafu(display("Error creating the error file '{}': {}", path.display(), source))]
    ErrorFile {
        source: io::Error,
        path: PathBuf,
        backtrace: Backtrace,
    },
    #[snafu(display("Error writing to the error file '{}': {}", path.display(), source))]
    ErrorWrite {
        source: io::Error,
        path: PathBuf,
        backtrace: Backtrace,
    },
    #[snafu(display("Error writing to the stdout file '{}': {}", path.display(), source))]
    StdoutWrite {
        source: io::Error,
        path: PathBuf,
        backtrace: Backtrace,
    },
    #[snafu(display("Error writing to the tarball: {}", source))]
    TarballWrite {
        source: io::Error,
        path: PathBuf,
        backtrace: Backtrace,
    },
    #[snafu(display("Error starting command '{}': {}", command, source))]
    CommandSpawn {
        command: String,
        source: io::Error,
        backtrace: Backtrace,
    },
    #[snafu(display("Error completing command '{}': {}", command, source))]
    CommandFinish {
        command: String,
        source: io::Error,
        backtrace: Backtrace,
    },
    #[snafu(display("Error creating tempdir: {}", source))]
    TempDirCreate {
        source: io::Error,
        backtrace: Backtrace,
    },
}

pub type Result<T> = std::result::Result<T, Error>;
