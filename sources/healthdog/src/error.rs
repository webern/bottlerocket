//! Provides the list of errors for `healthdog`.

use std::io;
use std::path::PathBuf;

use snafu::Snafu;

#[derive(Debug, Snafu)]
#[snafu(visibility = "pub(crate)")]
pub(crate) enum Error {
    #[snafu(display("Unable to load bottlerocket release info: '{}'", source))]
    BottlerocketRelease { source: bottlerocket_release::Error },

    #[snafu(display("Usage error."))]
    Usage { message: Option<String> },
}

pub(crate) type Result<T> = std::result::Result<T, Error>;
