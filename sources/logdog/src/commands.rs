// Copyright 2020 Amazon.com, Inc. or its affiliates. All Rights Reserved.

use crate::exec_to_file::ExecToFile;

/// Contains the list of command that `logdog` will run.

/// Produces the list of commands that we will run on the Bottlerocket host.
pub(crate) fn commands() -> Vec<ExecToFile> {
    vec![
        // a copy of os-release to tell us the version and build of Bottlerocket.
        ExecToFile {
            command: "cat".to_string(),
            args: vec!["/etc/os-release".to_string()],
            output_filename: "os-release".to_string(),
        },
        // Get a list of boots that journalctl knows about.
        ExecToFile {
            command: "journalctl".to_string(),
            args: vec!["--list-boots".to_string(), "--no-pager".to_string()],
            output_filename: "journalctl-list-boots".to_string(),
        },
        // Get errors from journalctl.
        ExecToFile {
            command: "journalctl".to_string(),
            args: vec![
                "-p".to_string(),
                "err".to_string(),
                "-a".to_string(),
                "--no-pager".to_string(),
            ],
            output_filename: "journalctl.errors".to_string(),
        },
        // Get all log lines from journalctl.
        ExecToFile {
            command: "journalctl".to_string(),
            args: vec!["-a".to_string(), "--no-pager".to_string()],
            output_filename: "journalctl.log".to_string(),
        },
        // Get signpost status to tell us the status of grub and the boot partitions.
        ExecToFile {
            command: "signpost".to_string(),
            args: vec!["status".to_string()],
            output_filename: "signpost".to_string(),
        },
        // Get Bottlerocket settings using the apiclient.
        ExecToFile {
            command: "apiclient".to_string(),
            args: vec![
                "--method".to_string(),
                "GET".to_string(),
                "--uri".to_string(),
                "/".to_string(),
            ],
            output_filename: "settings.json".to_string(),
        },
        // Get networking status from wicked.
        ExecToFile {
            command: "wicked".to_string(),
            args: vec!["show".to_string(), "all".to_string()],
            output_filename: "wicked".to_string(),
        },
        // Get configuration info from containerd.
        ExecToFile {
            command: "containerd".to_string(),
            args: vec!["config".to_string(), "dump".to_string()],
            output_filename: "containerd-config".to_string(),
        },
        // Get the status of kubelet and other kube processes from systemctl.
        ExecToFile {
            command: "systemctl".to_string(),
            args: vec![
                "status".to_string(),
                "kube*".to_string(),
                "-l".to_string(),
                "--no-pager".to_string(),
            ],
            output_filename: "kube-status".to_string(),
        },
        // Get the kernel message buffer with dmesg.
        ExecToFile {
            command: "dmesg".to_string(),
            args: vec!["--color=never".to_string(), "--nopager".to_string()],
            output_filename: "dmesg".to_string(),
        },
        // Get firewall filtering information from iptables.
        ExecToFile {
            command: "iptables".to_string(),
            args: vec!["-nvL".to_string(), "-t".to_string(), "filter".to_string()],
            output_filename: "iptables-filter".to_string(),
        },
        // Get firewall nat information from iptables.
        ExecToFile {
            command: "iptables".to_string(),
            args: vec!["-nvL".to_string(), "-t".to_string(), "nat".to_string()],
            output_filename: "iptables-nat".to_string(),
        },
        // Get disk and filesytem information from df.
        ExecToFile {
            command: "df".to_string(),
            args: vec!["-h".to_string()],
            output_filename: "df".to_string(),
        },
        // Get disk inode information from df.
        ExecToFile {
            command: "df".to_string(),
            args: vec!["-i".to_string()],
            output_filename: "df-inodes".to_string(),
        },
    ]
}
