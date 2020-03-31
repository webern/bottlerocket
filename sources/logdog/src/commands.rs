// Copyright 2020 Amazon.com, Inc. or its affiliates. All Rights Reserved.

use crate::exec_to_file::ExecToFile;

/// Contains the list of command that `logdog` will run.

/// Produces the list of commands that we will run on the Bottlerocket host.
pub(crate) fn commands() -> Vec<ExecToFile> {
    vec![
        // a copy of os-release to tell us the version and build of Bottlerocket.
        ExecToFile {
            command: "cat",
            args: vec!["/etc/os-release"],
            output_filename: "os-release",
        },
        // Get a list of boots that journalctl knows about.
        ExecToFile {
            command: "journalctl",
            args: vec!["--list-boots", "--no-pager"],
            output_filename: "journalctl-list-boots",
        },
        // Get errors from journalctl.
        ExecToFile {
            command: "journalctl",
            args: vec!["-p", "err", "-a", "--no-pager"],
            output_filename: "journalctl.errors",
        },
        // Get all log lines from journalctl.
        ExecToFile {
            command: "journalctl",
            args: vec!["-a", "--no-pager"],
            output_filename: "journalctl.log",
        },
        // Get signpost status to tell us the status of grub and the boot partitions.
        ExecToFile {
            command: "signpost",
            args: vec!["status"],
            output_filename: "signpost",
        },
        // Get Bottlerocket settings using the apiclient.
        ExecToFile {
            command: "apiclient",
            args: vec!["--method", "GET", "--uri", "/"],
            output_filename: "settings.json",
        },
        // Get networking status from wicked.
        ExecToFile {
            command: "wicked",
            args: vec!["show", "all"],
            output_filename: "wicked",
        },
        // Get configuration info from containerd.
        ExecToFile {
            command: "containerd",
            args: vec!["config", "dump"],
            output_filename: "containerd-config",
        },
        // Get the status of kubelet and other kube processes from systemctl.
        ExecToFile {
            command: "systemctl",
            args: vec!["status", "kube*", "-l", "--no-pager"],
            output_filename: "kube-status",
        },
        // Get the kernel message buffer with dmesg.
        ExecToFile {
            command: "dmesg",
            args: vec!["--color=never", "--nopager"],
            output_filename: "dmesg",
        },
        // Get firewall filtering information from iptables.
        ExecToFile {
            command: "iptables",
            args: vec!["-nvL", "-t", "filter"],
            output_filename: "iptables-filter",
        },
        // Get firewall nat information from iptables.
        ExecToFile {
            command: "iptables",
            args: vec!["-nvL", "-t", "nat"],
            output_filename: "iptables-nat",
        },
        // Get disk and filesytem information from df.
        ExecToFile {
            command: "df",
            args: vec!["-h"],
            output_filename: "df",
        },
        // Get disk inode information from df.
        ExecToFile {
            command: "df",
            args: vec!["-i"],
            output_filename: "df-inodes",
        },
    ]
}
