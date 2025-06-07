use std::path::Path;
use std::{env, fs};

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    let workspace_root = Path::new(&manifest_dir).parent().unwrap();
    let workspace_crates = fs::read_dir(workspace_root)
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.join("Cargo.toml").exists() {
                Some(entry.file_name().into_string().ok()?)
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join(",");

    println!("cargo:rustc-env=WORKSPACE_CRATES={workspace_crates}");
}
