// Copyright 2019 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: MIT OR Apache-2.0

#![deny(rust_2018_idioms)]

mod error;
mod exec_to_file;

use crate::error::Result;
use std::path::PathBuf;
use std::str::FromStr;


fn main() -> Result<()> {
    crate::exec_to_file::exec_to_file(crate::exec_to_file::ExecToFile {
        command: "echo",
        args: vec!("arg1", "arg2"),
        output_filename: "x.log",
        output_dir: PathBuf::from_str("/tmp").unwrap(),
    })
}
