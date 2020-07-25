use crate::config::Config;
use crate::error::{self, Result};
use bottlerocket_release::BottlerocketRelease;
use snafu::ResultExt;

pub(crate) struct Healthdog {
    config: Config,
    os_release: BottlerocketRelease,
}

impl Healthdog {
    /// Create a new instance by parsing the os-release and healthdog.toml files in their default
    /// locations.
    pub(crate) fn new() -> Result<Self> {
        Self::from_parts(None, None)
    }

    /// Create a new instance by optionally passing in the `Config`, the `BottlerocketRelease`, or
    /// both. For each of these, if `None` is passed, then the default file location is used.
    pub(crate) fn from_parts(
        config: Option<Config>,
        os_release: Option<BottlerocketRelease>,
    ) -> Result<Self> {
        Ok(Self {
            config: if let Some(cfg) = config {
                cfg
            } else {
                Config::new()?
            },
            os_release: if let Some(osr) = os_release {
                osr
            } else {
                BottlerocketRelease::new().context(error::BottlerocketRelease)?
            },
        })
    }
}
