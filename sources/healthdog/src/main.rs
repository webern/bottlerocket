/*!
# Introduction

`healthdog` sends anonymous information about the health of a Bottlerocket host.
It does so by sending key-value pairs as query params in an HTTP GET request.

# What it Sends

```suggestion
The standard set of metrics:
* `sender`: the application sending the report.
* `event`: the event that invoked the report.
* `version`: the Bottlerocket version.
* `variant`: the Bottlerocket variant.
* `arch`: the machine architecture, e.g.'x86_64' or 'arm'.
* `region`: the region the machine is running in.
* `seed`: the seed value used to roll-out updates.
* `version-lock`: the optional setting that locks Bottlerocket to a certain version.
* `ignore-waves`: an update setting that allows hosts to update before their seed is reached.

Additionally, when `healthdog` sends a 'health ping', it adds:
* `is-healthy`: true or false based on whether critical services are running.
* `failed_services`: a list of critical services that have failed, if any.

# Configuration

The following configuration options are available, and read by `healthdog` from a `toml` file that looks like this:

```toml
# the url to which healthdog will send metrics information
metrics_url = "https://example.com/metrics"
# whether or not healthdog will send metrics. opt-out by setting this to false
send_metrics = true
# a list of systemd service names that will be checked
service_health = ["apiserver", "containerd", "kubelet"]
# the region
region = "us-west-2"
# the update wave seed
seed = 1234
# what version bottlerocket should stay on
version_lock = "latest"
# whether bottlerocket should ignore update roll-out timing
ignore_waves = false

*/

#![deny(rust_2018_idioms)]

mod args;
mod config;
mod error;
mod healthdog;
#[cfg(test)]
mod healthdog_test;
#[cfg(test)]
mod main_test;
mod service_check;

use crate::args::{Command, USAGE};
use crate::config::Config;
use crate::error::{Error, Result};
use crate::healthdog::Healthdog;
use crate::service_check::{ServiceCheck, SystemdCheck};
use args::parse_args;
use bottlerocket_release::BottlerocketRelease;
use env_logger::Builder;
use log::{error, trace};
use snafu::ResultExt;
use std::sync::Once;
use std::{env, process};

fn main() -> ! {
    process::exit(match main_inner(env::args(), Box::new(SystemdCheck {})) {
        Ok(()) => 0,
        Err(err) => {
            if let Error::Usage { message } = err {
                if let Some(message) = message {
                    eprintln!("{}\n", message)
                }
                eprintln!("{}", USAGE);
                2
            } else {
                eprintln!("{}", err);
                1
            }
        }
    })
}

/// To facilitate testing of `main_inner` function, ensure that the logger is only initialized once.
static INIT_LOGGER_ONCE: Once = Once::new();

/// pub(crate) for testing.
pub(crate) fn main_inner<A>(args: A, service_check: Box<dyn ServiceCheck>) -> Result<()>
where
    A: Iterator<Item = String>,
{
    let arguments = parse_args(args)?;
    INIT_LOGGER_ONCE.call_once(|| {
        match arguments.log_level {
            None => Builder::new().init(),
            Some(level) => Builder::new().filter_module("healthdog", level).init(),
        }
        trace!("logger initialized");
    });
    let os_release = if let Some(os_release_path) = &arguments.os_release {
        BottlerocketRelease::from_file(os_release_path)
    } else {
        BottlerocketRelease::new()
    }
    .context(error::BottlerocketRelease)?;
    let config = match &arguments.config_path {
        None => Config::new()?,
        Some(filepath) => Config::from_file(filepath)?,
    };
    // exit early with no error if the opt-out flag is set
    if !config.send_metrics {
        return Ok(());
    }
    let healthdog = Healthdog::from_parts(Some(config), Some(os_release), Some(service_check))?;
    match arguments.command {
        Command::BootSuccess => {
            if let Err(err) = healthdog.send_boot_success() {
                // we don't want to fail the boot if there is a failure to send this message, so
                // we log the error and return Ok(())
                error!("Error while reporting boot success: {}", err);
            }
        }
        Command::HealthPing => {
            healthdog.send_health_ping()?;
        }
    }
    Ok(())
}
