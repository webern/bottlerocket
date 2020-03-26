# logdog

Current version: 0.1.0

## Introduction

`logdog` is a program that gathers logs from various places on a Bottlerocket host and combines them
into a tarball for easy export.

Usage example:
```bash
$ logdog
logs are at: /tmp/bottlerocket-logs.tar.gz
```

### TODO

 [x] journalctl-list-boots is instead errors
 [x] dmesg not working
 [x] iptables missing
 [ ] customer readme instructions ssh
 [ ] customer readme instructions ssm
 [ ] unit tests

## Colophon

This text was generated from `README.tpl` using [cargo-readme](https://crates.io/crates/cargo-readme), and includes the rustdoc from `src/main.rs`.