#![deny(rust_2018_idioms)]

mod error;
use crate::error::{Result, Error};
// use crate::error::Result;

use std::process::{Command, Stdio};
use std::fs::File;
use snafu::ResultExt;
use crate::error::FileError;
use std::path::PathBuf;
use std::str::FromStr;
use std::fs;

const TEST_SCRIPT: &str = r####"#!/bin/bash
echo "1 stdout"
>&2 echo "2 stderr"
echo "3 stdout"
>&2 echo "4 stderr"
echo "5 stdout"
"####;

fn main() -> Result<()> {
    // let result = Command::new("echo").arg("Hello World".into()).stderr_to_stdout().output();
    // match result {
    //     Err(e) => return Err(Error::SomethingHappened { message: "Failed to echo".to_string(), source: e }),
    //     Ok(o) => {
    //
    //     }
    // }
    // Err(Error::ErrorMessage { message: "Hello error.".to_string() })
    let outputs = File::create("out.txt").context(crate::error::FileError { path: "out.txt".to_string()})?;
    let errors = outputs.try_clone().context(crate::error::FileError { path: "out.txt".to_string()})?;

    let pbuf = PathBuf::from_str("./test-script.sh").unwrap();
    let pbuf = fs::canonicalize(&pbuf).unwrap();

    Command::new("/bin/bash")
        .args(&[ "-c", pbuf.to_str().unwrap()])
        .stdout(Stdio::from(outputs))
        .stderr(Stdio::from(errors))
        .spawn().context(crate::error::FileError { path: "out.txt".to_string()})?
        .wait_with_output().context(crate::error::FileError { path: "out.txt".to_string()})?;

    Ok(())
}
