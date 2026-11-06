//! On-screen controls that feed the same Bevy keyboard and mouse events the
//! desktop bindings already read.

use std::collections::HashMap;

use bevy::{
    input::{
        gamepad::{GamepadAxis, GamepadAxisType, GamepadButton, GamepadButtonType},
        keyboard::{Key, KeyboardInput, NativeKey},
        mouse::{MouseButtonInput, MouseMotion, MouseScrollUnit, MouseWheel},
        touch::TouchPhase,
        ButtonState,
    },
    prelude::*,
    window::PrimaryWindow,
};
use de_core::gamestate::GameState;

use crate::hud::PopUpMenu;

const DEADZONE: f32 = 0.22;
const CURSOR_SPEED: f32 = 720.;
const ROTATE_PIXELS: f32 = 14.;

pub(crate) struct TouchPlugin;

impl Plugin for TouchPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TouchControlState>()
            .add_systems(OnEnter(GameState::Playing), spawn_overlay)
            .add_systems(OnExit(GameState::Playing), despawn_overlay)
            .add_systems(
                Update,
                (
                    sync_overlay_visibility,
                    sync_stick_knobs,
                    sync_cursor_widget,
                    sync_world_controls,
                    log_overlay_layout,
                ),
            )
            // First runs before InputSchedule, so gameplay systems read these
            // events in the same frame a real key or mouse event would.
            .add_systems(
                First,
                apply_virtual_input.run_if(in_state(GameState::Playing)),
            );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ControlKind {
    Move,
    Aim,
    LeftClick,
    RightClick,
    ZoomIn,
    ZoomOut,
    RotateLeft,
    RotateRight,
    Pause,
    BuildBase,
    BuildPower,
}

#[derive(Clone, Copy, Debug)]
enum TouchRole {
    Control(ControlKind),
    Point,
}

#[derive(Resource, Default)]
struct TouchControlState {
    cursor: Option<Vec2>,
    touches: HashMap<u64, TouchRole>,
    move_stick: Vec2,
    aim_stick: Vec2,
    left_held: bool,
    right_held: bool,
    shift_held: bool,
    pause_down: bool,
    base_down: bool,
    power_down: bool,
    arrow: Option<KeyCode>,
}

#[derive(Component)]
struct TouchOverlay;

#[derive(Component)]
struct StickKnob(ControlKind);

#[derive(Component)]
struct CursorWidget;

fn spawn_overlay(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    ui_camera: Query<Entity, With<bevy::ui::IsDefaultUiCamera>>,
    world_camera: Query<(Entity, &Projection), With<Camera3d>>,
    window: Query<&Window, With<PrimaryWindow>>,
) {
    if !cfg!(target_os = "android") {
        return;
    }
    info!("Spawning touch control overlay");
    spawn_world_controls(&mut commands, &mut meshes, &mut materials, &world_camera, &window);

    let font = asset_server.load("fonts/Fira_Mono/FiraMono-Medium.ttf");
    let pad = Color::rgba(0.15, 0.55, 0.95, 0.5);
    let button = Color::rgba(0.05, 0.08, 0.12, 0.78);

    let mut overlay = commands.spawn((
        NodeBundle {
            style: Style {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.),
                height: Val::Percent(100.),
                ..default()
            },
            z_index: ZIndex::Global(2),
            ..default()
        },
        TouchOverlay,
    ));
    if let Ok(camera) = ui_camera.get_single() {
        overlay.insert(bevy::ui::TargetCamera(camera));
    }
    overlay
        .with_children(|parent| {
            circle(parent, pad, 4., 62., 16.);
            knob(parent, ControlKind::Move, 4., 62., 16.);
            circle(parent, pad, 80., 62., 16.);
            knob(parent, ControlKind::Aim, 80., 62., 16.);
            labeled(parent, &font, button, "L", 68., 58., 10., 12.);
            labeled(parent, &font, button, "R", 68., 74., 10., 12.);
            labeled(parent, &font, button, "+", 88., 6., 8., 10.);
            labeled(parent, &font, button, "-", 88., 18., 8., 10.);
            labeled(parent, &font, button, "RL", 76., 6., 10., 10.);
            labeled(parent, &font, button, "RR", 76., 18., 10., 10.);
            labeled(parent, &font, button, "Esc", 2., 4., 12., 10.);
            labeled(parent, &font, button, "B", 16., 4., 8., 10.);
            labeled(parent, &font, button, "P", 26., 4., 8., 10.);
            parent.spawn((
                NodeBundle {
                    style: Style {
                        position_type: PositionType::Absolute,
                        width: Val::Px(14.),
                        height: Val::Px(14.),
                        ..default()
                    },
                    background_color: Color::rgba(1.0, 0.95, 0.7, 0.9).into(),
                    z_index: ZIndex::Global(3),
                    ..default()
                },
                CursorWidget,
            ));
        });
}

#[derive(Component)]
struct WorldControl {
    local: Vec3,
    scale: Vec3,
}

#[derive(Component)]
struct WorldCursor {
    scale: Vec3,
}

#[derive(Resource)]
struct WorldOverlay {
    half: Vec2,
    distance: f32,
}

/// Quads parented to the world camera. The in-game UI pass produces no pixels
/// on this Android renderer, so the same layout is geometry in front of the lens.
fn spawn_world_controls(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    world_camera: &Query<(Entity, &Projection), With<Camera3d>>,
    window: &Query<&Window, With<PrimaryWindow>>,
) {
    let Ok((camera, projection)) = world_camera.get_single() else {
        info!("touch overlay has no world camera");
        return;
    };
    let Ok(window) = window.get_single() else {
        return;
    };
    const DISTANCE: f32 = 6.0;
    let (half_w, half_h) = match projection {
        Projection::Perspective(perspective) => {
            let half_h = DISTANCE * (perspective.fov * 0.5).tan();
            (half_h * perspective.aspect_ratio, half_h)
        }
        Projection::Orthographic(ortho) => (ortho.area.width() * 0.5, ortho.area.height() * 0.5),
    };
    info!(
        "touch world overlay window={}x{} half={}x{}",
        window.width(),
        window.height(),
        half_w,
        half_h
    );
    commands.insert_resource(WorldOverlay {
        half: Vec2::new(half_w, half_h),
        distance: DISTANCE,
    });

    let mesh = meshes.add(Cuboid::new(1.0, 1.0, 0.05));
    let mat = |materials: &mut Assets<StandardMaterial>, color: Color| {
        materials.add(StandardMaterial {
            base_color: color,
            emissive: color,
            perceptual_roughness: 1.0,
            cull_mode: None,
            ..default()
        })
    };
    let local_of = |fx: f32, fy: f32, w_frac: f32, h_frac: f32| {
        (
            Vec3::new((fx * 2. - 1.) * half_w, (1. - fy * 2.) * half_h, -DISTANCE),
            Vec3::new(
                (w_frac * 2. * half_w).max(0.01),
                (h_frac * 2. * half_h).max(0.01),
                1.,
            ),
        )
    };

    let move_mat = mat(materials, Color::rgb(0.1, 0.85, 1.0));
    let aim_mat = mat(materials, Color::rgb(1.0, 0.45, 0.05));
    let button_mat = mat(materials, Color::rgb(0.95, 0.95, 0.95));
    let cursor_mat = mat(materials, Color::rgb(1.0, 0.95, 0.2));
    let specs = [
        (0.12_f32, 0.72, 0.16, 0.22, move_mat),
        (0.88, 0.72, 0.16, 0.22, aim_mat),
        (0.73, 0.64, 0.09, 0.14, button_mat.clone()),
        (0.73, 0.80, 0.09, 0.14, button_mat.clone()),
        (0.92, 0.11, 0.07, 0.12, button_mat.clone()),
        (0.92, 0.23, 0.07, 0.12, button_mat.clone()),
        (0.81, 0.11, 0.08, 0.12, button_mat.clone()),
        (0.81, 0.23, 0.08, 0.12, button_mat.clone()),
        (0.08, 0.09, 0.10, 0.12, button_mat.clone()),
        (0.20, 0.09, 0.07, 0.12, button_mat.clone()),
        (0.30, 0.09, 0.07, 0.12, button_mat),
    ];
    for (fx, fy, w, h, material) in specs {
        let (local, scale) = local_of(fx, fy, w, h);
        commands.spawn((
            PbrBundle {
                mesh: mesh.clone(),
                material,
                inherited_visibility: bevy::render::view::InheritedVisibility::VISIBLE,
                ..default()
            },
            WorldControl { local, scale },
            bevy::render::view::NoFrustumCulling,
            bevy::pbr::NotShadowCaster,
        ));
    }
    let (_, cursor_scale) = local_of(0.5, 0.5, 0.02, 0.03);
    commands.spawn((
        PbrBundle {
            mesh,
            material: cursor_mat,
            inherited_visibility: bevy::render::view::InheritedVisibility::VISIBLE,
            ..default()
        },
        WorldCursor {
            scale: cursor_scale,
        },
        bevy::render::view::NoFrustumCulling,
        bevy::pbr::NotShadowCaster,
    ));
    let _ = camera;
}

fn sync_world_controls(
    camera: Query<&GlobalTransform, With<Camera3d>>,
    overlay: Option<Res<WorldOverlay>>,
    state: Res<TouchControlState>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut controls: Query<(&WorldControl, &mut Transform), Without<WorldCursor>>,
    mut cursor: Query<(&WorldCursor, &mut Transform), Without<WorldControl>>,
    mut gizmos: Gizmos,
    mut logged: Local<bool>,
) {
    let Ok(camera) = camera.get_single() else {
        return;
    };
    let rotation = camera.to_scale_rotation_translation().1;
    for (control, mut transform) in &mut controls {
        transform.translation = camera.transform_point(control.local);
        transform.rotation = rotation;
        transform.scale = control.scale;
        let center = transform.translation;
        let right = rotation * Vec3::X * control.scale.x * 0.5;
        let up = rotation * Vec3::Y * control.scale.y * 0.5;
        gizmos.line(center - right - up, center + right - up, Color::rgb(0.0, 1.0, 0.2));
        gizmos.line(center + right - up, center + right + up, Color::rgb(0.0, 1.0, 0.2));
        gizmos.line(center + right + up, center - right + up, Color::rgb(0.0, 1.0, 0.2));
        gizmos.line(center - right + up, center - right - up, Color::rgb(0.0, 1.0, 0.2));
        if !*logged {
            info!("touch control world pos {}", transform.translation);
            *logged = true;
        }
    }
    let Some(overlay) = overlay else {
        return;
    };
    let Ok(window) = window.get_single() else {
        return;
    };
    let Some(position) = state.cursor else {
        return;
    };
    let Ok((cursor, mut transform)) = cursor.get_single_mut() else {
        return;
    };
    let fx = position.x / window.width().max(1.);
    let fy = position.y / window.height().max(1.);
    let local = Vec3::new(
        (fx * 2. - 1.) * overlay.half.x,
        (1. - fy * 2.) * overlay.half.y,
        -overlay.distance,
    );
    transform.translation = camera.transform_point(local);
    transform.rotation = rotation;
    transform.scale = cursor.scale;
}


fn circle(parent: &mut ChildBuilder, color: Color, left: f32, top: f32, size: f32) {
    parent.spawn(NodeBundle {
        style: Style {
            position_type: PositionType::Absolute,
            left: Val::Percent(left),
            top: Val::Percent(top),
            width: Val::Percent(size),
            height: Val::Percent(size),
            ..default()
        },
        background_color: color.into(),
        ..default()
    });
}

fn knob(parent: &mut ChildBuilder, kind: ControlKind, left: f32, top: f32, size: f32) {
    parent.spawn((
        NodeBundle {
            style: Style {
                position_type: PositionType::Absolute,
                left: Val::Percent(left + size * 0.3),
                top: Val::Percent(top + size * 0.3),
                width: Val::Percent(size * 0.4),
                height: Val::Percent(size * 0.4),
                ..default()
            },
            background_color: Color::rgba(1.0, 1.0, 1.0, 0.55).into(),
            ..default()
        },
        StickKnob(kind),
    ));
}

fn labeled(
    parent: &mut ChildBuilder,
    font: &Handle<Font>,
    color: Color,
    text: &str,
    left: f32,
    top: f32,
    width: f32,
    height: f32,
) {
    parent
        .spawn(NodeBundle {
            style: Style {
                position_type: PositionType::Absolute,
                left: Val::Percent(left),
                top: Val::Percent(top),
                width: Val::Percent(width),
                height: Val::Percent(height),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            background_color: color.into(),
            ..default()
        })
        .with_children(|parent| {
            parent.spawn(TextBundle::from_section(
                text,
                TextStyle {
                    font: font.clone(),
                    font_size: 22.0,
                    color: Color::WHITE,
                },
            ));
        });
}

fn log_overlay_layout(
    mut frames: Local<u8>,
    overlay: Query<&Node, With<TouchOverlay>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<&Camera>,
) {
    if *frames >= 8 {
        return;
    }
    *frames += 1;
    if *frames != 8 {
        return;
    }
    let size = overlay.get_single().map(Node::size).ok();
    let window = windows
        .get_single()
        .ok()
        .map(|window| (window.width(), window.height(), window.scale_factor()));
    let viewports: Vec<_> = cameras
        .iter()
        .map(|camera| {
            (
                camera.order,
                camera.physical_viewport_size(),
                camera.logical_viewport_size(),
            )
        })
        .collect();
    info!("touch overlay layout node={size:?} window={window:?} cameras={viewports:?}");
}

fn despawn_overlay(
    mut commands: Commands,
    overlay: Query<Entity, With<TouchOverlay>>,
    world: Query<Entity, Or<(With<WorldControl>, With<WorldCursor>)>>,
) {
    for entity in &overlay {
        commands.entity(entity).despawn_recursive();
    }
    for entity in &world {
        commands.entity(entity).despawn_recursive();
    }
    commands.remove_resource::<WorldOverlay>();
}

fn sync_overlay_visibility(
    menu: Query<&Visibility, With<PopUpMenu>>,
    mut overlay: Query<&mut Visibility, (With<TouchOverlay>, Without<PopUpMenu>)>,
) {
    let menu_open = menu
        .iter()
        .any(|visibility| *visibility != Visibility::Hidden);
    for mut visibility in &mut overlay {
        *visibility = if menu_open {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
}

fn sync_stick_knobs(state: Res<TouchControlState>, mut knobs: Query<(&StickKnob, &mut Style)>) {
    for (knob, mut style) in &mut knobs {
        let stick = match knob.0 {
            ControlKind::Move => state.move_stick,
            ControlKind::Aim => state.aim_stick,
            _ => continue,
        };
        let (left, top, size) = match knob.0 {
            ControlKind::Move => (4., 62., 16.),
            ControlKind::Aim => (80., 62., 16.),
            _ => continue,
        };
        let travel = size * 0.28;
        style.left = Val::Percent(left + size * 0.3 + stick.x * travel);
        style.top = Val::Percent(top + size * 0.3 - stick.y * travel);
    }
}

fn sync_cursor_widget(
    window: Query<&Window, With<PrimaryWindow>>,
    state: Res<TouchControlState>,
    mut cursor: Query<&mut Style, With<CursorWidget>>,
) {
    let Ok(_window) = window.get_single() else {
        return;
    };
    let Some(position) = state.cursor else {
        return;
    };
    let Ok(mut style) = cursor.get_single_mut() else {
        return;
    };
    style.left = Val::Px(position.x - 7.);
    style.top = Val::Px(position.y - 7.);
}

fn apply_virtual_input(
    time: Res<Time>,
    mut touches: EventReader<TouchInput>,
    gamepads: Res<Gamepads>,
    axes: Res<Axis<GamepadAxis>>,
    gamepad_buttons: Res<ButtonInput<GamepadButton>>,
    mut windows: Query<(Entity, &mut Window), With<PrimaryWindow>>,
    menu: Query<&Visibility, With<PopUpMenu>>,
    mut state: ResMut<TouchControlState>,
    mut keys: EventWriter<KeyboardInput>,
    mut mouse_buttons: EventWriter<MouseButtonInput>,
    mut wheel: EventWriter<MouseWheel>,
    mut motion: EventWriter<MouseMotion>,
) {
    if !cfg!(target_os = "android") && gamepads.iter().next().is_none() {
        return;
    }
    let Ok((window_entity, mut window)) = windows.get_single_mut() else {
        return;
    };
    let menu_open = menu
        .iter()
        .any(|visibility| *visibility != Visibility::Hidden);
    if menu_open {
        release_all(&mut state, window_entity, &mut keys, &mut mouse_buttons);
        return;
    }

    if state.cursor.is_none() {
        state.cursor = Some(Vec2::new(window.width() * 0.5, window.height() * 0.5));
    }

    let width = window.width().max(1.);
    let height = window.height().max(1.);
    let controls = control_layout(width, height);

    for touch in touches.read() {
        match touch.phase {
            TouchPhase::Started | TouchPhase::Moved => {
                let role = if touch.phase == TouchPhase::Started {
                    let role = controls
                        .iter()
                        .find(|control| control.contains(touch.position))
                        .map(|control| TouchRole::Control(control.kind))
                        .unwrap_or(TouchRole::Point);
                    state.touches.insert(touch.id, role);
                    info!("touch {} at {:?} -> {:?}", touch.id, touch.position, role);
                    role
                } else {
                    state
                        .touches
                        .get(&touch.id)
                        .copied()
                        .unwrap_or(TouchRole::Point)
                };
                match role {
                    TouchRole::Control(ControlKind::Move) => {
                        state.move_stick =
                            stick_vector(&controls, ControlKind::Move, touch.position);
                    }
                    TouchRole::Control(ControlKind::Aim) => {
                        state.aim_stick = stick_vector(&controls, ControlKind::Aim, touch.position);
                    }
                    TouchRole::Point => state.cursor = Some(touch.position),
                    _ => {}
                }
            }
            TouchPhase::Ended | TouchPhase::Canceled => match state.touches.remove(&touch.id) {
                Some(TouchRole::Control(ControlKind::Move)) => state.move_stick = Vec2::ZERO,
                Some(TouchRole::Control(ControlKind::Aim)) => state.aim_stick = Vec2::ZERO,
                _ => {}
            },
        }
    }

    apply_gamepad(
        &gamepads,
        &axes,
        &gamepad_buttons,
        &mut state,
        width,
        height,
        time.delta_seconds(),
    );

    state.cursor = Some(
        state.cursor.unwrap_or(Vec2::ZERO)
            + Vec2::new(state.aim_stick.x, -state.aim_stick.y)
                * CURSOR_SPEED
                * time.delta_seconds(),
    );
    if let Some(cursor) = state.cursor.as_mut() {
        cursor.x = cursor.x.clamp(0., width - 1.);
        cursor.y = cursor.y.clamp(0., height - 1.);
        window.set_cursor_position(Some(*cursor));
    }

    let pressed_controls = currently_pressed(&state.touches);
    let left = pressed_controls.contains(&ControlKind::LeftClick);
    let right = pressed_controls.contains(&ControlKind::RightClick);
    let zoom_in = pressed_controls.contains(&ControlKind::ZoomIn);
    let zoom_out = pressed_controls.contains(&ControlKind::ZoomOut);
    let rotate_left = pressed_controls.contains(&ControlKind::RotateLeft);
    let rotate_right = pressed_controls.contains(&ControlKind::RotateRight);
    let pause = pressed_controls.contains(&ControlKind::Pause);
    let build_base = pressed_controls.contains(&ControlKind::BuildBase);
    let build_power = pressed_controls.contains(&ControlKind::BuildPower);

    set_button(
        &mut state.left_held,
        left,
        MouseButton::Left,
        window_entity,
        &mut mouse_buttons,
    );
    set_button(
        &mut state.right_held,
        right,
        MouseButton::Right,
        window_entity,
        &mut mouse_buttons,
    );

    if zoom_in {
        wheel.send(MouseWheel {
            unit: MouseScrollUnit::Line,
            x: 0.,
            y: -1.,
            window: window_entity,
        });
    }
    if zoom_out {
        wheel.send(MouseWheel {
            unit: MouseScrollUnit::Line,
            x: 0.,
            y: 1.,
            window: window_entity,
        });
    }

    let rotate = rotate_left || rotate_right;
    set_key(
        &mut state.shift_held,
        rotate,
        KeyCode::ShiftLeft,
        window_entity,
        &mut keys,
    );
    if rotate_left {
        motion.send(MouseMotion {
            delta: Vec2::new(-ROTATE_PIXELS, 0.),
        });
    }
    if rotate_right {
        motion.send(MouseMotion {
            delta: Vec2::new(ROTATE_PIXELS, 0.),
        });
    }

    set_key(
        &mut state.pause_down,
        pause,
        KeyCode::Escape,
        window_entity,
        &mut keys,
    );
    set_key(
        &mut state.base_down,
        build_base,
        KeyCode::KeyB,
        window_entity,
        &mut keys,
    );
    set_key(
        &mut state.power_down,
        build_power,
        KeyCode::KeyP,
        window_entity,
        &mut keys,
    );

    let arrow = arrow_key(state.move_stick);
    if state.arrow != arrow {
        info!("virtual arrow {:?} -> {:?}", state.arrow, arrow);
        if let Some(previous) = state.arrow {
            send_key(previous, ButtonState::Released, window_entity, &mut keys);
        }
        if let Some(next) = arrow {
            send_key(next, ButtonState::Pressed, window_entity, &mut keys);
        }
        state.arrow = arrow;
    }
}

fn apply_gamepad(
    gamepads: &Gamepads,
    axes: &Axis<GamepadAxis>,
    buttons: &ButtonInput<GamepadButton>,
    state: &mut TouchControlState,
    width: f32,
    height: f32,
    dt: f32,
) {
    let Some(gamepad) = gamepads.iter().next() else {
        return;
    };
    let axis = |kind| axes.get(GamepadAxis::new(gamepad, kind)).unwrap_or(0.);
    let move_stick = Vec2::new(
        axis(GamepadAxisType::LeftStickX),
        axis(GamepadAxisType::LeftStickY),
    );
    let finger_move = state
        .touches
        .values()
        .any(|role| matches!(role, TouchRole::Control(ControlKind::Move)));
    if !finger_move {
        state.move_stick = if move_stick.length() > DEADZONE {
            move_stick.clamp_length_max(1.)
        } else {
            Vec2::ZERO
        };
    }
    let aim = Vec2::new(
        axis(GamepadAxisType::RightStickX),
        axis(GamepadAxisType::RightStickY),
    );
    if aim.length() > DEADZONE {
        let aim = aim.clamp_length_max(1.);
        if let Some(cursor) = state.cursor.as_mut() {
            *cursor += Vec2::new(aim.x, -aim.y) * CURSOR_SPEED * dt;
            cursor.x = cursor.x.clamp(0., width - 1.);
            cursor.y = cursor.y.clamp(0., height - 1.);
        }
    }
    let down = |kind| buttons.pressed(GamepadButton::new(gamepad, kind));
    if down(GamepadButtonType::South) {
        state
            .touches
            .insert(u64::MAX - 1, TouchRole::Control(ControlKind::LeftClick));
    } else {
        state.touches.remove(&(u64::MAX - 1));
    }
    if down(GamepadButtonType::East) {
        state
            .touches
            .insert(u64::MAX - 2, TouchRole::Control(ControlKind::RightClick));
    } else {
        state.touches.remove(&(u64::MAX - 2));
    }
    if down(GamepadButtonType::Start) {
        state
            .touches
            .insert(u64::MAX - 3, TouchRole::Control(ControlKind::Pause));
    } else {
        state.touches.remove(&(u64::MAX - 3));
    }
}

struct LayoutControl {
    kind: ControlKind,
    center: Vec2,
    radius: f32,
}

impl LayoutControl {
    fn contains(&self, point: Vec2) -> bool {
        point.distance(self.center) <= self.radius
    }
}

fn control_layout(width: f32, height: f32) -> Vec<LayoutControl> {
    let unit = width.min(height);
    let place = |kind, x, y, radius| LayoutControl {
        kind,
        center: Vec2::new(width * x, height * y),
        radius: unit * radius,
    };
    vec![
        place(ControlKind::Move, 0.12, 0.72, 0.11),
        place(ControlKind::Aim, 0.88, 0.72, 0.11),
        place(ControlKind::LeftClick, 0.73, 0.64, 0.055),
        place(ControlKind::RightClick, 0.73, 0.80, 0.055),
        place(ControlKind::ZoomIn, 0.92, 0.11, 0.045),
        place(ControlKind::ZoomOut, 0.92, 0.23, 0.045),
        place(ControlKind::RotateLeft, 0.81, 0.11, 0.045),
        place(ControlKind::RotateRight, 0.81, 0.23, 0.045),
        place(ControlKind::Pause, 0.08, 0.09, 0.05),
        place(ControlKind::BuildBase, 0.20, 0.09, 0.04),
        place(ControlKind::BuildPower, 0.30, 0.09, 0.04),
    ]
}

fn stick_vector(controls: &[LayoutControl], kind: ControlKind, point: Vec2) -> Vec2 {
    let Some(control) = controls.iter().find(|control| control.kind == kind) else {
        return Vec2::ZERO;
    };
    let delta = point - control.center;
    let stick = Vec2::new(delta.x, -delta.y) / control.radius.max(1.);
    if stick.length() < DEADZONE {
        return Vec2::ZERO;
    }
    stick.clamp_length_max(1.)
}

fn currently_pressed(touches: &HashMap<u64, TouchRole>) -> Vec<ControlKind> {
    touches
        .values()
        .filter_map(|role| match role {
            TouchRole::Control(kind) => Some(*kind),
            TouchRole::Point => None,
        })
        .collect()
}

fn arrow_key(stick: Vec2) -> Option<KeyCode> {
    if stick.length() < DEADZONE {
        return None;
    }
    if stick.x.abs() > stick.y.abs() {
        Some(if stick.x < 0. {
            KeyCode::ArrowLeft
        } else {
            KeyCode::ArrowRight
        })
    } else {
        Some(if stick.y > 0. {
            KeyCode::ArrowUp
        } else {
            KeyCode::ArrowDown
        })
    }
}

fn set_button(
    held: &mut bool,
    down: bool,
    button: MouseButton,
    window: Entity,
    events: &mut EventWriter<MouseButtonInput>,
) {
    if *held == down {
        return;
    }
    *held = down;
    events.send(MouseButtonInput {
        button,
        state: if down {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        },
        window,
    });
}

fn set_key(
    held: &mut bool,
    down: bool,
    key: KeyCode,
    window: Entity,
    events: &mut EventWriter<KeyboardInput>,
) {
    if *held == down {
        return;
    }
    *held = down;
    send_key(
        key,
        if down {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        },
        window,
        events,
    );
}

fn send_key(
    key_code: KeyCode,
    state: ButtonState,
    window: Entity,
    events: &mut EventWriter<KeyboardInput>,
) {
    events.send(KeyboardInput {
        key_code,
        logical_key: Key::Unidentified(NativeKey::Unidentified),
        state,
        window,
    });
}

fn release_all(
    state: &mut TouchControlState,
    window: Entity,
    keys: &mut EventWriter<KeyboardInput>,
    mouse_buttons: &mut EventWriter<MouseButtonInput>,
) {
    state.touches.clear();
    state.move_stick = Vec2::ZERO;
    state.aim_stick = Vec2::ZERO;
    if let Some(arrow) = state.arrow.take() {
        send_key(arrow, ButtonState::Released, window, keys);
    }
    if state.left_held {
        state.left_held = false;
        mouse_buttons.send(MouseButtonInput {
            button: MouseButton::Left,
            state: ButtonState::Released,
            window,
        });
    }
    if state.right_held {
        state.right_held = false;
        mouse_buttons.send(MouseButtonInput {
            button: MouseButton::Right,
            state: ButtonState::Released,
            window,
        });
    }
    if state.shift_held {
        state.shift_held = false;
        send_key(KeyCode::ShiftLeft, ButtonState::Released, window, keys);
    }
    if state.pause_down {
        state.pause_down = false;
        send_key(KeyCode::Escape, ButtonState::Released, window, keys);
    }
    if state.base_down {
        state.base_down = false;
        send_key(KeyCode::KeyB, ButtonState::Released, window, keys);
    }
    if state.power_down {
        state.power_down = false;
        send_key(KeyCode::KeyP, ButtonState::Released, window, keys);
    }
}
