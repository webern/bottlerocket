use rusoto_core::region::ParseRegionError;
use rusoto_core::{Region, RusotoError};
use rusoto_eks::{DescribeClusterError, Eks as RusotoEks, EksClient};
use snafu::{OptionExt, ResultExt, Snafu};
use std::str::FromStr;

#[derive(Debug, Snafu)]
pub(super) enum Error {
    #[snafu(display("Error describing cluster: {}", source))]
    DescribeCluster {
        source: RusotoError<DescribeClusterError>,
    },

    #[snafu(display("Cluster object is missing from EKS response"))]
    MissingCluster {},

    #[snafu(display("kubernetes_network_config is missing the service_ipv_4_cidr field"))]
    MissingIpv4Cidr {},

    #[snafu(display("Cluster object is missing the kubernetes_network_config field"))]
    MissingNetworkConfig {},

    #[snafu(display("Unable to parse '{}' as a region: {}", region, source))]
    RegionParse {
        region: String,
        source: ParseRegionError,
    },
}

type Result<T> = std::result::Result<T, Error>;

/// Returns the cluster's [serviceIPv4CIDR] DNS IP by calling the EKS API.
/// (https://docs.aws.amazon.com/eks/latest/APIReference/API_KubernetesNetworkConfigRequest.html)
pub(super) async fn get_cluster_cidr(region: &str, cluster: &str) -> Result<String> {
    let parsed_region = Region::from_str(region).context(RegionParse { region })?;
    let client = EksClient::new(parsed_region);
    let describe_cluster = rusoto_eks::DescribeClusterRequest {
        name: cluster.to_owned(),
    };
    client
        .describe_cluster(describe_cluster)
        .await
        .context(DescribeCluster {})?
        .cluster
        .context(MissingCluster)?
        .kubernetes_network_config
        .context(MissingNetworkConfig)?
        .service_ipv_4_cidr
        .context(MissingIpv4Cidr)
}
