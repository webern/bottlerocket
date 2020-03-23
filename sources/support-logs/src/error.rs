// Copyright 2019 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: MIT OR Apache-2.0

use snafu::{Snafu, Backtrace};
use std::io;

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
}

pub type Result<T> = std::result::Result<T, Error>;
