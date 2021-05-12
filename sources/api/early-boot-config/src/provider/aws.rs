//! The aws module implements the `PlatformDataProvider` trait for gathering userdata on AWS.

use super::{PlatformDataProvider, SettingsJson};
use crate::compression::expand_slice_maybe;
use async_trait::async_trait;
use imdsclient::ImdsClient;
use serde_json::json;
use snafu::{OptionExt, ResultExt};
use std::fs;
use std::path::Path;

/// Unit struct for AWS so we can implement the PlatformDataProvider trait.
pub(crate) struct AwsDataProvider;

impl AwsDataProvider {
    const IDENTITY_DOCUMENT_FILE: &'static str = "/etc/early-boot-config/identity-document";
    const IDENTITY_DOCUMENT_TARGET: &'static str = "instance-identity/document";

    /// Fetches user data, which is expected to be in TOML form and contain a `[settings]` section,
    /// returning a SettingsJson representing the inside of that section.
    async fn user_data(client: &mut ImdsClient) -> Result<Option<SettingsJson>> {
        let user_data_raw = match client.fetch_userdata().await.context(error::ImdsRequest)? {
            None => return Ok(None),
            Some(s) => s,
        };
        let user_data_str = expand_slice_maybe(&user_data_raw)
            .context(error::Decompression { what: "user data" })?;
        trace!("Received user data: {}", user_data_str);

        let json = SettingsJson::from_toml_str(&user_data_str, "user data").context(
            error::SettingsToJSON {
                from: "instance user data",
            },
        )?;
        Ok(Some(json))
    }

    /// Fetches the instance identity, returning a SettingsJson representing the values from the
    /// document which we'd like to send to the API - currently just region.
    async fn identity_document(client: &mut ImdsClient) -> Result<Option<SettingsJson>> {
        let desc = "instance identity document";
        let target = Self::IDENTITY_DOCUMENT_TARGET;
        let file = Self::IDENTITY_DOCUMENT_FILE;

        let iid_str = if Path::new(file).exists() {
            info!("{} found at {}, using it", desc, file);
            fs::read_to_string(file).context(error::InputFileRead { path: file })?
        } else {
            match client
                .fetch_dynamic(target, desc)
                .await
                .context(error::ImdsRequest)?
            {
                None => return Ok(None),
                Some(raw) => {
                    expand_slice_maybe(&raw).context(error::Decompression { what: "user data" })?
                }
            }
        };
        trace!("Received instance identity document: {}", iid_str);

        // Grab region from instance identity document.
        let iid: serde_json::Value =
            serde_json::from_str(&iid_str).context(error::DeserializeJson)?;
        let region = iid
            .get("region")
            .context(error::IdentityDocMissingData { missing: "region" })?;
        let val = json!({ "aws": {"region": region} });

        let json = SettingsJson::from_val(&val, desc).context(error::SettingsToJSON {
            from: "instance identity document",
        })?;
        Ok(Some(json))
    }
}

#[async_trait]
impl PlatformDataProvider for AwsDataProvider {
    /// Return settings changes from the instance identity document and user data.
    async fn platform_data(
        &self,
    ) -> std::result::Result<Vec<SettingsJson>, Box<dyn std::error::Error>> {
        let mut output = Vec::new();

        let mut client = ImdsClient::new().await.context(error::ImdsClient)?;

        // Instance identity doc first, so the user has a chance to override
        match Self::identity_document(&mut client).await? {
            None => warn!("No instance identity document found."),
            Some(s) => output.push(s),
        }

        // Optional user-specified configuration / overrides
        match Self::user_data(&mut client).await? {
            None => warn!("No user data found."),
            Some(s) => output.push(s),
        }

        Ok(output)
    }
}

mod error {
    use snafu::Snafu;
    use std::io;
    use std::path::PathBuf;

    #[derive(Debug, Snafu)]
    #[snafu(visibility = "pub(super)")]
    pub(crate) enum Error {
        #[snafu(display("Failed to decompress {}: {}", what, source))]
        Decompression { what: String, source: io::Error },

        #[snafu(display("Error deserializing from JSON: {}", source))]
        DeserializeJson { source: serde_json::error::Error },

        #[snafu(display("Instance identity document missing {}", missing))]
        IdentityDocMissingData { missing: String },

        #[snafu(display("IMDS client failed: {}", source))]
        ImdsClient { source: imdsclient::Error },

        #[snafu(display(
            "IMDS client failed: Response '404' while fetching '{}' from '{}'",
            target,
            target_type,
        ))]
        ImdsData { target: String, target_type: String },

        #[snafu(display("IMDS request failed: {}", source))]
        ImdsRequest { source: imdsclient::Error },

        #[snafu(display("Unable to read input file '{}': {}", path.display(), source))]
        InputFileRead { path: PathBuf, source: io::Error },

        #[snafu(display("Unable to serialize settings from {}: {}", from, source))]
        SettingsToJSON {
            from: String,
            source: crate::settings::Error,
        },
    }
}

type Result<T> = std::result::Result<T, error::Error>;
