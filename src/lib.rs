use std::time::Duration;

use bevy::log::LogPlugin;
#[cfg(not(any(target_os = "android", target_os = "macos")))]
use bevy::window::{CursorGrabMode, PrimaryWindow};
use bevy::{
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    prelude::*,
    window::WindowMode,
};
use bevy_kira_audio::AudioPlugin;
use de_audio::AudioPluginGroup;
use de_behaviour::BehaviourPluginGroup;
use de_camera::CameraPluginGroup;
use de_combat::CombatPluginGroup;
use de_conf::ConfigPluginGroup;
use de_construction::ConstructionPluginGroup;
use de_controller::ControllerPluginGroup;
use de_core::{state::AppState, transition::DeStateTransition, CorePluginGroup};
use de_energy::EnergyPluginGroup;
use de_gui::GuiPluginGroup;
use de_index::IndexPluginGroup;
use de_loader::LoaderPluginGroup;
#[cfg(not(target_os = "android"))]
use de_lobby_client::LobbyClientPluginGroup;
use de_log::LogPluginGroup;
use de_menu::MenuPluginGroup;
use de_movement::MovementPluginGroup;
use de_multiplayer::MultiplayerPluginGroup;
use de_objects::ObjectsPluginGroup;
use de_pathing::PathingPluginGroup;
use de_signs::SignsPluginGroup;
use de_spawner::SpawnerPluginGroup;
use de_terrain::TerrainPluginGroup;
use tracing::{span, Level};

const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
const GIT_SHA: &str = env!("GIT_SHA");

pub fn run() {
    let mut app = App::new();
    // we want logging as early as possible
    app.add_plugins(LogPluginGroup);

    info!(
        "Starting Q Dungeon Prime {{ \"Version\": \"{}\", \"GitSha\": \"{}\" }}",
        CARGO_PKG_VERSION, GIT_SHA
    );

    {
        let span = span!(Level::TRACE, "Startup");
        let _enter = span.enter();

        #[cfg(not(target_os = "android"))]
        app.insert_resource(Msaa::Sample4);
        // MSAA panics on some Android devices with Bevy 0.13.
        #[cfg(target_os = "android")]
        app.insert_resource(Msaa::Off);

        app.add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Q Dungeon Prime".to_string(),
                        mode: WindowMode::BorderlessFullscreen,
                        ..Default::default()
                    }),
                    ..default()
                })
                .disable::<LogPlugin>(),
        )
        .add_plugins(AudioPlugin)
        .add_plugins((
            LogDiagnosticsPlugin {
                debug: false,
                wait_duration: Duration::from_secs(10),
                filter: None,
            },
            FrameTimeDiagnosticsPlugin,
            GamePlugin,
        ))
        .add_plugins(ConfigPluginGroup)
        .add_plugins(GuiPluginGroup);

        #[cfg(not(target_os = "android"))]
        app.add_plugins(LobbyClientPluginGroup);

        app.add_plugins(MenuPluginGroup)
            .add_plugins(CorePluginGroup)
            .add_plugins(EnergyPluginGroup)
            .add_plugins(ObjectsPluginGroup)
            .add_plugins(TerrainPluginGroup)
            .add_plugins(LoaderPluginGroup)
            .add_plugins(IndexPluginGroup)
            .add_plugins(PathingPluginGroup)
            .add_plugins(SignsPluginGroup)
            .add_plugins(SpawnerPluginGroup)
            .add_plugins(MovementPluginGroup)
            .add_plugins(ControllerPluginGroup)
            .add_plugins(CameraPluginGroup)
            .add_plugins(BehaviourPluginGroup)
            .add_plugins(CombatPluginGroup)
            .add_plugins(ConstructionPluginGroup)
            .add_plugins(AudioPluginGroup)
            .add_plugins(MultiplayerPluginGroup);
    }

    app.run();
}

struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_state_with_set::<AppState>();

        #[cfg(not(any(target_os = "android", target_os = "macos")))]
        {
            app.add_systems(OnEnter(AppState::AppLoading), cursor_grab_system);
        }
    }
}

#[cfg(not(any(target_os = "android", target_os = "macos")))]
fn cursor_grab_system(mut window_query: Query<&mut Window, With<PrimaryWindow>>) {
    let mut window = window_query.single_mut();
    window.cursor.grab_mode = CursorGrabMode::Confined;
}
