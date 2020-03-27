// Copyright 2020 Amazon.com, Inc. or its affiliates. All Rights Reserved.

use std::io;
use std::path::PathBuf;

use snafu::{Backtrace, Snafu};

#[derive(Debug, Snafu)]
#[snafu(visibility = "pub")]
pub enum Error {
    #[snafu(display("File error '{}': {}", path.to_string_lossy(), source))]
    File {
        source: io::Error,
        path: PathBuf,
        backtrace: Backtrace,
    },
    #[snafu(display("IO error: {}", source))]
    Io {
        source: io::Error,
        backtrace: Backtrace,
    },
}

pub type Result<T> = std::result::Result<T, Error>;
