//! Android entry point for Q Dungeon Prime.
//!
//! `cargo-apk` packages this cdylib. The `bevy_main` macro installs the
//! NativeActivity glue Bevy 0.13 expects and then starts the shared game loop.

#[cfg(target_os = "android")]
#[bevy::prelude::bevy_main]
fn main() {
    de_game::run();
}

// Host builds of this crate (CI `cargo udeps` / `cargo build --workspace`)
// never enter `android_main`. Reference the dependencies so they stay used.
#[cfg(not(target_os = "android"))]
#[allow(dead_code)]
fn desktop_dependency_anchor() {
    let _ = bevy::prelude::App::new;
    let _ = de_game::run;
}
