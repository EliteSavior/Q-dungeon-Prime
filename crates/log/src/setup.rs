use std::path::PathBuf;

use bevy::prelude::*;
use de_core::fs::logs_dir;
use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{fmt, EnvFilter};

pub(crate) struct LogPlugin;

/// the handle for the guard (dropping it will disable the log writer)
#[derive(Resource)]
pub(crate) struct CurrentLogHandle {
    #[allow(dead_code)]
    guard: WorkerGuard,
}

struct FileLog {
    writer: tracing_appender::non_blocking::NonBlocking,
    guard: WorkerGuard,
    dir: PathBuf,
    path: PathBuf,
}

impl Plugin for LogPlugin {
    fn build(&self, app: &mut App) {
        let file_log = open_file_log();
        let file_layer = file_log
            .as_ref()
            .map(|log| fmt::layer().json().with_writer(log.writer.clone()));

        let collector = tracing_subscriber::registry()
            .with(
                EnvFilter::builder()
                    // defaults to INFO if RUST_LOG not set
                    .with_default_directive(Level::INFO.into())
                    .from_env_lossy(),
            )
            .with(fmt::layer().with_writer(std::io::stdout))
            .with(file_layer);
        tracing::subscriber::set_global_default(collector)
            .expect("Unable to set a global collector");

        if let Some(log) = file_log {
            info!("Logging to file {:?} in directory: {:?}", log.path, log.dir);
            app.insert_resource(CurrentLogHandle { guard: log.guard });
        } else {
            info!("File logging unavailable; writing to stderr/logcat only");
        }
    }
}

/// Opens a rolling log file when a writable logs directory exists.
///
/// Android native activities do not have a home directory, and the process
/// working directory is not writable. Falling back to a relative `logs/` path
/// makes `tracing-appender` panic during startup, so file logging is skipped.
fn open_file_log() -> Option<FileLog> {
    let dir: PathBuf = logs_dir().ok()?.into();
    std::fs::create_dir_all(&dir).ok()?;

    let dt = chrono::Local::now();
    let path: PathBuf = dt.format("%Y-%m-%d_%H-%M-%S.jsonl").to_string().into();
    let file_appender = tracing_appender::rolling::never(dir.as_path(), path.as_path());
    let (writer, guard) = tracing_appender::non_blocking(file_appender);
    Some(FileLog {
        writer,
        guard,
        dir,
        path,
    })
}
