/*!
# Introduction

shibaken is called by sundog as a setting generator.

shibaken will fetch and populate the admin container's user-data with authorized ssh keys from the
AWS instance metadata service (IMDS).

(The name "shibaken" comes from the fact that Shiba are small, but agile, hunting dogs.)
*/

#![deny(rust_2018_idioms)]

use imdsclient::ImdsClient;
use log::{debug, info, warn};
use serde::Serialize;
use simplelog::{ColorChoice, Config as LogConfig, LevelFilter, TermLogger, TerminalMode};
use snafu::{OptionExt, ResultExt};
use std::str::FromStr;
use std::{env, process};

#[derive(Serialize)]
struct UserData {
    ssh: Ssh,
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct Ssh {
    authorized_keys: Vec<String>,
}
impl UserData {
    fn new(public_keys: Vec<String>) -> Self {
        UserData {
            ssh: Ssh {
                authorized_keys: public_keys,
            },
        }
    }
}

/// Returns a list of public keys.
async fn fetch_public_keys_from_imds() -> Result<Vec<String>> {
    info!("Connecting to IMDS");
    let mut client = ImdsClient::new().await.context(error::ImdsClient)?;
    info!("Fetching list of available public keys from IMDS");
    // Returns a list of available public keys as '0=my-public-key'
    let public_key_list = match client
        .fetch_metadata("public-keys", "public keys list")
        .await
        .context(error::ImdsRequest)?
    {
        Some(public_key_list) => {
            debug!("available public keys '{}'", &public_key_list);
            public_key_list
        }
        None => {
            debug!("no available public keys");
            return Ok(Vec::new());
        }
    };

    info!("Generating targets to fetch text of available public keys");
    let public_key_targets = build_public_key_targets(&public_key_list);

    info!("Fetching public keys from IMDS");
    let mut public_keys = Vec::new();
    let target_count: u32 = 0;
    for target in &public_key_targets {
        let target_count = target_count + 1;
        let description = format!(
            "public key ({}/{})",
            target_count,
            &public_key_targets.len()
        );
        let public_key_text = client
            .fetch_metadata(&target, &description)
            .await
            .context(error::ImdsRequest)?
            .context(error::ImdsData {
                target,
                target_type: "meta-data",
            })?;
        let public_key = public_key_text.trim_end();
        // Simple check to see if the text is probably an ssh key.
        if public_key.starts_with("ssh") {
            debug!("{}", &public_key);
            public_keys.push(public_key.to_string())
        } else {
            warn!(
                "'{}' does not appear to be a valid key. Skipping...",
                &public_key
            );
            continue;
        }
    }
    if public_keys.is_empty() {
        warn!("No valid keys found");
    }
    Ok(public_keys)
}

/// Returns a list of public keys available in IMDS. Since IMDS returns the list of keys as
/// '0=my-public-key', we need to strip the index and insert it into the public key target.
fn build_public_key_targets(public_key_list: &str) -> Vec<String> {
    let mut public_key_targets = Vec::new();
    for available_key in public_key_list.lines() {
        let f: Vec<&str> = available_key.split('=').collect();
        // If f[0] isn't a number, then it isn't a valid index.
        if f[0].parse::<u32>().is_ok() {
            let public_key_target = format!("public-keys/{}/openssh-key", f[0]);
            public_key_targets.push(public_key_target);
        } else {
            warn!(
                "'{}' does not appear to be a valid index. Skipping...",
                &f[0]
            );
            continue;
        }
    }
    if public_key_targets.is_empty() {
        warn!("No valid key targets found");
    }
    public_key_targets
}

/// Store the args we receive on the command line.
struct Args {
    log_level: LevelFilter,
}

/// Print a usage message in the event a bad arg is passed
fn usage() {
    let program_name = env::args().next().unwrap_or_else(|| "program".to_string());
    eprintln!(
        r"Usage: {}
            [ --log-level trace|debug|info|warn|error ]",
        program_name
    );
}

/// Parse the args to the program and return an Args struct
fn parse_args(args: env::Args) -> Result<Args> {
    let mut log_level = None;

    let mut iter = args.skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_ref() {
            "--log-level" => {
                let log_level_str = iter.next().context(error::Usage {
                    message: "Did not give argument to --log-level",
                })?;
                log_level = Some(
                    LevelFilter::from_str(&log_level_str)
                        .context(error::LogLevel { log_level_str })?,
                );
            }

            x => {
                return error::Usage {
                    message: format!("unexpected argument '{}'", x),
                }
                .fail()
            }
        }
    }

    Ok(Args {
        log_level: log_level.unwrap_or(LevelFilter::Info),
    })
}

async fn run() -> Result<()> {
    let args = parse_args(env::args())?;

    // TerminalMode::Stderr will send all logs to stderr, as sundog only expects the json output of
    // the setting on stdout.
    TermLogger::init(
        args.log_level,
        LogConfig::default(),
        TerminalMode::Stderr,
        ColorChoice::Auto,
    )
    .context(error::Logger)?;

    info!("shibaken started");

    let public_keys = fetch_public_keys_from_imds().await?;

    let user_data = UserData::new(public_keys);

    info!("Generating user-data");
    // Serialize user_data to a JSON string that can be read by the admin container.
    let user_data_json = serde_json::to_string(&user_data).context(error::SerializeJson)?;
    debug!("{}", &user_data_json);

    info!("Encoding user-data");
    // admin container user-data must be base64-encoded to be passed through to the admin container
    // using a setting, rather than another arbitrary storage mechanism. This approach allows the
    // user to bypass shibaken and use their own user-data if desired.
    let user_data_base64 = base64::encode(&user_data_json);

    info!("Outputting user-data");
    // sundog expects JSON-serialized output so that many types can be represented, allowing the
    // API model to use more accurate types.
    let output = serde_json::to_string(&user_data_base64).context(error::SerializeJson)?;

    println!("{}", output);

    Ok(())
}

// Returning a Result from main makes it print a Debug representation of the error, but with Snafu
// we have nice Display representations of the error, so we wrap "main" (run) and print any error.
// https://github.com/shepmaster/snafu/issues/110
#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        match e {
            error::Error::Usage { .. } => {
                eprintln!("{}", e);
                usage();
                // sundog matches on the exit codes of the setting generators, so we should return 1
                // to make sure that this is treated as a failure.
                process::exit(1);
            }
            _ => {
                eprintln!("{}", e);
                process::exit(1);
            }
        }
    }
}

mod error {
    use snafu::Snafu;

    #[derive(Debug, Snafu)]
    #[snafu(visibility = "pub(super)")]
    pub(super) enum Error {
        #[snafu(display("IMDS request failed: {}", source))]
        ImdsRequest { source: imdsclient::Error },

        #[snafu(display("IMDS client failed: {}", source))]
        ImdsClient { source: imdsclient::Error },

        #[snafu(display(
            "IMDS client failed: Response '404' while fetching '{}' from '{}'",
            target,
            target_type,
        ))]
        ImdsData { target: String, target_type: String },

        #[snafu(display("Logger setup error: {}", source))]
        Logger { source: log::SetLoggerError },

        #[snafu(display("Invalid log level '{}'", log_level_str))]
        LogLevel {
            log_level_str: String,
            source: log::ParseLevelError,
        },

        #[snafu(display("Error serializing to JSON: {}", source))]
        SerializeJson { source: serde_json::error::Error },

        #[snafu(display("{}", message))]
        Usage { message: String },
    }
}
use error::Error;
type Result<T> = std::result::Result<T, Error>;
