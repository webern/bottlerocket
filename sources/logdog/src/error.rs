// Copyright 2019 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: MIT OR Apache-2.0

use snafu::{Snafu, Backtrace};
use std::io;
use crate::exec_to_file::ExecToFile;

#[derive(Debug, Snafu)]
#[snafu(visibility = "pub")]
pub enum Error {
    #[snafu(display("Error: '{}'", message))]
    ErrorMessage {
        message: String,
    },
    #[snafu(display("File error '{}': {}", path, source))]
    FileError {
        source: io::Error,
        path: String,
        backtrace: Backtrace,
    },
    #[snafu(display("IO error: {}", source))]
    IoError {
        source: io::Error,
        backtrace: Backtrace,
    },
    // #[snafu(display("Error while executing command '{:?}': '{}'", command, source))]
    // CommandError {
    //     source: io::Error,
    //     command: ExecToFile,
    // },
}

pub type Result<T> = std::result::Result<T, Error>;
