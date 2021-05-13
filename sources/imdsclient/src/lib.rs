/*!
The imdsclient library provides high-level methods to interact with the AWS Instance Metadata Service.
The high-level methods provided are [`fetch_dynamic`], [`fetch_metadata`], and [`fetch_userdata`].

For more control, and to query IMDS without high-level wrappers, there is also a [`fetch_imds`] method.
This method is useful for specifying things like a pinned date for the IMDS schema version.
*/

#![deny(rust_2018_idioms)]

use http::StatusCode;
use log::{debug, info, trace, warn};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use snafu::{ensure, OptionExt, ResultExt};

const BASE_URI: &str = "http://169.254.169.254";
const SCHEMA_VERSION: &str = "2021-01-03";
const IDENTITY_DOCUMENT_TARGET: &'static str = "instance-identity/document";

// Currently only able to get fetch session tokens from `latest`
const IMDS_SESSION_TARGET: &str = "latest/api/token";

/// A client for making IMDSv2 queries.
/// It obtains a session token when it is first instantiated and is reused between helper functions.
pub struct ImdsClient {
    client: Client,
    imds_base_uri: String,
    session_token: String,
}

/// This is the return type when querying for the IMDS identity document, which contains information
/// such as region and instance_type. We only include the fields that we are using in Bottlerocket.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityDocument {
    region: String,
    instance_type: String,
}

impl IdentityDocument {
    pub fn region(&self) -> &str {
        self.region.as_str()
    }

    pub fn instance_type(&self) -> &str {
        self.instance_type.as_str()
    }
}

impl ImdsClient {
    pub async fn new() -> Result<Self> {
        Self::new_impl(BASE_URI.to_string()).await
    }

    async fn new_impl(imds_base_uri: String) -> Result<Self> {
        let client = Client::new();
        let session_token = fetch_token(&client, &imds_base_uri).await?;
        Ok(Self {
            client,
            imds_base_uri,
            session_token,
        })
    }

    /// Gets `user-data` from IMDS. The user-data may be either a UTF-8 string or compressed bytes.
    pub async fn fetch_userdata(&mut self) -> Result<Option<Vec<u8>>> {
        self.fetch_imds(SCHEMA_VERSION, "user-data", "user-data")
            .await
    }

    /// Returns the 'identity document' with fields like region and instance_type.
    pub async fn fetch_identity_document(&mut self) -> Result<IdentityDocument> {
        let response = self
            .fetch_dynamic(IDENTITY_DOCUMENT_TARGET, "fetch_identity_document")
            .await?
            .context(error::Empty {
                what: "identity document",
            })?;
        let identity_document: IdentityDocument =
            serde_json::from_slice(&response).context(error::Serde)?;
        Ok(identity_document)
    }

    /// Returns the list of network interface mac addresses.
    pub async fn fetch_mac_addresses(&mut self) -> Result<Vec<String>> {
        let macs_target = "network/interfaces/macs";
        let macs = self
            .fetch_metadata(&macs_target, "MAC addresses")
            .await?
            .context(error::Empty {
                what: "list of mac addresses",
            })?;

        Ok(macs.split('\n').map(|s| s.to_string()).collect())
    }

    /// Gets the list of CIDR blocks for a given network interface `mac` address.
    pub async fn fetch_cidr_blocks_for_mac(&mut self, mac: &str) -> Result<Vec<String>> {
        // Infer the cluster DNS based on our CIDR blocks.
        let mac_cidr_blocks_target =
            format!("network/interfaces/macs/{}/vpc-ipv4-cidr-blocks", mac);
        let cidr_blocks = self
            .fetch_metadata(&mac_cidr_blocks_target, "MAC CIDR blocks")
            .await?
            .context(error::Empty {
                what: "list of CIDR blocks",
            })?;

        Ok(cidr_blocks.split('\n').map(|s| s.to_string()).collect())
    }

    /// Gets the local IPV4 address from instance metadata.
    pub async fn fetch_local_ipv4_address(&mut self) -> Result<String> {
        let node_ip_target = "local-ipv4";
        self.fetch_metadata(&node_ip_target, "node IPv4 address")
            .await?
            .context(error::Empty { what: "local-ipv4" })
    }

    /// Returns a list of public ssh keys skipping any keys that do not start with 'ssh'.
    pub async fn fetch_public_ssh_keys(&mut self) -> Result<Vec<String>> {
        info!("Fetching list of available public keys from IMDS");
        // Returns a list of available public keys as '0=my-public-key'
        let public_key_list = match self
            .fetch_metadata("public-keys", "public keys list")
            .await?
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

            let public_key_text = self
                .fetch_metadata(&target, &description)
                .await?
                .context(error::Empty { what: "public key" })?;
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

    /// Helper to fetch `dynamic` targets from IMDS.
    /// - `end_target` is the uri path relative to `dynamic`.
    /// - `description` is used in debugging and error statements.
    async fn fetch_dynamic<S1, S2>(
        &mut self,
        end_target: S1,
        description: S2,
    ) -> Result<Option<Vec<u8>>>
    where
        S1: AsRef<str>,
        S2: AsRef<str>,
    {
        let dynamic_target = format!("dynamic/{}", end_target.as_ref());
        self.fetch_imds(SCHEMA_VERSION, &dynamic_target, description.as_ref())
            .await
    }

    /// Helper to fetch `meta-data` targets from IMDS.
    /// - `end_target` is the uri path relative to `meta-data`.
    /// - `description` is used in debugging and error statements.
    async fn fetch_metadata<S1, S2>(
        &mut self,
        end_target: S1,
        description: S2,
    ) -> Result<Option<String>>
    where
        S1: AsRef<str>,
        S2: AsRef<str>,
    {
        let metadata_target = format!("meta-data/{}", end_target.as_ref());
        match self
            .fetch_imds(SCHEMA_VERSION, &metadata_target, description.as_ref())
            .await?
        {
            Some(metadata_body) => Ok(Some(
                String::from_utf8(metadata_body).context(error::NonUtf8Response)?,
            )),
            None => Ok(None),
        }
    }

    /// Fetch data from IMDS.
    async fn fetch_imds<S1, S2, S3>(
        &mut self,
        schema_version: S1,
        target: S2,
        description: S3,
    ) -> Result<Option<Vec<u8>>>
    where
        S1: AsRef<str>,
        S2: AsRef<str>,
        S3: AsRef<str>,
    {
        let uri = format!(
            "{}/{}/{}",
            self.imds_base_uri,
            schema_version.as_ref(),
            target.as_ref()
        );
        debug!("Requesting {} from {}", description.as_ref(), &uri);
        let mut attempt: u8 = 1;
        let max_attempts: u8 = 3;
        loop {
            attempt += 1;
            ensure!(attempt <= max_attempts, error::FailedFetch { attempt });
            let response = self
                .client
                .get(&uri)
                .header("X-aws-ec2-metadata-token", &self.session_token)
                .send()
                .await
                .context(error::Request {
                    method: "GET",
                    uri: &uri,
                })?;
            trace!("IMDS response: {:?}", &response);

            match response.status() {
                code @ StatusCode::OK => {
                    info!("Received {}", description.as_ref());
                    let response_body = response
                        .bytes()
                        .await
                        .context(error::ResponseBody {
                            method: "GET",
                            uri: &uri,
                            code,
                        })?
                        .to_vec();

                    let response_str = printable_string(&response_body);
                    trace!("Response: {:?}", response_str);

                    return Ok(Some(response_body));
                }

                // IMDS returns 404 if no user data is given, or if IMDS is disabled
                StatusCode::NOT_FOUND => return Ok(None),

                // IMDS returns 401 if the session token is expired or invalid
                StatusCode::UNAUTHORIZED => {
                    info!("Session token is invalid or expired");
                    self.refresh_token().await?;
                    info!("Refreshed session token");
                    continue;
                }

                code => {
                    let response_body = response
                        .bytes()
                        .await
                        .context(error::ResponseBody {
                            method: "GET",
                            uri: &uri,
                            code,
                        })?
                        .to_vec();

                    let response_str = printable_string(&response_body);

                    trace!("Response: {:?}", response_str);

                    return error::Response {
                        method: "GET",
                        uri: &uri,
                        code,
                        response_body: response_str,
                    }
                    .fail();
                }
            }
        }
    }

    /// Fetches a new session token and adds it to the current ImdsClient.
    async fn refresh_token(&mut self) -> Result<()> {
        self.session_token = fetch_token(&self.client, &self.imds_base_uri).await?;
        Ok(())
    }
}

/// Converts `bytes` to a `String` if it is a UTF-8 encoded string. Truncates the string if it is
/// too long for printing.
fn printable_string(bytes: &[u8]) -> String {
    if let Ok(s) = String::from_utf8(bytes.into()) {
        if s.len() < 2048 {
            s
        } else {
            format!("{}<truncated...>", &s[0..2034])
        }
    } else {
        "<binary>".to_string()
    }
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

/// Helper to fetch an IMDSv2 session token that is valid for 60 seconds.
async fn fetch_token(client: &Client, imds_base_uri: &str) -> Result<String> {
    let uri = format!("{}/{}", imds_base_uri, IMDS_SESSION_TARGET);
    let response = client
        .put(&uri)
        .header("X-aws-ec2-metadata-token-ttl-seconds", "60")
        .send()
        .await
        .context(error::Request {
            method: "PUT",
            uri: &uri,
        })?
        .error_for_status()
        .context(error::BadResponse { uri: &uri })?;
    let code = response.status();
    response.text().await.context(error::ResponseBody {
        method: "PUT",
        uri,
        code,
    })
}

mod error {
    use http::StatusCode;
    use snafu::Snafu;

    // Extracts the status code from a reqwest::Error and converts it to a string to be displayed
    fn get_status_code(source: &reqwest::Error) -> String {
        source
            .status()
            .as_ref()
            .map(|i| i.as_str())
            .unwrap_or("Unknown")
            .to_string()
    }

    #[derive(Debug, Snafu)]
    #[snafu(visibility = "pub(super)")]

    pub enum Error {
        #[snafu(display("Response '{}' from '{}': {}", get_status_code(&source), uri, source))]
        BadResponse { uri: String, source: reqwest::Error },

        #[snafu(display("404 retrieving {}", what))]
        Empty { what: String },

        #[snafu(display("IMDS fetch failed after {} attempts", attempt))]
        FailedFetch { attempt: u8 },

        #[snafu(display("IMDS session failed: {}", source))]
        FailedSession { source: reqwest::Error },

        #[snafu(display("Response was not UTF-8: {}", source))]
        NonUtf8Response { source: std::string::FromUtf8Error },

        #[snafu(display("Error {}ing '{}': {}", method, uri, source))]
        Request {
            method: String,
            uri: String,
            source: reqwest::Error,
        },

        #[snafu(display("Error {} when {}ing '{}': {}", code, method, uri, response_body))]
        Response {
            method: String,
            uri: String,
            code: StatusCode,
            response_body: String,
        },

        #[snafu(display(
            "Unable to read response body when {}ing '{}' (code {}) - {}",
            method,
            uri,
            code,
            source
        ))]
        ResponseBody {
            method: String,
            uri: String,
            code: StatusCode,
            source: reqwest::Error,
        },

        #[snafu(display("Deserialization error: {}", source))]
        Serde { source: serde_json::Error },
    }
}

pub use error::Error;
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod test {
    use super::*;
    use httptest::{matchers::*, responders::*, Expectation, Server};

    #[tokio::test]
    async fn new_imds_client() {
        let server = Server::run();
        let port = server.addr().port();
        let base_uri = format!("http://localhost:{}", port);
        let token = "some+token";
        server.expect(
            Expectation::matching(request::method_path("PUT", "/latest/api/token"))
                .times(1)
                .respond_with(
                    status_code(200)
                        .append_header("X-aws-ec2-metadata-token-ttl-seconds", "60")
                        .body(token),
                ),
        );
        let imds_client = ImdsClient::new_impl(base_uri).await.unwrap();
        assert_eq!(imds_client.session_token, token);
    }

    #[tokio::test]
    async fn fetch_imds() {
        let server = Server::run();
        let port = server.addr().port();
        let base_uri = format!("http://localhost:{}", port);
        let token = "some+token";
        let schema_version = "latest";
        let target = "meta-data/instance-type";
        let description = "instance type";
        let response_code = 200;
        let response_body = "m5.large";
        server.expect(
            Expectation::matching(request::method_path("PUT", "/latest/api/token"))
                .times(1)
                .respond_with(
                    status_code(200)
                        .append_header("X-aws-ec2-metadata-token-ttl-seconds", "60")
                        .body(token),
                ),
        );
        server.expect(
            Expectation::matching(request::method_path(
                "GET",
                format!("/{}/{}", schema_version, target),
            ))
            .times(1)
            .respond_with(
                status_code(response_code)
                    .append_header("X-aws-ec2-metadata-token", token)
                    .body(response_body),
            ),
        );
        let mut imds_client = ImdsClient::new_impl(base_uri).await.unwrap();
        let imds_data = imds_client
            .fetch_imds(schema_version, target, description)
            .await
            .unwrap();
        assert_eq!(imds_data, Some(response_body.as_bytes().to_vec()));
    }

    #[tokio::test]
    async fn fetch_imds_notfound() {
        let server = Server::run();
        let port = server.addr().port();
        let base_uri = format!("http://localhost:{}", port);
        let token = "some+token";
        let schema_version = "latest";
        let target = "meta-data/instance-type";
        let description = "instance type";
        let response_code = 404;
        server.expect(
            Expectation::matching(request::method_path("PUT", "/latest/api/token"))
                .times(1)
                .respond_with(
                    status_code(200)
                        .append_header("X-aws-ec2-metadata-token-ttl-seconds", "60")
                        .body(token),
                ),
        );
        server.expect(
            Expectation::matching(request::method_path(
                "GET",
                format!("/{}/{}", schema_version, target),
            ))
            .times(1)
            .respond_with(
                status_code(response_code).append_header("X-aws-ec2-metadata-token", token),
            ),
        );
        let mut imds_client = ImdsClient::new_impl(base_uri).await.unwrap();
        let imds_data = imds_client
            .fetch_imds(schema_version, target, description)
            .await
            .unwrap();
        assert_eq!(imds_data, None);
    }

    #[tokio::test]
    async fn fetch_imds_unauthorized() {
        let server = Server::run();
        let port = server.addr().port();
        let base_uri = format!("http://localhost:{}", port);
        let token = "some+token";
        let schema_version = "latest";
        let target = "meta-data/instance-type";
        let description = "instance type";
        let response_code = 401;
        server.expect(
            Expectation::matching(request::method_path("PUT", "/latest/api/token"))
                .times(3)
                .respond_with(
                    status_code(200)
                        .append_header("X-aws-ec2-metadata-token-ttl-seconds", "60")
                        .body(token),
                ),
        );
        server.expect(
            Expectation::matching(request::method_path(
                "GET",
                format!("/{}/{}", schema_version, target),
            ))
            .times(2)
            .respond_with(
                status_code(response_code).append_header("X-aws-ec2-metadata-token", token),
            ),
        );
        let mut imds_client = ImdsClient::new_impl(base_uri).await.unwrap();
        assert!(imds_client
            .fetch_imds(schema_version, target, description)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn fetch_imds_timeout() {
        let server = Server::run();
        let port = server.addr().port();
        let base_uri = format!("http://localhost:{}", port);
        let token = "some+token";
        let schema_version = "latest";
        let target = "meta-data/instance-type";
        let description = "instance type";
        let response_code = 408;
        server.expect(
            Expectation::matching(request::method_path("PUT", "/latest/api/token"))
                .times(1)
                .respond_with(
                    status_code(200)
                        .append_header("X-aws-ec2-metadata-token-ttl-seconds", "60")
                        .body(token),
                ),
        );
        server.expect(
            Expectation::matching(request::method_path(
                "GET",
                format!("/{}/{}", schema_version, target),
            ))
            .times(1)
            .respond_with(
                status_code(response_code).append_header("X-aws-ec2-metadata-token", token),
            ),
        );
        let mut imds_client = ImdsClient::new_impl(base_uri).await.unwrap();
        assert!(imds_client
            .fetch_imds(schema_version, target, description)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn fetch_metadata() {
        let server = Server::run();
        let port = server.addr().port();
        let base_uri = format!("http://localhost:{}", port);
        let token = "some+token";
        let end_target = "instance-type";
        let description = "instance type";
        let response_code = 200;
        let response_body = "m5.large";
        server.expect(
            Expectation::matching(request::method_path("PUT", "/latest/api/token"))
                .times(1)
                .respond_with(
                    status_code(200)
                        .append_header("X-aws-ec2-metadata-token-ttl-seconds", "60")
                        .body(token),
                ),
        );
        server.expect(
            Expectation::matching(request::method_path(
                "GET",
                format!("/{}/meta-data/{}", SCHEMA_VERSION, end_target),
            ))
            .times(1)
            .respond_with(
                status_code(response_code)
                    .append_header("X-aws-ec2-metadata-token", token)
                    .body(response_body),
            ),
        );
        let mut imds_client = ImdsClient::new_impl(base_uri).await.unwrap();
        let imds_data = imds_client
            .fetch_metadata(end_target, description)
            .await
            .unwrap();
        assert_eq!(imds_data, Some(response_body.to_string()));
    }

    #[tokio::test]
    async fn fetch_dynamic() {
        let server = Server::run();
        let port = server.addr().port();
        let base_uri = format!("http://localhost:{}", port);
        let token = "some+token";
        let end_target = "instance-identity/document";
        let description = "instance identity document";
        let response_code = 200;
        let response_body = r#"{"region" : "us-west-2"}"#;
        server.expect(
            Expectation::matching(request::method_path("PUT", "/latest/api/token"))
                .times(1)
                .respond_with(
                    status_code(200)
                        .append_header("X-aws-ec2-metadata-token-ttl-seconds", "60")
                        .body(token),
                ),
        );
        server.expect(
            Expectation::matching(request::method_path(
                "GET",
                format!("/{}/dynamic/{}", SCHEMA_VERSION, end_target),
            ))
            .times(1)
            .respond_with(
                status_code(response_code)
                    .append_header("X-aws-ec2-metadata-token", token)
                    .body(response_body),
            ),
        );
        let mut imds_client = ImdsClient::new_impl(base_uri).await.unwrap();
        let imds_data = imds_client
            .fetch_dynamic(end_target, description)
            .await
            .unwrap();
        assert_eq!(imds_data, Some(response_body.as_bytes().to_vec()));
    }

    #[tokio::test]
    async fn fetch_userdata() {
        let server = Server::run();
        let port = server.addr().port();
        let base_uri = format!("http://localhost:{}", port);
        let token = "some+token";
        let response_code = 200;
        let response_body = r#"settings.motd = "Welcome to Bottlerocket!""#;
        server.expect(
            Expectation::matching(request::method_path("PUT", "/latest/api/token"))
                .times(1)
                .respond_with(
                    status_code(200)
                        .append_header("X-aws-ec2-metadata-token-ttl-seconds", "60")
                        .body(token),
                ),
        );
        server.expect(
            Expectation::matching(request::method_path(
                "GET",
                format!("/{}/user-data", SCHEMA_VERSION),
            ))
            .times(1)
            .respond_with(
                status_code(response_code)
                    .append_header("X-aws-ec2-metadata-token", token)
                    .body(response_body),
            ),
        );
        let mut imds_client = ImdsClient::new_impl(base_uri).await.unwrap();
        let imds_data = imds_client.fetch_userdata().await.unwrap();
        assert_eq!(imds_data, Some(response_body.as_bytes().to_vec()));
    }

    #[test]
    fn printable_string_short() {
        let input = "Hello".as_bytes();
        let expected = "Hello".to_string();
        let actual = printable_string(input);
        assert_eq!(expected, actual);
    }

    #[test]
    fn printable_string_binary() {
        let input: [u8; 5] = [0, 254, 1, 0, 4];
        let expected = "<binary>".to_string();
        let actual = printable_string(&input);
        assert_eq!(expected, actual);
    }

    #[test]
    fn printable_string_untruncated() {
        let mut input = String::new();
        for _ in 0..2047 {
            input.push('.');
        }
        let expected = input.clone();
        let actual = printable_string(input.as_bytes());
        assert_eq!(expected, actual);
    }

    #[test]
    fn printable_string_truncated() {
        let mut input = String::new();
        for _ in 0..2048 {
            input.push('.');
        }
        let mut expected = String::new();
        for _ in 0..2034 {
            expected.push('.');
        }
        expected.push_str("<truncated...>");
        let actual = printable_string(input.as_bytes());
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_public_key_list() {
        let list = r#"0=zero
1=one
2=two"#;
        let parsed_list = build_public_key_targets(list);
        assert_eq!(3, parsed_list.len());
        assert_eq!("public-keys/0/openssh-key", parsed_list.get(0).unwrap());
        assert_eq!("public-keys/1/openssh-key", parsed_list.get(1).unwrap());
        assert_eq!("public-keys/2/openssh-key", parsed_list.get(2).unwrap());
    }
}
