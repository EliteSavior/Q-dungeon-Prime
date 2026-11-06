use std::path::{Path, PathBuf};

#[cfg(not(target_os = "android"))]
use async_std::stream::StreamExt;
use async_std::{fs, io};
use bevy::{
    prelude::*,
    tasks::{futures_lite::future, IoTaskPool, Task},
};
#[cfg(not(target_os = "android"))]
use de_core::assets::asset_path;
use de_core::{log_full_error, state::AppState};
use de_gui::{ButtonCommands, GuiCommands, OuterStyle};
#[cfg(not(target_os = "android"))]
use de_map::io::MAP_FILE_SUFFIX;
use de_map::{
    io::{load_metadata, MapLoadingError},
    meta::MapMetadata,
};
use thiserror::Error;

pub(crate) struct MapSelectionPlugin;

impl Plugin for MapSelectionPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<MapState>()
            .add_event::<SelectMapEvent>()
            .add_event::<MapSelectedEvent>()
            .add_systems(OnEnter(MapState::On), setup)
            .add_systems(OnExit(MapState::On), cleanup)
            .add_systems(
                Update,
                (
                    (init_buttons, button_system).run_if(in_state(MapState::On)),
                    select_map_system
                        .run_if(in_state(AppState::InMenu))
                        .run_if(on_event::<SelectMapEvent>()),
                ),
            );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, States)]
enum MapState {
    On,
    #[default]
    Off,
}

/// Send this event to display map selection on top of current UI.
#[derive(Event)]
pub(crate) struct SelectMapEvent;

/// This event is sent after a map is selected, just before menu state is
/// switched to a next state.
#[derive(Event)]
pub(crate) struct MapSelectedEvent {
    path: PathBuf,
    // Read by the desktop multiplayer menu, which is not compiled for Android.
    #[cfg_attr(target_os = "android", allow(dead_code))]
    metadata: MapMetadata,
}

impl MapSelectedEvent {
    fn new(path: PathBuf, metadata: MapMetadata) -> Self {
        Self { path, metadata }
    }

    /// Path to the map on the local file system.
    pub(crate) fn path(&self) -> &Path {
        self.path.as_path()
    }

    /// Selected map metadata.
    #[cfg_attr(target_os = "android", allow(dead_code))]
    pub(crate) fn metadata(&self) -> &MapMetadata {
        &self.metadata
    }
}

#[derive(Resource)]
struct PopUpNode(Entity);

#[derive(Resource)]
struct LoadingTask(Task<Result<Vec<MapEntry>, LoadingError>>);

#[derive(Component)]
struct MapEntry(PathBuf, MapMetadata);

impl MapEntry {
    fn new(path: PathBuf, meta: MapMetadata) -> Self {
        Self(path, meta)
    }

    fn path(&self) -> &Path {
        self.0.as_path()
    }

    fn metadata(&self) -> &MapMetadata {
        &self.1
    }
}

#[derive(Error, Debug)]
pub enum LoadingError {
    #[error(transparent)]
    Io { source: io::Error },
    #[error(transparent)]
    Map { source: MapLoadingError },
}

fn setup(mut commands: Commands) {
    let task = IoTaskPool::get().spawn(load_available_maps());
    commands.insert_resource(LoadingTask(task));

    let node_id = commands
        .spawn(NodeBundle {
            style: Style {
                position_type: PositionType::Absolute,
                top: Val::Percent(0.),
                bottom: Val::Percent(0.),
                left: Val::Percent(0.),
                right: Val::Percent(0.),
                width: Val::Percent(100.),
                height: Val::Percent(100.),
                ..default()
            },
            background_color: Color::GRAY.into(),
            z_index: ZIndex::Local(10),
            ..default()
        })
        .id();
    commands.insert_resource(PopUpNode(node_id));
}

fn init_buttons(
    mut commands: GuiCommands,
    node: Res<PopUpNode>,
    task: Option<ResMut<LoadingTask>>,
) {
    let Some(mut task) = task else { return };
    let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
        return;
    };

    let map_entries = match result {
        Ok(entries) => entries,
        Err(err) => {
            log_full_error!(err);
            panic!("{}", err);
        }
    };

    commands.remove_resource::<LoadingTask>();

    let column_node = commands
        .spawn(NodeBundle {
            style: Style {
                flex_direction: FlexDirection::Column,
                width: Val::Percent(25.),
                height: Val::Percent(100.),
                margin: UiRect::all(Val::Auto),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            ..default()
        })
        .id();

    commands.entity(node.0).add_child(column_node);

    for map in map_entries {
        let button = map_button(&mut commands, map);
        commands.entity(column_node).add_child(button);
    }
}

fn cleanup(mut commands: Commands, node: Res<PopUpNode>) {
    commands.remove_resource::<LoadingTask>();
    commands.entity(node.0).despawn_recursive();
}

fn button_system(
    mut next_state: ResMut<NextState<MapState>>,
    interactions: Query<(&Interaction, &MapEntry), Changed<Interaction>>,
    mut events: EventWriter<MapSelectedEvent>,
) {
    for (&interaction, map) in interactions.iter() {
        if let Interaction::Pressed = interaction {
            next_state.set(MapState::Off);
            events.send(MapSelectedEvent::new(
                map.path().into(),
                map.metadata().clone(),
            ));
        }
    }
}

async fn load_available_maps() -> Result<Vec<MapEntry>, LoadingError> {
    #[cfg(target_os = "android")]
    {
        return load_available_maps_from_apk().await;
    }
    #[cfg(not(target_os = "android"))]
    {
        load_available_maps_from_dir().await
    }
}

#[cfg(target_os = "android")]
async fn load_available_maps_from_apk() -> Result<Vec<MapEntry>, LoadingError> {
    use async_std::io::WriteExt;

    // Read the APK assets synchronously. `AAsset` is not `Send`, so it cannot
    // be held across `.await` inside the I/O task pool.
    let (maps_dir, packed_maps) = read_apk_maps()?;
    fs::create_dir_all(&maps_dir)
        .await
        .map_err(|source| LoadingError::Io { source })?;

    let mut map_entries = Vec::new();
    for (name, bytes) in packed_maps {
        let out_path = maps_dir.join(&name);
        let mut file = fs::File::create(&out_path)
            .await
            .map_err(|source| LoadingError::Io { source })?;
        file.write_all(&bytes)
            .await
            .map_err(|source| LoadingError::Io { source })?;
        drop(file);

        let metadata = load_metadata(out_path.as_path())
            .await
            .map_err(|source| LoadingError::Map { source })?;
        map_entries.push(MapEntry::new(out_path.into(), metadata));
    }

    map_entries.sort_by(|a, b| a.metadata().name().cmp(b.metadata().name()));
    Ok(map_entries)
}

#[cfg(target_os = "android")]
fn read_apk_maps() -> Result<(async_std::path::PathBuf, Vec<(String, Vec<u8>)>), LoadingError> {
    use std::ffi::CString;

    let android_app = bevy::winit::ANDROID_APP
        .get()
        .expect("Bevy must be setup with the #[bevy_main] macro on Android");
    let internal = android_app
        .internal_data_path()
        .ok_or_else(|| LoadingError::Io {
            source: io::Error::new(
                io::ErrorKind::NotFound,
                "Android internal data path is unavailable",
            ),
        })?;
    let maps_dir = async_std::path::PathBuf::from(internal).join("maps");

    let mut packed_maps = Vec::new();
    for name in env!("DE_MAP_FILES")
        .split('|')
        .filter(|name| !name.is_empty())
    {
        let asset_name = format!("maps/{name}");
        let c_name = CString::new(asset_name.clone()).map_err(|err| LoadingError::Io {
            source: io::Error::new(io::ErrorKind::InvalidInput, err.to_string()),
        })?;
        let bytes = {
            let mut opened =
                android_app
                    .asset_manager()
                    .open(&c_name)
                    .ok_or_else(|| LoadingError::Io {
                        source: io::Error::new(
                            io::ErrorKind::NotFound,
                            format!("APK asset missing: {asset_name}"),
                        ),
                    })?;
            opened
                .buffer()
                .map_err(|source| LoadingError::Io { source })?
                .to_vec()
        };
        packed_maps.push((name.to_owned(), bytes));
    }

    Ok((maps_dir, packed_maps))
}

#[cfg(not(target_os = "android"))]
async fn load_available_maps_from_dir() -> Result<Vec<MapEntry>, LoadingError> {
    let maps_dir = asset_path("maps");

    let mut map_entries = Vec::new();
    let mut dir_entries = match fs::read_dir(maps_dir).await {
        Ok(entries) => entries,
        Err(err) => return Err(LoadingError::Io { source: err }),
    };

    while let Some(res) = dir_entries.next().await {
        let path = match res {
            Ok(entry) => entry.path(),
            Err(err) => return Err(LoadingError::Io { source: err }),
        };

        if !path.is_file().await {
            continue;
        }
        if !path
            .file_name()
            .and_then(|n| n.to_str())
            .map_or(false, |n| n.ends_with(MAP_FILE_SUFFIX))
        {
            continue;
        }

        let metadata = match load_metadata(path.as_path()).await {
            Ok(meta) => meta,
            Err(err) => return Err(LoadingError::Map { source: err }),
        };
        map_entries.push(MapEntry::new(path.into(), metadata));
    }

    map_entries.sort_by(|a, b| a.metadata().name().cmp(b.metadata().name()));
    Ok(map_entries)
}

fn map_button(commands: &mut GuiCommands, map: MapEntry) -> Entity {
    commands
        .spawn_button(
            OuterStyle {
                width: Val::Percent(100.),
                height: Val::Percent(8.),
                margin: UiRect::new(
                    Val::Percent(0.),
                    Val::Percent(0.),
                    Val::Percent(2.),
                    Val::Percent(2.),
                ),
            },
            map.metadata().name(),
        )
        .insert(map)
        .id()
}

fn select_map_system(mut next_state: ResMut<NextState<MapState>>) {
    next_state.set(MapState::On);
}
