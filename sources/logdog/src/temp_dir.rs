// Copyright 2019 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fs::{remove_dir_all, create_dir_all};
use std::path::PathBuf;

/// Represents a self-cleaning (RAII) temporary directory. This is a simple inline implementation to
/// avoid bringing in unnecessary dependencies.
pub(crate) struct TempDir {
    path_buf: PathBuf,
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let result = remove_dir_all(&self.path_buf);
        if result.is_err() {
            println!(
                "Could not delete directory '{}': {}",
                self.path_buf.to_string_lossy(),
                result.err().unwrap()
            );
        }
    }
}

impl TempDir {
    /// Creates the directory if it does not already exist.
    pub(crate) fn new(path_buf: PathBuf) -> Result<TempDir, std::io::Error> {
        create_dir_all(&path_buf)?;
        Ok(TempDir { path_buf })
    }

    /// Gets the path.
    pub(crate) fn path(&self) -> PathBuf {
        self.path_buf.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use uuid::Uuid;

    use super::*;

    #[test]
    fn test() {
        let uuid = Uuid::new_v4().to_string();
        let path_buf = std::env::temp_dir().join(uuid);
        // Create a TempDir within a scope
        {
            let _temp_dir = TempDir::new(path_buf.clone()).unwrap();
            assert!(Path::new(&path_buf).exists())
        }
        assert!(!Path::new(&path_buf).exists())
    }
}