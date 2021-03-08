/*!
The imdsclient library provides high-level methods to interact with the AWS Instance Metadata Service.
The high-level methods provided are [`fetch_dynamic`], [`fetch_metadata`], and [`fetch_userdata`].

For more control, and to query IMDS without high-level wrappers, there is also a [`fetch_imds`] method.
This method is useful for specifying things like a pinned date for the IMDS schema version.
*/

#![deny(rust_2018_idioms)]

use http::StatusCode;
use log::{debug, info, trace};
use reqwest::Client;
use snafu::{ensure, ResultExt};

const IMDS_BASE_URI: &str = "http://169.254.169.254";
const IMDS_PINNED_DATE: &str = "2021-01-03";

// Currently only able to get fetch session tokens from `latest`
const IMDS_SESSION_TARGET: &str = "latest/api/token";

/// A client for making IMDSv2 queries.
/// It obtains a session token when it is first instantiated and is reused between helper functions.
pub struct ImdsClient {
    client: Client,
    imds_base_uri: String,
    session_token: String,
}

impl ImdsClient {
    pub async fn new() -> Result<Self> {
        Self::new_impl(IMDS_BASE_URI.to_string()).await
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

    /// Helper to fetch `dynamic` targets from IMDS, preferring an override file if present.
    pub async fn fetch_dynamic<S1, S2>(
        &mut self,
        end_target: S1,
        description: S2,
    ) -> Result<Option<Vec<u8>>>
    where
        S1: AsRef<str>,
        S2: AsRef<str>,
    {
        let dynamic_target = format!("dynamic/{}", end_target.as_ref());
        self.fetch_imds(IMDS_PINNED_DATE, &dynamic_target, description.as_ref())
            .await
    }

    /// Helper to fetch `meta-data` targets from IMDS, preferring an override file if present.
    pub async fn fetch_metadata<S1, S2>(
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
            .fetch_imds(IMDS_PINNED_DATE, &metadata_target, description.as_ref())
            .await?
        {
            Some(metadata_body) => Ok(Some(
                String::from_utf8(metadata_body).context(error::NonUtf8Response)?,
            )),
            None => Ok(None),
        }
    }

    /// Helper to fetch `user-data` from IMDS, preferring an override file if present.
    pub async fn fetch_userdata(&mut self) -> Result<Option<Vec<u8>>> {
        self.fetch_imds(IMDS_PINNED_DATE, "user-data", "user-data")
            .await
    }

    /// Fetch data from IMDS, preferring an override file if present.
    pub async fn fetch_imds<S1, S2, S3>(
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
        let mut attempt: u8 = 0;
        let max_attempts: u8 = 2;
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

            // IMDS data can be larger than we'd want to log (50k+ compressed) so we don't necessarily
            // want to show the whole thing, and don't want to show binary data.
            fn response_string(response: &[u8]) -> String {
                // arbitrary max len; would be nice to print the start of the data if it's
                // uncompressed, but we'd need to break slice at a safe point for UTF-8, and without
                // reading in the whole thing like String::from_utf8.
                if response.len() > 2048 {
                    "<very long>".to_string()
                } else if let Ok(s) = String::from_utf8(response.into()) {
                    s
                } else {
                    "<binary>".to_string()
                }
            }

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

                    let response_str = response_string(&response_body);
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

                    let response_str = response_string(&response_body);

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
    pub async fn refresh_token(&mut self) -> Result<()> {
        self.session_token = fetch_token(&self.client, &self.imds_base_uri).await?;
        Ok(())
    }
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
    }
}

pub use error::Error;
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod test {
    use super::{ImdsClient, IMDS_PINNED_DATE};
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
                format!("/{}/meta-data/{}", IMDS_PINNED_DATE, end_target),
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
                format!("/{}/dynamic/{}", IMDS_PINNED_DATE, end_target),
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
                format!("/{}/user-data", IMDS_PINNED_DATE),
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
}
