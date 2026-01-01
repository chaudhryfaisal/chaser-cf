//! Build script for chaser-cf
//!
//! Generates C header file using cbindgen

use std::env;
use std::path::PathBuf;

fn main() {
    // Only generate headers when building the cdylib/staticlib
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    // Generate C header
    let config = cbindgen::Config::from_file("cbindgen.toml").unwrap_or_default();

    cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(config)
        .generate()
        .map(|bindings| {
            let out_path = PathBuf::from(&crate_dir).join("include");
            std::fs::create_dir_all(&out_path).ok();
            bindings.write_to_file(out_path.join("chaser_cf.h"));
        })
        .ok();

    // Tell Cargo to rerun if FFI module changes
    println!("cargo:rerun-if-changed=src/ffi/mod.rs");
    println!("cargo:rerun-if-changed=cbindgen.toml");
}
