// Ensures the rust-embed source folder exists at build time. In a clean dev
// checkout `web/dist/` may not exist yet (the frontend hasn't been built);
// rust-embed would otherwise fail at compile time. Creating an empty folder
// with a `.gitkeep` placeholder keeps the embed path stable.
use std::fs;
use std::path::Path;

fn main() {
    let dist = Path::new("..").join("web").join("dist");
    if !dist.exists() {
        fs::create_dir_all(&dist).expect("create web/dist");
    }
    let keep = dist.join(".gitkeep");
    if !keep.exists() {
        fs::write(&keep, b"").expect("write web/dist/.gitkeep");
    }
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../web/dist");
}
