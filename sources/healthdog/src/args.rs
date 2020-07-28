use crate::error::{self, Error, Result};
use snafu::{ensure, OptionExt};
use std::path::PathBuf;

const BOOT_SUCCESS: &str = "report-boot-success";
const HEALTH_PING: &str = "send-health-ping";
pub(crate) const DEFAULT_CONFIG_PATH: &str = "/etc/healthdog.toml";

/// The command, e.g. `healthdog report-boot-success` or `healthdog send-health-ping`
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum Command {
    BootSuccess,
    HealthPing,
}

impl Command {
    fn parse<S: AsRef<str>>(s: S) -> Result<Self> {
        match s.as_ref() {
            BOOT_SUCCESS => Ok(Command::BootSuccess),
            HEALTH_PING => Ok(Command::HealthPing),
            unk => Err(Error::Usage {
                message: Some(format!("Unknown command: '{}'", unk)),
            }),
        }
    }
}

pub(crate) struct Arguments {
    pub(crate) command: Command,
    pub(crate) config_path: PathBuf,
    pub(crate) os_release: Option<PathBuf>,
}

/// The usage message for --help.
pub(crate) const USAGE: &str = r"USAGE:
healthdog <SUBCOMMAND> <OPTIONS>

SUBCOMMANDS:
    report-boot-success     Send a report that the instance booted successfully.

    send-health-ping        Check services and report whether the host is healthy or not.

GLOBAL OPTIONS:
    [ --config ]            Path to the TOML config file.
    [ --os-release ]        Path to the os-release file.
";

/// Parses the command line arguments.
pub(crate) fn parse_args<A>(args: A) -> Result<Arguments>
where
    A: Iterator<Item = String>,
{
    let mut config_path = None;
    let mut subcommand = None;
    let mut os_release = None;
    let mut iter = args.skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_ref() {
            "--config" => {
                let val = iter.next().context(error::Usage {
                    message: String::from("Did not give argument to --config"),
                })?;
                config_path = Some(PathBuf::from(val));
            }
            "--os-release" => {
                let val = iter.next().context(error::Usage {
                    message: String::from("Did not give argument to --os-release"),
                })?;
                os_release = Some(PathBuf::from(val));
            }
            "--help" | "-h" => return Err(Error::Usage { message: None }),
            // Assume any arguments not prefixed with '-' is a subcommand
            s if !s.starts_with('-') => {
                ensure!(
                    subcommand.is_none(),
                    error::Usage {
                        message: Some(format!("A second command was found: '{}'", s))
                    }
                );
                subcommand = Some(Command::parse(s)?);
            }
            unknown => {
                return Err(Error::Usage {
                    message: Some(format!("Unexpected argument: '{}'", unknown)),
                });
            }
        }
    }

    Ok(Arguments {
        command: subcommand.context(error::Usage {
            message: Some(String::from("Subcommand not found.")),
        })?,
        config_path: config_path.unwrap_or(PathBuf::from(DEFAULT_CONFIG_PATH)),
        os_release,
    })
}

#[test]
fn parse_args_test_boot_success() {
    let raw_args = vec![
        String::from("/bin/healthdog"),
        String::from(BOOT_SUCCESS),
        String::from("--config"),
        String::from("/some/path"),
    ];
    let iter = raw_args.iter().cloned();
    let args = parse_args(iter).unwrap();
    assert_eq!(args.command, Command::BootSuccess);
    assert_eq!(args.config_path.to_str().unwrap(), "/some/path");
    assert!(args.os_release.is_none());
}

#[test]
fn parse_args_test_boot_success_default_config() {
    let raw_args = vec![
        String::from("/bin/healthdog"),
        String::from(BOOT_SUCCESS),
        String::from("--os-release"),
        String::from("/my/os-release"),
    ];
    let iter = raw_args.iter().cloned();
    let args = parse_args(iter).unwrap();
    assert_eq!(args.command, Command::BootSuccess);
    assert_eq!(args.config_path.to_str().unwrap(), "/etc/healthdog.toml");
    assert_eq!(args.os_release.unwrap().to_str().unwrap(), "/my/os-release");
}

#[test]
fn parse_args_test_health_ping() {
    let raw_args = vec![
        String::from("/bin/healthdog"),
        String::from(HEALTH_PING),
        String::from("--config"),
        String::from("/some/path"),
    ];
    let iter = raw_args.iter().cloned();
    let args = parse_args(iter).unwrap();
    assert_eq!(args.command, Command::HealthPing);
    assert_eq!(args.config_path.to_str().unwrap(), "/some/path");
}

#[test]
fn parse_args_test_bad_command() {
    let raw_args = vec![
        String::from("/bin/healthdog"),
        String::from("nope"),
        String::from("--config"),
        String::from("/some/path"),
    ];
    let iter = raw_args.iter().cloned();
    let result = parse_args(iter);
    assert!(result.is_err())
}

#[test]
fn parse_args_test_no_command() {
    let raw_args = vec![
        String::from("/bin/healthdog"),
        String::from("--config"),
        String::from("/some/path"),
    ];
    let iter = raw_args.iter().cloned();
    let result = parse_args(iter);
    assert!(result.is_err())
}

#[test]
fn parse_args_test_bad_value() {
    let raw_args = vec![
        String::from("/bin/healthdog"),
        String::from("nope"),
        String::from("--config"),
    ];
    let iter = raw_args.iter().cloned();
    let result = parse_args(iter);
    assert!(result.is_err())
}
