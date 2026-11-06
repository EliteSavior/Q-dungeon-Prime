use std::{env, fs, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let maps_dir = manifest_dir.join("../../assets/maps");
    println!("cargo:rerun-if-changed={}", maps_dir.display());

    let mut names = Vec::new();
    if let Ok(entries) = fs::read_dir(&maps_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.ends_with(".dem.tar") {
                names.push(name.into_owned());
            }
        }
    }
    names.sort();
    println!("cargo:rustc-env=DE_MAP_FILES={}", names.join("|"));
}
