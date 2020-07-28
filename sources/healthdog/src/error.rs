//! Provides the list of errors for `healthdog`.

use snafu::Snafu;
use std::path::PathBuf;

#[derive(Debug, Snafu)]
#[snafu(visibility = "pub(crate)")]
pub(crate) enum Error {
    #[snafu(display("Unable to load bottlerocket release info: '{}'", source))]
    BottlerocketRelease { source: bottlerocket_release::Error },

    #[snafu(display("Failed to parse config file {}: {}", path.display(), source))]
    ConfigParse {
        path: PathBuf,
        source: toml::de::Error,
    },

    #[snafu(display("Failed to read config file {}: {}", path.display(), source))]
    ConfigRead {
        path: PathBuf,
        source: std::io::Error,
    },

    #[snafu(display("Usage error."))]
    Usage { message: Option<String> },

    #[snafu(display("Unable to parse URL {}: {}", url, source))]
    UrlParse {
        url: String,
        source: url::ParseError,
    },
}

pub(crate) type Result<T> = std::result::Result<T, Error>;
