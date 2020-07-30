// src/variant/current is a link to the API model we actually want to build; this build.rs creates
// that symlink based on the VARIANT environment variable, which either comes from the build
// system or the user, if doing a local `cargo build`.
//
// See README.md to understand the symlink setup.

use std::env;
use std::fs::{self, File};
use std::io::{self, Write};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process;

fn symlink_force<P1, P2>(target: P1, link: P2) -> io::Result<()>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>,
{
    // Remove link if it already exists
    if let Err(e) = fs::remove_file(&link) {
        if e.kind() != io::ErrorKind::NotFound {
            return Err(e);
        }
    }
    // Link to requested target
    symlink(&target, &link)
}

fn link_current_variant() {
    // The VARIANT variable is originally BUILDSYS_VARIANT, set in the top-level Makefile.toml,
    // and is passed through as VARIANT by the top-level Dockerfile.  It represents which OS
    // variant we're building, and therefore which API model to use.
    let var = "VARIANT";
    println!("cargo:rerun-if-env-changed={}", var);
    let variant = env::var(var).unwrap_or_else(|_| {
        eprintln!("For local builds, you must set the {} environment variable so we know which API model to build against.  Valid values are the directories in variants/, for example \"aws-k8s-1.17\".", var);
        process::exit(1);
    });

    // Point to the source for the requested variant
    let variant_link = "src/variant/current";
    let variant_target = format!("../{}", variant);

    // Make sure requested variant exists
    let variant_path = format!("src/{}", variant);
    if !Path::new(&variant_path).exists() {
        eprintln!("The environment variable {} should refer to a directory under sources/models/src with an API model, but it's set to '{}' which doesn't exist", var, variant);
        process::exit(1);
    }

    // Create the symlink for the following `cargo build` to use for its source code
    symlink_force(&variant_target, variant_link).unwrap_or_else(|e| {
        eprintln!("Failed to create symlink at '{}' pointing to '{}' - we need this to support different API models for different variants.  Error: {}", variant_link, variant_target, e);
        process::exit(1);
    });

    // Also create the link for mod.rs so Rust can import source from the "current" link
    // created above.
    let mod_link = "src/variant/mod.rs";
    let mod_target = "../variant_mod.rs";
    symlink_force(&mod_target, mod_link).unwrap_or_else(|e| {
        eprintln!("Failed to create symlink at '{}' pointing to '{}' - we need this to build a Rust module structure through the `current` link.  Error: {}", mod_link, mod_target, e);
        process::exit(1);
    });
}

fn generate_readme() {
    // Check for environment variable "SKIP_README". If it is set,
    // skip README generation
    if env::var_os("SKIP_README").is_some() {
        return;
    }

    let mut lib = File::open("src/lib.rs").unwrap();
    let mut template = File::open("README.tpl").unwrap();

    let content = cargo_readme::generate_readme(
        &PathBuf::from("."), // root
        &mut lib,            // source
        Some(&mut template), // template
        // The "add x" arguments don't apply when using a template.
        true,  // add title
        false, // add badges
        false, // add license
        true,  // indent headings
    )
    .unwrap();

    let mut readme = File::create("README.md").unwrap();
    readme.write_all(content.as_bytes()).unwrap();
}

fn main() {
    generate_readme();
    link_current_variant();
}
