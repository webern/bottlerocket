# logdog

Current version: 0.1.0

## Introduction

`logdog` is a program that gathers logs from various places on a Bottlerocket host and combines them
into a tarball for easy export.

Usage example:
```rust
$ logdog
logs are at: /tmp/bottlerocket-logs.tar.gz
```

#### Logs Collected

`logdog` will aggregate the following information:

 * a copy of os-release to tell us the version and build of Bottlerocket
 * a list of boots that journalctl knows about
 * errors from journalctl
 * all log lines from journalctl
 * signpost status to tell us the status of grub and the boot partitions
 * Bottlerocket settings using the apiclient
 * networking status from wicked
 * configuration info from containerd
 * the status of kubelet and other kube processes from systemctl
 * the kernel message buffer with dmesg
 * firewall filtering information from iptables
 * firewall nat information from iptables.


## Colophon

This text was generated from `README.tpl` using [cargo-readme](https://crates.io/crates/cargo-readme), and includes the rustdoc from `src/main.rs`.