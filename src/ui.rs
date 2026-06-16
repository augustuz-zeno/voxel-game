// =============================================================================
// HUD, Hotbar, Debug Overlay, and Input handling
// =============================================================================

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::window::CursorGrabMode;

use crate::blocks::BlockType;
use crate::render::BlockIcons;
use crate::state::{AppState, InGameEntity};
use crate::player::{Player, PlayerPhysics};
use crate::world::Chunk;

#[derive(Resource)]
pub struct HotbarState {
    pub active_index: usize,
    pub blocks: [BlockType; 9],
}

impl Default for HotbarState {
    fn default() -> Self {
        Self {
            active_index: 0,
            blocks: [
                BlockType::Grass, BlockType::Dirt, BlockType::Stone, BlockType::Sand,
                BlockType::Wood, BlockType::Leaves, BlockType::Water, BlockType::Bedrock,
                BlockType::Air,
            ]
        }
    }
}

#[derive(Component)]
pub struct HotbarSlot {
    pub index: usize,
}

#[derive(Resource, Default)]
pub struct DebugState {
    pub visible: bool,
}

#[derive(Component)]
pub struct DebugOverlay;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<HotbarState>()
            .init_resource::<DebugState>()
            .add_systems(OnEnter(AppState::InGame), setup_hud)
            .add_systems(
                Update,
                (
                    update_hotbar_ui,
                    toggle_debug,
                    update_debug_overlay,
                    grab_cursor,
                    escape_key,
                ).run_if(in_state(AppState::InGame)),
            );
    }
}

fn setup_hud(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    icons: Res<BlockIcons>,
    hotbar: Res<HotbarState>,
) {
    let font = asset_server.load("fonts/VCROSDMonoNova.ttf");
    
    // Debug
    commands.spawn((
        DebugOverlay,
        TextBundle::from_section(
            "",
            TextStyle { font: font.clone(), font_size: 16.0, color: Color::srgb(0.9, 1.0, 0.9) }
        )
        .with_style(Style {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            left: Val::Px(10.0),
            display: Display::None,
            ..default()
        }),
        InGameEntity,
    ));

    // Crosshair
    for (w, h, ml, mt) in [(2.0f32, 18.0f32, -1.0f32, -9.0f32), (18.0, 2.0, -9.0, -1.0)] {
        commands.spawn((
            NodeBundle {
                style: Style {
                    position_type: PositionType::Absolute,
                    width: Val::Px(w),
                    height: Val::Px(h),
                    left: Val::Percent(50.0),
                    top: Val::Percent(50.0),
                    margin: UiRect { left: Val::Px(ml), top: Val::Px(mt), ..default() },
                    ..default()
                },
                background_color: BackgroundColor(Color::WHITE),
                ..default()
            },
            InGameEntity,
        ));
    }

    // Hotbar Container
    commands.spawn((
        NodeBundle {
            style: Style {
                position_type: PositionType::Absolute,
                bottom: Val::Px(20.0),
                left: Val::Percent(50.0),
                margin: UiRect::left(Val::Px(-202.0)), // Center (404 wide)
                width: Val::Px(404.0),
                height: Val::Px(44.0),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                padding: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            background_color: BackgroundColor(Color::srgb(0.1, 0.1, 0.1)),
            ..default()
        },
        InGameEntity,
    )).with_children(|p| {
        for i in 0..9 {
            let block = hotbar.blocks[i];
            let image = if block == BlockType::Air {
                asset_server.load("textures/block/dirt.png") // Fallback
            } else if let Some(h) = icons.icons.get(&block) {
                h.clone()
            } else {
                asset_server.load("textures/block/dirt.png")
            };
            
            p.spawn((
                ImageBundle {
                    style: Style { width: Val::Px(40.0), height: Val::Px(40.0), border: UiRect::all(Val::Px(2.0)), ..default() },
                    image: UiImage::new(image),
                    ..default()
                },
                BorderColor(if i == hotbar.active_index { Color::WHITE } else { Color::srgb(0.3, 0.3, 0.3) }),
                HotbarSlot { index: i }
            ));
        }
    });
}

fn update_hotbar_ui(
    keys: Res<ButtonInput<KeyCode>>,
    mut hotbar: ResMut<HotbarState>,
    mut slots_q: Query<(&HotbarSlot, &mut BorderColor)>,
) {
    let mut new_index = hotbar.active_index;
    if keys.just_pressed(KeyCode::Digit1) { new_index = 0; }
    if keys.just_pressed(KeyCode::Digit2) { new_index = 1; }
    if keys.just_pressed(KeyCode::Digit3) { new_index = 2; }
    if keys.just_pressed(KeyCode::Digit4) { new_index = 3; }
    if keys.just_pressed(KeyCode::Digit5) { new_index = 4; }
    if keys.just_pressed(KeyCode::Digit6) { new_index = 5; }
    if keys.just_pressed(KeyCode::Digit7) { new_index = 6; }
    if keys.just_pressed(KeyCode::Digit8) { new_index = 7; }
    if keys.just_pressed(KeyCode::Digit9) { new_index = 8; }

    if new_index != hotbar.active_index {
        hotbar.active_index = new_index;
        for (slot, mut border) in slots_q.iter_mut() {
            border.0 = if slot.index == hotbar.active_index { Color::WHITE } else { Color::srgb(0.3, 0.3, 0.3) };
        }
    }
}

fn toggle_debug(
    keys: Res<ButtonInput<KeyCode>>,
    mut debug: ResMut<DebugState>,
    mut query: Query<&mut Style, With<DebugOverlay>>
) {
    if keys.just_pressed(KeyCode::F3) {
        debug.visible = !debug.visible;
        for mut style in &mut query {
            style.display = if debug.visible { Display::Flex } else { Display::None };
        }
    }
}

fn update_debug_overlay(
    debug: Res<DebugState>,
    diagnostics: Res<DiagnosticsStore>,
    player_q: Query<(&Transform, &PlayerPhysics), With<Player>>,
    chunks_q: Query<&Chunk>,
    mut text_q: Query<&mut Text, With<DebugOverlay>>,
) {
    if !debug.visible { return; }
    let Ok((tf, physics)) = player_q.get_single() else { return; };
    let Ok(mut text) = text_q.get_single_mut() else { return; };

    let fps = diagnostics.get(&FrameTimeDiagnosticsPlugin::FPS).and_then(|d| d.smoothed()).unwrap_or(0.0);
    let ft = diagnostics.get(&FrameTimeDiagnosticsPlugin::FRAME_TIME).and_then(|d| d.smoothed()).unwrap_or(0.0);
    let pos = tf.translation;
    let mode = if physics.fly_mode { "Полёт" } else { "Ходьба" };
    let sneak = if physics.sneaking { " [Присяд]" } else { "" };

    text.sections[0].value = format!(
        "ФПС: {:.0}\nКадр: {:.1} мс\nXYZ: {:.1} / {:.1} / {:.1}\nЧанков: {}\nРежим: {}{}",
        fps, ft * 1000.0, pos.x, pos.y, pos.z, chunks_q.iter().count(), mode, sneak
    );
}

fn grab_cursor(
    mut windows_q: Query<&mut Window>,
    mouse: Res<ButtonInput<MouseButton>>
) {
    let Ok(mut window) = windows_q.get_single_mut() else { return; };
    if mouse.just_pressed(MouseButton::Left) && window.cursor.grab_mode == CursorGrabMode::None {
        window.cursor.grab_mode = CursorGrabMode::Locked;
        window.cursor.visible = false;
    }
}

fn escape_key(
    keys: Res<ButtonInput<KeyCode>>,
    mut windows_q: Query<&mut Window>,
    mut next_state: ResMut<NextState<AppState>>
) {
    if !keys.just_pressed(KeyCode::Escape) { return; }
    let Ok(mut window) = windows_q.get_single_mut() else { return; };
    if window.cursor.grab_mode == CursorGrabMode::Locked {
        window.cursor.grab_mode = CursorGrabMode::None;
        window.cursor.visible = true;
    } else {
        next_state.set(AppState::Menu);
    }
}
