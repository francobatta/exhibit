//! Bakes Exhibit's own TMDB key into the build, from `EXHIBIT_TMDB_KEY` or the git-ignored
//! file `tmdb-app-key` at the workspace root, so the owner never has to find one. It is
//! scrambled only so it is not plain text in the exe: anyone determined can recover it (every
//! request carries it), so it is a convenience, never a secret. Without either, the build has
//! no key of its own and asks for one, as before.
//!
//! A key file added after a build without one is noticed once `exhibit-core` is rebuilt
//! (`cargo clean -p exhibit-core`); the release script checks for it.

use std::path::PathBuf;

/// Must match `tmdb::PAD`.
const PAD: &[u8] = b"exhibit-gallery";

fn main() {
    println!("cargo:rerun-if-env-changed=EXHIBIT_TMDB_KEY");
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let file = root.join("..").join("..").join("tmdb-app-key");
    if file.is_file() {
        println!("cargo:rerun-if-changed={}", file.display());
    }
    let key = std::env::var("EXHIBIT_TMDB_KEY")
        .ok()
        .or_else(|| std::fs::read_to_string(&file).ok())
        .map(|k| k.trim().to_string())
        .unwrap_or_default();
    let scrambled: String = key
        .bytes()
        .enumerate()
        .map(|(i, b)| format!("{:02x}", b ^ PAD[i % PAD.len()]))
        .collect();
    println!("cargo:rustc-env=EXHIBIT_TMDB_BUILTIN={scrambled}");
}
