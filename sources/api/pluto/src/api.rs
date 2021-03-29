use snafu::{OptionExt, ResultExt, Snafu};

// FIXME Get these from configuration in the future
const DEFAULT_API_SOCKET: &str = "/run/api.sock";
const CLUSTER_NAME_URI: &str = "/settings?keys=settings.kubernetes.cluster-name";

#[derive(Debug, Snafu)]
pub(super) enum Error {
    #[snafu(display("Error calling Bottlerocket API: {}", source))]
    ApiClientError {
        source: apiclient::Error,
        uri: String,
    },

    #[snafu(display("The 'cluster-name' setting is missing"))]
    ClusterNameMissing {},

    #[snafu(display("The 'cluster-name' setting is not a string"))]
    ClusterNameType {},

    #[snafu(display("Kubernetes settings are missing"))]
    KubernetesKey {},

    #[snafu(display("Kubernetes settings are not a JSON object"))]
    KubernetesObject {},

    #[snafu(display("API response was not a JSON object"))]
    ResponseObject {},

    #[snafu(display("Unable to parse Bottlerocket API response as JSON: {}", source))]
    ResponseJsonParse { source: serde_json::Error },
}

/// The result type for the [`api`] module.
pub(super) type Result<T> = std::result::Result<T, Error>;

/// Gets the Kubernetes cluster name from the Bottlerocket API.
pub(super) async fn get_cluster_name() -> Result<String> {
    let (_, raw_response) =
        apiclient::raw_request(DEFAULT_API_SOCKET, CLUSTER_NAME_URI, "GET", None)
            .await
            .context(ApiClientError {
                uri: CLUSTER_NAME_URI,
            })?;
    let parsed_response: serde_json::Value =
        serde_json::from_str(&raw_response).context(ResponseJsonParse)?;

    Ok(parsed_response
        .as_object()
        .context(ResponseObject)?
        .get("kubernetes")
        .context(KubernetesKey)?
        .as_object()
        .context(KubernetesObject)?
        .get("cluster-name")
        .context(ClusterNameMissing)?
        .as_str()
        .context(ClusterNameType)?
        .to_owned())
}
