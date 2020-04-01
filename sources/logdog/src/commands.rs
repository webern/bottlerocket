// Copyright 2020 Amazon.com, Inc. or its affiliates. All Rights Reserved.

/// Contains the list of command that `logdog` will run.

/// Returns the standard list of `logdog` commands.
pub(crate) fn commands() -> Vec<(&'static str, &'static str)> {
    vec![
        ("os-release", "cat /etc/os-release"),
        ("journalctl-boots", "journalctl --list-boots --no-pager"),
        ("journalctl.errors", "journalctl -p err -a --no-pager"),
        ("journalctl.log", "journalctl -a --no-pager"),
        ("signpost", "signpost status"),
        ("settings.json", "apiclient --method GET --uri /"),
        ("wicked", "wicked show all"),
        ("containerd-config", "containerd config dump"),
        ("kube-status", "systemctl status kube* -l --no-pager"),
        ("dmesg", "dmesg --color=never --nopager"),
        ("iptables-filter", "iptables -nvL -t filter"),
        ("iptables-nat", "iptables -nvL -t nat"),
        ("df", "df -h"),
        ("df-inodes", "df -i"),
    ]
}
