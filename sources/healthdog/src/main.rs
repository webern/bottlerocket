/*!
# Introduction

`healthdog` is a program that gathers logs from various places on a Bottlerocket host and combines them
into a tarball for easy export.

Usage example:
```
$ healthdog
logs are at: /tmp/bottlerocket-logs.tar.gz
```
*/

#![deny(rust_2018_idioms)]

mod args;
mod config;
mod error;
mod healthdog;
#[cfg(test)]
mod healthdog_test;
mod service_check;

use crate::args::{Command, USAGE};
use crate::config::Config;
use crate::error::{Error, Result};
use crate::healthdog::Healthdog;
use crate::service_check::{ServiceCheck, SystemdCheck};
use args::parse_args;
use bottlerocket_release::BottlerocketRelease;
use env_logger::Builder;
use log::trace;
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

/// To facilitate end-to-end testing, ensure that the logger is only initialized once.
static INIT_LOGGER_ONCE: Once = Once::new();

fn main_inner<A>(args: A, service_check: Box<dyn ServiceCheck>) -> Result<()>
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
    let config = Config::from_file(&arguments.config_path)?;
    let healthdog = Healthdog::from_parts(Some(config), Some(os_release), Some(service_check))?;
    match arguments.command {
        Command::BootSuccess => {
            if let Err(err) = healthdog.send_boot_success() {
                // we don't want to fail the boot if there is a failure to send this message, so
                // we log the error and return Ok(())
                eprintln!("healthdog error while reporting boot success: {}", err);
            }
        }
        Command::HealthPing => {
            healthdog.send_health_ping()?;
        }
    }
    Ok(())
}
