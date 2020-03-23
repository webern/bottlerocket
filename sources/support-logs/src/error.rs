use snafu::{ResultExt, Snafu};
use std::path::PathBuf;
use std::io;

#[derive(Debug, Snafu)]
#[snafu(visibility = "pub")]
pub enum Error {
    #[snafu(display("Something happened: '{}': {}", message, source))]
    SomethingHappened {
        message: String,
        source: std::io::Error,
    },
    #[snafu(display("Error: '{}'", message))]
    ErrorMessage {
        message: String,
    },
    #[snafu(display("File error '{}': {}", path, source))]
    FileError {
        source: io::Error,
        path: String,
    },
}

pub type Result<T> = std::result::Result<T, Error>;
