use crate::args::DEFAULT_CONFIG_PATH;
use crate::error::Result;
use std::path::{Path, PathBuf};

pub(crate) struct Config {}

impl Config {
    pub(crate) fn new() -> Result<Self> {
        Self::from_file(PathBuf::from(DEFAULT_CONFIG_PATH))
    }

    pub(crate) fn from_file<P: AsRef<Path>>(_file: P) -> Result<Self> {
        Ok(Config {})
    }
}
