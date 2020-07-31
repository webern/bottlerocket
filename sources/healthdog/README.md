# healthdog

Current version: 0.1.0

## Introduction

Healthdog sends anonymous information about the health of a Bottlerocket host.
It does so by sending key-value pairs as query params in an HTTP GET request.

Healthdog also has the ability to check that a list of critical services is running.
It does so using `systemctl` and reports services that are not healthy.

## What it Sends

#### The standard set of metrics:

* `sender`: the application sending the report.
* `event`: the event that invoked the report.
* `version`: the Bottlerocket version.
* `variant`: the Bottlerocket variant.
* `arch`: the machine architecture, e.g.'x86_64' or 'arm'.
* `region`: the region the machine is running in.
* `seed`: the seed value used to roll-out updates.
* `version-lock`: the optional setting that locks Bottlerocket to a certain version.
* `ignore-waves`: an update setting that allows hosts to update before their seed is reached.

#### Additionally, when `healthdog` sends a 'health ping', it adds:

* `is-healthy`: true or false based on whether critical services are running.
* `failed_services`: a list of critical services that have failed, if any.

## Configuration

Configuration is read from a TOML file that looks like this:

```toml
# the url to which healthdog will send metrics information
metrics_url = "https://example.com/metrics"
# whether or not healthdog will send metrics. opt-out by setting this to false
send_metrics = true
# a list of systemd service names that will be checked
service_health = ["apiserver", "containerd", "kubelet"]
# the region
region = "us-west-2"
# the update wave seed
seed = 1234
# what version bottlerocket should stay on
version_lock = "latest"
# whether bottlerocket should ignore update roll-out timing
ignore_waves = false
```

## Colophon

This text was generated from `README.tpl` using [cargo-readme](https://crates.io/crates/cargo-readme), and includes the rustdoc from `src/main.rs`.