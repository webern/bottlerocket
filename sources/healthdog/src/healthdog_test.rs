use crate::config::Config;
use crate::error::Result;
use crate::healthcheck::{ServiceCheck, ServiceHealth};
use crate::healthdog::Healthdog;
use bottlerocket_release::BottlerocketRelease;
use httptest::{matchers::*, responders::*, Expectation, Server};
use tempfile::TempDir;

const OS_RELEASE: &str = r#"NAME=Bottlerocket
ID=bottlerocket
PRETTY_NAME="Bottlerocket OS 0.4.0"
VARIANT_ID=aws-k8s-1.16
VERSION_ID=0.4.0
BUILD_ID=7303622
"#;

fn os_release() -> BottlerocketRelease {
    let td = TempDir::new().unwrap();
    let path = td.path().join("os-release");
    std::fs::write(&path, OS_RELEASE).unwrap();
    BottlerocketRelease::from_file(&path).unwrap()
}

struct TestCheck {}

impl ServiceCheck for TestCheck {
    fn check(&self, service_name: &str) -> Result<ServiceHealth> {
        if service_name.ends_with("fail") {
            Ok(ServiceHealth {
                is_healthy: false,
                exit_code: Some(1),
            })
        } else if service_name.ends_with("error") {
            Err(crate::error::Error::Usage { message: None })
        } else {
            Ok(ServiceHealth {
                is_healthy: true,
                exit_code: None,
            })
        }
    }
}

#[test]
fn send_healthy_ping() {
    let server = Server::run();
    let matcher = all_of![
        request::method_path("GET", "/metrics"),
        request::query(url_decoded(contains(("sender", "healthdog")))),
        request::query(url_decoded(contains(("event", "health-ping")))),
        request::query(url_decoded(contains(("version", "0.4.0")))),
        request::query(url_decoded(contains(("variant", "aws-k8s-1.16")))),
        request::query(url_decoded(contains(("arch", "x86_64")))),
        request::query(url_decoded(contains(("region", "us-east-1")))),
        request::query(url_decoded(contains(("seed", "2041")))),
        request::query(url_decoded(contains(("is_healthy", "true")))),
        request::query(url_decoded(contains(("failed_services", "")))),
    ];
    server.expect(Expectation::matching(matcher).respond_with(status_code(200)));
    let port = server.addr().port();
    let healthdog = Healthdog::from_parts(
        Some(Config {
            metrics_url: format!("http://localhost:{}/metrics", port),
            send_metrics: true,
            service_health: vec![
                String::from("service_a"),
                String::from("service_b"),
                String::from("service_c"),
            ],
            region: String::from("us-east-1"),
            seed: 2041,
            version_lock: String::from("latest"),
            ignore_waves: false,
        }),
        Some(os_release()),
        Some(Box::new(TestCheck {})),
    )
    .unwrap();
    healthdog.send_health_ping();
}
