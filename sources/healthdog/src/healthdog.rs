use crate::config::Config;
use crate::error::{self, Result};
use crate::healthcheck::{ServiceCheck, SystemdCheck};
use bottlerocket_release::BottlerocketRelease;
use snafu::ResultExt;

pub(crate) struct Healthdog {
    config: Config,
    os_release: BottlerocketRelease,
    healthcheck: Box<dyn ServiceCheck>,
}

impl Healthdog {
    /// Create a new instance by parsing the os-release and healthdog.toml files from their default
    /// locations, and constructing a SystemdCheck object.
    pub(crate) fn new() -> Result<Self> {
        Self::from_parts(None, None, None)
    }

    /// Create a new instance by optionally passing in the `Config`, the `BottlerocketRelease`, and
    /// `SystemCheck` objects. For each of these, if `None` is passed, then the default is used.
    pub(crate) fn from_parts(
        config: Option<Config>,
        os_release: Option<BottlerocketRelease>,
        healthcheck: Option<Box<dyn ServiceCheck>>,
    ) -> Result<Self> {
        Ok(Self {
            config: config.unwrap_or(Config::new()?),
            os_release: os_release
                .unwrap_or(BottlerocketRelease::new().context(error::BottlerocketRelease)?),
            healthcheck: healthcheck.unwrap_or(Box::new(SystemdCheck {})),
        })
    }
}
