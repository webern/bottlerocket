use crate::error::{self, Error, Result};
use snafu::{ensure, ResultExt};
use std::process::Command;

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct ServiceHealth {
    /// Whether or not the service reports as healthy.
    pub(crate) is_healthy: bool,
    /// In the event of an unhealthy service, the service's failure exit code goes here.
    pub(crate) exit_code: Option<i32>,
}

pub(crate) trait ServiceCheck {
    /// Checks the given service to see if it is healthy.
    fn check(&self, service_name: &str) -> Result<ServiceHealth>;
}

pub(crate) struct SystemdCheck {}

impl ServiceCheck for SystemdCheck {
    fn check(&self, service_name: &str) -> Result<ServiceHealth> {
        if is_ok(service_name)? {
            return Ok(ServiceHealth {
                is_healthy: true,
                exit_code: None,
            });
        }
        Ok(ServiceHealth {
            is_healthy: false,
            exit_code: parse_service_exit_code(service_name)?,
        })
    }
}

struct Outcome {
    exit: i32,
    stdout: String,
    stderr: String,
}

const EXIT_TRUE: i32 = 0;

impl Outcome {
    fn is_exit_true(&self) -> bool {
        self.exit == EXIT_TRUE
    }
}

fn systemctl(args: &[&str]) -> Result<Outcome> {
    let output = Command::new("systemctl")
        .args(args)
        .output()
        // TODO - add more info to this error?
        .context(error::Command)?;

    Ok(Outcome {
        exit: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(output.stdout.as_slice()).into(),
        stderr: String::from_utf8_lossy(output.stderr.as_slice()).into(),
    })
}

fn is_active(service: &str) -> Result<bool> {
    let outcome = systemctl(&["is-active", service])?;
    Ok(outcome.is_exit_true())
}

fn is_failed(service: &str) -> Result<bool> {
    let outcome = systemctl(&["is-failed", service])?;
    Ok(outcome.is_exit_true())
}

fn is_ok(service: &str) -> Result<bool> {
    Ok(!is_failed(service)? && is_active(service)?)
}

fn parse_service_exit_code(service: &str) -> Result<Option<i32>> {
    // TODO - implement parse_service_exit_code
    let outcome = systemctl(&["--no-pager", "status", service])?;
    if outcome.exit != 0 {
        return Err(Error::CommandExit {
            exit: outcome.exit,
            stderr: outcome.stderr,
        });
    }
    Ok(parse_stdout(&outcome.stdout)?)
}

fn parse_stdout(_stdout: &str) -> Result<Option<i32>> {
    // TODO - implement parse_stdout
    Ok(None)
}
