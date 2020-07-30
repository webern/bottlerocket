use crate::error::{Error, Result};
use crate::main_inner;
use crate::service_check::{ServiceCheck, ServiceHealth};
use httptest::responders::status_code;
use httptest::{matchers::*, Expectation, Server};
use std::fs::write;
use tempfile::TempDir;

const OS_RELEASE: &str = r#"PRETTY_NAME=Bottlerocket,
VARIANT_ID=myvariant
VERSION_ID=1.2.3
BUILD_ID=abcdef0
"#;

struct MockCheck {}

impl ServiceCheck for MockCheck {
    fn check(&self, service_name: &str) -> Result<ServiceHealth> {
        if service_name.ends_with("failed") {
            Ok(ServiceHealth {
                is_healthy: false,
                exit_code: Some(1),
            })
        } else {
            Ok(ServiceHealth {
                is_healthy: true,
                exit_code: None,
            })
        }
    }
}

// dynamically create a config file where we can set server port, list of services, and send_metrics
fn create_config_file_contents(port: u16, services: &[&str], send_metrics: bool) -> String {
    let mut svcs = String::new();
    for (i, &service) in services.iter().enumerate() {
        if i == 0 {
            svcs.push('"')
        }
        svcs.push_str(service);
        if i == services.len() - 1 {
            svcs.push('"');
        } else {
            svcs.push_str("\",\"");
        }
    }
    format!(
        r#"
    metrics_url = "http://localhost:{}/metrics"
    send_metrics = {}
    service_health = [{}]
    region = "us-west-2"
    seed = 1234
    version_lock = "v0.1.2"
    ignore_waves = false
    "#,
        port, send_metrics, svcs
    )
}

// create the config and os-release files in a tempdir and return the tempdir
fn create_test_files(port: u16, services: &[&str], send_metrics: bool) -> TempDir {
    let t = TempDir::new().unwrap();
    write(
        t.path().join("healthdog.toml"),
        create_config_file_contents(port, services, send_metrics),
    )
    .unwrap();
    write(t.path().join("os-release"), OS_RELEASE).unwrap();
    t
}

// create the path to the config in the tempdir
fn config_path(tempdir: &TempDir) -> String {
    tempdir
        .path()
        .join("healthdog.toml")
        .to_str()
        .unwrap()
        .to_owned()
}

// create the path to os-release in the tempdir
fn os_release_path(tempdir: &TempDir) -> String {
    tempdir
        .path()
        .join("os-release")
        .to_str()
        .unwrap()
        .to_owned()
}

#[test]
fn send_boot_success_happy() {
    let server = Server::run();
    server.expect(
        Expectation::matching(request::method_path("GET", "/metrics"))
            .respond_with(status_code(200)),
    );
    let port = server.addr().port();
    let tempdir = create_test_files(port, &["a", "b"], true);
    let args = vec![
        String::from("healthdog"),
        String::from("send-boot-success"),
        String::from("--config"),
        config_path(&tempdir),
        String::from("--os-release"),
        os_release_path(&tempdir),
    ];
    main_inner(args.iter().cloned(), Box::new(MockCheck {})).unwrap();
}

#[test]
/// assert that a request is NOT sent to the server when the user sets `send_metrics` to false
fn send_boot_success_opt_out() {
    let server = Server::run();
    server.expect(
        Expectation::matching(request::method_path("GET", "/metrics"))
            .times(0)
            .respond_with(status_code(200)),
    );
    let port = server.addr().port();
    let tempdir = create_test_files(port, &[], false);
    let args = vec![
        String::from("healthdog"),
        String::from("send-boot-success"),
        String::from("--config"),
        config_path(&tempdir),
        String::from("--os-release"),
        os_release_path(&tempdir),
    ];
    main_inner(args.iter().cloned(), Box::new(MockCheck {})).unwrap();
}

#[test]
/// assert that send-boot-success exits without error even when there is no HTTP server
fn send_boot_success_no_server() {
    let port = 0;
    let tempdir = create_test_files(port, &[], true);
    let args = vec![
        String::from("healthdog"),
        String::from("send-boot-success"),
        String::from("--config"),
        config_path(&tempdir),
        String::from("--os-release"),
        os_release_path(&tempdir),
    ];
    main_inner(args.iter().cloned(), Box::new(MockCheck {})).unwrap();
}

#[test]
/// assert that a the program will exit 0 even if the server sends a 404
fn send_boot_success_404() {
    let server = Server::run();
    server.expect(
        Expectation::matching(request::method_path("GET", "/metrics"))
            .respond_with(status_code(404)),
    );
    let port = server.addr().port();
    let tempdir = create_test_files(port, &[], true);
    let args = vec![
        String::from("healthdog"),
        String::from("send-boot-success"),
        String::from("--config"),
        config_path(&tempdir),
        String::from("--os-release"),
        os_release_path(&tempdir),
    ];
    main_inner(args.iter().cloned(), Box::new(MockCheck {})).unwrap();
}

#[test]
/// assert that a the program will exit 0 even if the server sends a 404
fn usage_error() {
    let args = vec![String::from("healthdog"), String::from("bad-command")];
    let err = main_inner(args.iter().cloned(), Box::new(MockCheck {}))
        .err()
        .unwrap();
    match err {
        Error::Usage { message: msg } => assert!(msg.unwrap().contains("bad-command")),
        bad => panic!("incorrect error type, expected Error::Usage, got {}", bad),
    }
}

#[test]
fn send_health_ping() {
    let server = Server::run();
    let matcher = all_of![
        request::method_path("GET", "/metrics"),
        request::query(url_decoded(contains(("is_healthy", "false")))),
        request::query(url_decoded(contains(("failed_services", "afailed:1")))),
    ];
    server.expect(Expectation::matching(matcher).respond_with(status_code(200)));
    let port = server.addr().port();
    let tempdir = create_test_files(port, &["afailed", "b"], true);
    let args = vec![
        String::from("healthdog"),
        String::from("send-health-ping"),
        String::from("--config"),
        config_path(&tempdir),
        String::from("--os-release"),
        os_release_path(&tempdir),
        String::from("--log-level"),
        String::from("error"),
    ];
    main_inner(args.iter().cloned(), Box::new(MockCheck {})).unwrap();
}
