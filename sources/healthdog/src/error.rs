//! Provides the list of errors for `healthdog`.

use snafu::Snafu;
use std::path::PathBuf;
use url::Url;

#[derive(Debug, Snafu)]
#[snafu(visibility = "pub(crate)")]
pub(crate) enum Error {
    #[snafu(display("Unable to load Bottlerocket release info: '{}'", source))]
    BottlerocketRelease { source: bottlerocket_release::Error },

    #[snafu(display("Command '{}' with args '{:?}' failed: {}", command, args, source))]
    Command {
        command: String,
        args: Vec<String>,
        source: std::io::Error,
    },

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

    #[snafu(display("Error building HTTP client for {}: {}", url.as_str(), source))]
    HttpClient { url: Url, source: reqwest::Error },

    #[snafu(display("Error receiving response {}: {}", url.as_str(), source))]
    HttpResponse { url: Url, source: reqwest::Error },

    #[snafu(display("Unable to parse '{}' to an int: '{}'", value, source))]
    IntParse {
        value: String,
        source: std::num::ParseIntError,
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
