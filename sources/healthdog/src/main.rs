/*!
# Introduction

`healthdog` sends anonymous information about the health of a Bottlerocket host.
It does so by sending key-value pairs as query params in an HTTP GET request.

# What it Sends

TODO - list of the metrics being sent.

*/

#![deny(rust_2018_idioms, unreachable_pub, missing_copy_implementations)]

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
