use crate::config::Config;
use crate::error::Error::UrlParse;
use crate::error::{self, Result};
use crate::healthcheck::{ServiceCheck, SystemdCheck};
use bottlerocket_release::BottlerocketRelease;
use reqwest::blocking::{Client, ClientBuilder, Request, Response};
use snafu::ResultExt;
use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;
use url::Url;

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
            config: match config {
                None => Config::new()?,
                Some(c) => c,
            },
            os_release: match os_release {
                None => BottlerocketRelease::new().context(error::BottlerocketRelease)?,
                Some(b) => b,
            },
            healthcheck: healthcheck.unwrap_or_else(|| Box::new(SystemdCheck {})),
        })
    }

    /// Sends any message to the metrics url
    // TODO - send documentation
    pub(crate) fn send<S1, S2>(
        &self,
        sender: S1,
        event: S2,
        values: Option<&HashMap<String, String>>,
    ) -> Result<()>
    where
        S1: AsRef<str>,
        S2: AsRef<str>,
    {
        let mut url = Url::from_str(&self.config.metrics_url).context(error::UrlParse {
            url: self.config.metrics_url.clone(),
        })?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("sender", sender.as_ref());
            q.append_pair("event", event.as_ref());
            let version = format!("{}", &self.os_release.version_id);
            q.append_pair("version", &version);
            q.append_pair("variant", &self.os_release.variant_id);
            q.append_pair("arch", &self.os_release.arch);
            q.append_pair("region", &self.config.region);
            q.append_pair("seed", format!("{}", &self.config.seed).as_str());
            q.append_pair("version-lock", &self.config.version_lock);
            let ignore_waves = format!("{}", self.config.ignore_waves);
            q.append_pair("ignore-waves", &ignore_waves);
            if let Some(map) = values {
                let mut keys: Vec<&String> = map.keys().collect();
                // sorted for consistency
                keys.sort();
                for key in keys {
                    if let Some(val) = map.get(key) {
                        q.append_pair(key, val);
                    }
                }
            }
        }
        Self::send_get_request(url, 20)?;
        Ok(())
    }

    /// Sends a notification to the metrics url that boot succeeded.
    pub(crate) fn send_boot_success(&self) -> Result<()> {
        self.send("healthdog", "boot-success", None)?;
        Ok(())
    }

    /// Checks the services listed in `config.service_health` using `healthcheck`. Sends a
    /// notification to the metrics url reporting `is_healthy=true&failed_services=` if all services
    /// are healthy, or `is_healthy=false&failed_services=a,b` if services 'a' and 'b' have failed.
    pub(crate) fn send_health_ping(&self) -> Result<()> {
        let mut is_healthy = true;
        let mut failed_services = String::new();
        for service in &self.config.service_health {
            let service_status = self.healthcheck.check(service)?;
            if !service_status.is_healthy {
                is_healthy = false;
                match service_status.exit_code {
                    None => failed_services.push_str(format!("{}", service).as_str()),
                    Some(exit_code) => failed_services
                        .push_str(format!("{}:{}", service.as_str(), exit_code).as_str()),
                }
            }
        }
        let mut values = HashMap::new();
        values.insert(String::from("is_healthy"), format!("{}", is_healthy));
        values.insert(String::from("failed_services"), failed_services);
        self.send("healthdog", "health-ping", Some(&values))?;
        Ok(())
    }

    // private

    fn send_get_request(url: Url, timeout_sec: u64) -> Result<()> {
        // TODO - create error variants for all of these unwraps
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_sec))
            .build()
            .unwrap();
        let response = client.get(url).send().unwrap();
        response.error_for_status().unwrap();
        Ok(())
    }
}
