use crate::error::{self, Result};
use lazy_static::lazy_static;
use log::trace;
use regex::Regex;
use snafu::ResultExt;
use std::process::Command;

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct ServiceHealth {
    /// Whether or not the service reports as healthy.
    pub(crate) is_healthy: bool,
    /// In the event of an unhealthy service, the service's exit code (if found).
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
}

impl Outcome {
    fn is_exit_true(&self) -> bool {
        self.exit == 0
    }
}

fn systemctl(args: &[&str]) -> Result<Outcome> {
    trace!("calling systemctl with '{:?}'", args);
    let output = Command::new("systemctl")
        .args(args)
        .output()
        .context(error::Command {
            command: "systemctl",
            args: args.iter().map(|&s| s.to_owned()).collect::<Vec<String>>(),
        })?;
    Ok(Outcome {
        exit: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(output.stdout.as_slice()).into(),
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
    // we don't check the command's exit code because systemctl returns non-zero codes for various
    // non-exceptional execution outcomes.
    let outcome = systemctl(&["--no-pager", "status", service])?;
    Ok(parse_stdout(&outcome.stdout)?)
}

/// Regex pattern for finding the exit code of a systemd service that has exited. There is a single
/// capture group, named `exit_code`.
const SYSTEMD_EXIT_REGEX_PATTERN: &str =
    r#"Main PID: \d+ \(code=[a-zA-Z0-9-_]+, status=(?P<exit_code>\d{1,3})/[A-Z]+\)"#;

lazy_static! {
    static ref RX: Regex = Regex::new(SYSTEMD_EXIT_REGEX_PATTERN).unwrap();
}

fn parse_stdout(stdout: &str) -> Result<Option<i32>> {
    trace!("parsing systemctl stdout:\n{}", stdout);
    let captures = if let Some(caps) = RX.captures(stdout) {
        caps
    } else {
        return Ok(None);
    };
    let s = if let Some(m) = captures.name("exit_code") {
        m.as_str()
    } else {
        return Ok(None);
    };
    Ok(Some(
        s.parse::<i32>().context(error::IntParse { value: s })?,
    ))
}

#[test]
fn parse_stdout_exit_0() {
    let stdout = r#"● somesvc-start.service - Do Somesvc Thing
   Loaded: loaded (/usr/lib/systemd/system/somesvc-start.service; static; vendor preset: disabled)
   Active: active (exited) since Tue 2020-07-28 17:20:10 UTC; 4min 11s ago
  Process: 824 ExecStart=/usr/sbin/somesvcd --mode=boot --pid-file=/run/somesvc/pid
           --attach-to-session (code=exited, status=0/SUCCESS)
  Process: 846 ExecStartPost=/usr/bin/somesvc show-splash (code=exited, status=123/SUCCESS)
 Main PID: 845 (code=exited, status=0/SUCCESS)

Jul 28 17:20:10 severus systemd[1]: Starting Do Somesvc Thing...
Jul 28 17:20:10 severus systemd[1]: Started Do Somesvc Thing.
"#;
    let got = parse_stdout(stdout).unwrap().unwrap();
    let want = 123;
    assert_eq!(got, want);
}
