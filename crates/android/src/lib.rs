//! Android entry point for Q Dungeon Prime.
//!
//! `cargo-apk` packages this cdylib. The `bevy_main` macro installs the
//! NativeActivity glue Bevy 0.13 expects and then starts the shared game loop.

#[cfg(target_os = "android")]
#[bevy::prelude::bevy_main]
fn main() {
    install_crash_hook();
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

#[cfg(target_os = "android")]
fn install_crash_hook() {
    std::panic::set_hook(Box::new(|info| {
        let payload = panic_payload(info);
        let location = info
            .location()
            .map(|location| {
                format!(
                    "{}:{}:{}",
                    location.file(),
                    location.line(),
                    location.column()
                )
            })
            .unwrap_or_else(|| "unknown location".to_string());
        let backtrace = std::backtrace::Backtrace::force_capture();
        let text = format!("Q Dungeon Prime crash\n{location}\n{payload}\n{backtrace}\n");
        log_android(&text);
        write_crash_files(&text);
    }));
}

#[cfg(target_os = "android")]
fn panic_payload(info: &std::panic::PanicHookInfo) -> String {
    if let Some(message) = info.payload().downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = info.payload().downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

#[cfg(target_os = "android")]
fn log_android(text: &str) {
    let tag = std::ffi::CString::new("QDungeonPrime").expect("tag");
    // ANDROID_LOG_ERROR
    const ANDROID_LOG_ERROR: i32 = 6;
    for line in text.lines() {
        let Ok(line) = std::ffi::CString::new(line) else {
            continue;
        };
        unsafe {
            __android_log_write(ANDROID_LOG_ERROR, tag.as_ptr(), line.as_ptr());
        }
    }
}

#[cfg(target_os = "android")]
fn write_crash_files(text: &str) {
    let Some(app) = bevy::winit::ANDROID_APP.get() else {
        return;
    };
    for dir in [app.external_data_path(), app.internal_data_path()]
        .into_iter()
        .flatten()
    {
        let path = dir.join("crash.txt");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(&path, text);
    }
}

#[cfg(target_os = "android")]
#[link(name = "log")]
extern "C" {
    fn __android_log_write(
        prio: i32,
        tag: *const std::os::raw::c_char,
        text: *const std::os::raw::c_char,
    ) -> i32;
}
