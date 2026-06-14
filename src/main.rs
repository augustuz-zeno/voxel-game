// =============================================================================
// Voxel Game — Bevy 0.14  (Minecraft clone)
// [dependencies]
// bevy = "0.14"
// =============================================================================

use bevy::{
    input::mouse::MouseMotion,
    math::Vec3A,
    prelude::*,
    render::{
        mesh::{Indices, PrimitiveTopology},
        primitives::{Frustum, Sphere},
        render_asset::RenderAssetUsages,
    },
    window::{CursorGrabMode, WindowPlugin},
};

// =============================================================================
// CONSTANTS
// =============================================================================

const CHUNK_SIZE: usize = 16;
const CHUNK_VOL: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;
const RENDER_DISTANCE: i32 = 4;

const MOUSE_SENSITIVITY: f32 = 0.002;
const MOVE_SPEED: f32 = 8.0;
const SPRINT_MULTIPLIER: f32 = 3.0;

const SEED: u64 = 42;
const TERRAIN_SCALE: f64 = 0.07;
const TERRAIN_HEIGHT: f64 = 12.0;
const WATER_LEVEL: usize = 4;

// =============================================================================
// APP STATE
// =============================================================================

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
enum AppState {
    #[default]
    Menu,
    InGame,
}

// =============================================================================
// BLOCK TYPES
// =============================================================================

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum BlockType {
    Air = 0,
    Grass = 1,
    Dirt = 2,
    Stone = 3,
    Sand = 4,
    Water = 5,
    Wood = 6,
    Leaves = 7,
    Bedrock = 8,
}

impl BlockType {
    pub fn is_transparent(self) -> bool {
        matches!(self, BlockType::Air | BlockType::Water | BlockType::Leaves)
    }

    pub fn base_color(self) -> [f32; 4] {
        match self {
            BlockType::Air => [0.0, 0.0, 0.0, 0.0],
            BlockType::Grass => [0.27, 0.58, 0.18, 1.0],
            BlockType::Dirt => [0.42, 0.28, 0.14, 1.0],
            BlockType::Stone => [0.47, 0.47, 0.47, 1.0],
            BlockType::Sand => [0.82, 0.76, 0.52, 1.0],
            BlockType::Water => [0.15, 0.35, 0.78, 0.7],
            BlockType::Wood => [0.38, 0.26, 0.12, 1.0],
            BlockType::Leaves => [0.18, 0.48, 0.12, 0.85],
            BlockType::Bedrock => [0.15, 0.15, 0.15, 1.0],
        }
    }

    pub fn face_color(self, normal: [f32; 3]) -> [f32; 4] {
        let b = self.base_color();
        let brightness = if normal[1] > 0.5 {
            1.15
        } else if normal[1] < -0.5 {
            0.60
        } else if normal[2].abs() > 0.5 {
            0.85
        } else {
            0.75
        };
        [
            (b[0] * brightness).min(1.0),
            (b[1] * brightness).min(1.0),
            (b[2] * brightness).min(1.0),
            b[3],
        ]
    }
}

// =============================================================================
// SIMPLE NOISE (no external crate)
// =============================================================================

fn hash(x: i64, z: i64, seed: u64) -> f64 {
    let mut h = (x as u64).wrapping_mul(374761393) ^ (z as u64).wrapping_mul(668265263) ^ seed;
    h ^= h >> 13;
    h = h.wrapping_mul(1274126177);
    h ^= h >> 16;
    (h as f64) / (u64::MAX as f64)
}

fn smooth_noise(fx: f64, fz: f64, seed: u64) -> f64 {
    let ix = fx.floor() as i64;
    let iz = fz.floor() as i64;
    let tx = fx - ix as f64;
    let tz = fz - iz as f64;
    let ux = tx * tx * (3.0 - 2.0 * tx);
    let uz = tz * tz * (3.0 - 2.0 * tz);
    let a = hash(ix, iz, seed);
    let b = hash(ix + 1, iz, seed);
    let c = hash(ix, iz + 1, seed);
    let d = hash(ix + 1, iz + 1, seed);
    a + (b - a) * ux + (c - a) * uz + (a - b - c + d) * ux * uz
}

fn terrain_height(wx: i32, wz: i32) -> usize {
    let x = wx as f64 * TERRAIN_SCALE;
    let z = wz as f64 * TERRAIN_SCALE;
    let n = smooth_noise(x, z, SEED) * 1.0
        + smooth_noise(x * 2.0, z * 2.0, SEED + 1) * 0.5
        + smooth_noise(x * 4.0, z * 4.0, SEED + 2) * 0.25;
    ((n / 1.75) * TERRAIN_HEIGHT + 6.0).clamp(1.0, CHUNK_SIZE as f64 - 2.0) as usize
}

// =============================================================================
// CHUNK DATA
// =============================================================================

pub struct ChunkData {
    pub blocks: Box<[BlockType; CHUNK_VOL]>,
}

impl ChunkData {
    pub fn new_empty() -> Self {
        Self {
            blocks: Box::new([BlockType::Air; CHUNK_VOL]),
        }
    }
    #[inline]
    pub fn idx(x: usize, y: usize, z: usize) -> usize {
        x + CHUNK_SIZE * (y + CHUNK_SIZE * z)
    }
    #[inline]
    pub fn get(&self, x: usize, y: usize, z: usize) -> BlockType {
        self.blocks[Self::idx(x, y, z)]
    }
    #[inline]
    pub fn set(&mut self, x: usize, y: usize, z: usize, b: BlockType) {
        self.blocks[Self::idx(x, y, z)] = b;
    }
    pub fn get_safe(&self, x: i32, y: i32, z: i32) -> Option<BlockType> {
        if x < 0
            || y < 0
            || z < 0
            || x >= CHUNK_SIZE as i32
            || y >= CHUNK_SIZE as i32
            || z >= CHUNK_SIZE as i32
        {
            None
        } else {
            Some(self.get(x as usize, y as usize, z as usize))
        }
    }
}

// =============================================================================
// TERRAIN GENERATION
// =============================================================================

pub fn generate_chunk(grid_pos: IVec3) -> ChunkData {
    let mut chunk = ChunkData::new_empty();
    let bx = grid_pos.x * CHUNK_SIZE as i32;
    let by = grid_pos.y * CHUNK_SIZE as i32;
    let bz = grid_pos.z * CHUNK_SIZE as i32;

    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            let surface = terrain_height(bx + x as i32, bz + z as i32);
            for y in 0..CHUNK_SIZE {
                let wy = by + y as i32;
                if wy < 0 {
                    continue;
                }
                let wy = wy as usize;
                let block = if wy == 0 {
                    BlockType::Bedrock
                } else if wy > surface {
                    if wy <= WATER_LEVEL {
                        BlockType::Water
                    } else {
                        BlockType::Air
                    }
                } else if wy == surface {
                    if surface <= WATER_LEVEL + 1 {
                        BlockType::Sand
                    } else {
                        BlockType::Grass
                    }
                } else if wy >= surface.saturating_sub(3) {
                    if surface <= WATER_LEVEL + 1 {
                        BlockType::Sand
                    } else {
                        BlockType::Dirt
                    }
                } else {
                    BlockType::Stone
                };
                chunk.set(x, y, z, block);
            }
        }
    }
    chunk
}

// =============================================================================
// CHUNK COMPONENT
// =============================================================================

#[derive(Component)]
pub struct Chunk {
    pub grid_pos: IVec3,
    pub data: ChunkData,
    pub dirty: bool,
}

impl Chunk {
    pub fn new(grid_pos: IVec3, data: ChunkData) -> Self {
        Self {
            grid_pos,
            data,
            dirty: true,
        }
    }
}

#[derive(Component)]
pub struct ChunkVisible;

// =============================================================================
// MESH BUILDER
// =============================================================================

pub fn build_chunk_mesh(chunk: &ChunkData) -> Mesh {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    const FACES: [([i32; 3], [[f32; 3]; 4], [f32; 3]); 6] = [
        (
            [1, 0, 0],
            [[1., 0., 0.], [1., 1., 0.], [1., 1., 1.], [1., 0., 1.]],
            [1., 0., 0.],
        ),
        (
            [-1, 0, 0],
            [[0., 0., 1.], [0., 1., 1.], [0., 1., 0.], [0., 0., 0.]],
            [-1., 0., 0.],
        ),
        (
            [0, 1, 0],
            [[0., 1., 0.], [0., 1., 1.], [1., 1., 1.], [1., 1., 0.]],
            [0., 1., 0.],
        ),
        (
            [0, -1, 0],
            [[0., 0., 1.], [0., 0., 0.], [1., 0., 0.], [1., 0., 1.]],
            [0., -1., 0.],
        ),
        (
            [0, 0, 1],
            [[1., 0., 1.], [1., 1., 1.], [0., 1., 1.], [0., 0., 1.]],
            [0., 0., 1.],
        ),
        (
            [0, 0, -1],
            [[0., 0., 0.], [0., 1., 0.], [1., 1., 0.], [1., 0., 0.]],
            [0., 0., -1.],
        ),
    ];

    for z in 0..CHUNK_SIZE {
        for y in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let block = chunk.get(x, y, z);
                if block == BlockType::Air {
                    continue;
                }
                for (dir, corners, normal) in &FACES {
                    let nb = chunk
                        .get_safe(x as i32 + dir[0], y as i32 + dir[1], z as i32 + dir[2])
                        .unwrap_or(BlockType::Air);
                    if !nb.is_transparent() {
                        continue;
                    }
                    if block == BlockType::Water && nb == BlockType::Water {
                        continue;
                    }
                    let color = block.face_color(*normal);
                    let base = positions.len() as u32;
                    for c in corners {
                        positions.push([x as f32 + c[0], y as f32 + c[1], z as f32 + c[2]]);
                        normals.push(*normal);
                        colors.push(color);
                    }
                    uvs.extend_from_slice(&[[0., 1.], [0., 0.], [1., 0.], [1., 1.]]);
                    indices.extend_from_slice(&[
                        base,
                        base + 1,
                        base + 2,
                        base,
                        base + 2,
                        base + 3,
                    ]);
                }
            }
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

// =============================================================================
// MATERIAL
// =============================================================================

#[derive(Resource)]
pub struct ChunkMaterial(pub Handle<StandardMaterial>);

fn setup_materials(mut commands: Commands, mut materials: ResMut<Assets<StandardMaterial>>) {
    let mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 1.0,
        metallic: 0.0,
        reflectance: 0.05,
        ..default()
    });
    commands.insert_resource(ChunkMaterial(mat));
}

// =============================================================================
// PLAYER
// =============================================================================

#[derive(Component)]
struct Player {
    pitch: f32,
    yaw: f32,
}

#[derive(Component)]
struct PlayerCamera;

// =============================================================================
// WORLD MANAGER
// =============================================================================

#[derive(Resource, Default)]
struct WorldManager {
    loaded_chunks: std::collections::HashSet<IVec3>,
}

// =============================================================================
// IN-GAME SETUP / TEARDOWN
// =============================================================================

#[derive(Component)]
struct InGameEntity;

fn setup_world(mut commands: Commands, mut world_mgr: ResMut<WorldManager>) {
    world_mgr.loaded_chunks.clear();

    commands.spawn((
        DirectionalLightBundle {
            directional_light: DirectionalLight {
                illuminance: 15_000.0,
                shadows_enabled: false,
                ..default()
            },
            transform: Transform::from_xyz(50.0, 100.0, 50.0).looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        },
        InGameEntity,
    ));

    commands.insert_resource(AmbientLight {
        color: Color::srgb(0.7, 0.75, 1.0),
        brightness: 200.0,
    });
    commands.insert_resource(ClearColor(Color::srgb(0.45, 0.65, 0.95)));

    let spawn_y = terrain_height(0, 0) as f32 + 2.5;
    commands
        .spawn((
            Player {
                pitch: 0.0,
                yaw: 0.0,
            },
            InGameEntity,
            TransformBundle::from(Transform::from_xyz(0.0, spawn_y, 0.0)),
            VisibilityBundle::default(),
        ))
        .with_children(|p| {
            p.spawn((
                Camera3dBundle {
                    camera: Camera {
                        order: 0,
                        ..default()
                    },
                    transform: Transform::from_xyz(0.0, 0.7, 0.0),
                    ..default()
                },
                PlayerCamera,
            ));
        });
}

fn teardown_world(
    mut commands: Commands,
    entities_q: Query<Entity, With<InGameEntity>>,
    chunk_q: Query<Entity, With<Chunk>>,
    mut world_mgr: ResMut<WorldManager>,
    mut ambient: ResMut<AmbientLight>,
    mut clear_color: ResMut<ClearColor>,
) {
    for e in &entities_q {
        commands.entity(e).despawn_recursive();
    }
    for e in &chunk_q {
        commands.entity(e).despawn_recursive();
    }
    world_mgr.loaded_chunks.clear();
    ambient.brightness = 80.0;
    clear_color.0 = Color::srgb(0.08, 0.08, 0.12);
}

// =============================================================================
// CHUNK STREAMING
// =============================================================================

fn stream_chunks(
    mut commands: Commands,
    player_q: Query<&Transform, With<Player>>,
    mut world_mgr: ResMut<WorldManager>,
) {
    let Ok(ptf) = player_q.get_single() else {
        return;
    };
    let px = (ptf.translation.x / CHUNK_SIZE as f32).floor() as i32;
    let pz = (ptf.translation.z / CHUNK_SIZE as f32).floor() as i32;
    for dz in -RENDER_DISTANCE..=RENDER_DISTANCE {
        for dx in -RENDER_DISTANCE..=RENDER_DISTANCE {
            let grid = IVec3::new(px + dx, 0, pz + dz);
            if !world_mgr.loaded_chunks.contains(&grid) {
                let data = generate_chunk(grid);
                commands.spawn((Chunk::new(grid, data), ChunkVisible));
                world_mgr.loaded_chunks.insert(grid);
            }
        }
    }
}

// =============================================================================
// MESH REBUILD
// =============================================================================

fn rebuild_dirty_chunks(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mat: Res<ChunkMaterial>,
    mut chunks_q: Query<(Entity, &mut Chunk)>,
) {
    for (entity, mut chunk) in &mut chunks_q {
        if !chunk.dirty {
            continue;
        }
        let mesh = meshes.add(build_chunk_mesh(&chunk.data));
        commands.entity(entity).insert(PbrBundle {
            mesh,
            material: mat.0.clone(),
            transform: Transform::from_translation(chunk.grid_pos.as_vec3() * CHUNK_SIZE as f32),
            ..default()
        });
        chunk.dirty = false;
    }
}

// =============================================================================
// PLAYER CONTROLS
// =============================================================================

fn player_look(
    mut mouse_events: EventReader<MouseMotion>,
    mut player_q: Query<(&mut Transform, &mut Player)>,
    mut camera_q: Query<&mut Transform, (With<PlayerCamera>, Without<Player>)>,
    windows_q: Query<&Window>,
) {
    let Ok(window) = windows_q.get_single() else {
        return;
    };
    if window.cursor.grab_mode == CursorGrabMode::None {
        return;
    }
    let mut delta = Vec2::ZERO;
    for ev in mouse_events.read() {
        delta += ev.delta;
    }
    if delta == Vec2::ZERO {
        return;
    }
    let Ok((mut ptf, mut player)) = player_q.get_single_mut() else {
        return;
    };
    let Ok(mut ctf) = camera_q.get_single_mut() else {
        return;
    };
    player.yaw -= delta.x * MOUSE_SENSITIVITY;
    player.pitch -= delta.y * MOUSE_SENSITIVITY;
    player.pitch = player.pitch.clamp(-1.55, 1.55);
    ptf.rotation = Quat::from_rotation_y(player.yaw);
    ctf.rotation = Quat::from_rotation_x(player.pitch);
}

fn player_move(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut player_q: Query<(&mut Transform, &Player)>,
) {
    let Ok((mut tf, _)) = player_q.get_single_mut() else {
        return;
    };
    let sprint = keys.pressed(KeyCode::ControlLeft);
    let speed = MOVE_SPEED * if sprint { SPRINT_MULTIPLIER } else { 1.0 };
    let dt = time.delta_seconds();

    let fwd = tf.forward();
    let right = tf.right();
    let fwd = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or_zero();
    let right = Vec3::new(right.x, 0.0, right.z).normalize_or_zero();

    let mut vel = Vec3::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        vel += fwd;
    }
    if keys.pressed(KeyCode::KeyS) {
        vel -= fwd;
    }
    if keys.pressed(KeyCode::KeyA) {
        vel -= right;
    }
    if keys.pressed(KeyCode::KeyD) {
        vel += right;
    }
    if keys.pressed(KeyCode::Space) {
        vel += Vec3::Y;
    }
    if keys.pressed(KeyCode::ShiftLeft) {
        vel -= Vec3::Y;
    }
    tf.translation += vel.normalize_or_zero() * speed * dt;
}

fn grab_cursor(mut windows_q: Query<&mut Window>, mouse: Res<ButtonInput<MouseButton>>) {
    let Ok(mut window) = windows_q.get_single_mut() else {
        return;
    };
    if mouse.just_pressed(MouseButton::Left) && window.cursor.grab_mode == CursorGrabMode::None {
        window.cursor.grab_mode = CursorGrabMode::Locked;
        window.cursor.visible = false;
    }
}

fn escape_key(
    keys: Res<ButtonInput<KeyCode>>,
    mut windows_q: Query<&mut Window>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    let Ok(mut window) = windows_q.get_single_mut() else {
        return;
    };
    if window.cursor.grab_mode == CursorGrabMode::Locked {
        window.cursor.grab_mode = CursorGrabMode::None;
        window.cursor.visible = true;
    } else {
        next_state.set(AppState::Menu);
    }
}

// =============================================================================
// FRUSTUM CULLING
// =============================================================================

pub fn frustum_culling_system(
    mut commands: Commands,
    camera_q: Query<&Frustum, With<Camera3d>>,
    chunks_q: Query<(Entity, &Transform), With<Chunk>>,
) {
    let Ok(frustum) = camera_q.get_single() else {
        return;
    };
    let half = CHUNK_SIZE as f32 * 0.5;
    let radius = (half * half * 3.0_f32).sqrt();
    for (entity, transform) in &chunks_q {
        let center = transform.translation + Vec3::splat(half);
        if frustum.intersects_sphere(
            &Sphere {
                center: Vec3A::from(center),
                radius,
            },
            true,
        ) {
            commands.entity(entity).insert(ChunkVisible);
        } else {
            commands.entity(entity).remove::<ChunkVisible>();
        }
    }
}

// =============================================================================
// HUD
// =============================================================================

#[derive(Component)]
struct HudText;

fn setup_hud(mut commands: Commands) {
    commands.spawn((
        HudText,
        TextBundle::from_sections([
            TextSection::new("Voxel Game\n", TextStyle { font_size: 22.0, color: Color::WHITE, ..default() }),
            TextSection::new(
                "WASD - move  |  Space - up  |  Shift - down  |  Ctrl - sprint\nClick to lock cursor  |  Esc - unlock  |  Esc again - menu",
                TextStyle { font_size: 15.0, color: Color::srgb(0.75, 0.75, 0.75), ..default() },
            ),
        ])
        .with_style(Style {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            left: Val::Px(10.0),
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
                    margin: UiRect {
                        left: Val::Px(ml),
                        top: Val::Px(mt),
                        ..default()
                    },
                    ..default()
                },
                background_color: BackgroundColor(Color::WHITE),
                ..default()
            },
            InGameEntity,
        ));
    }
}

// =============================================================================
// MENU
// =============================================================================

#[derive(Component)]
struct MenuRoot;
#[derive(Component)]
struct MenuCamera;
#[derive(Component)]
enum MenuButton {
    CreateWorld,
    Quit,
}

fn setup_menu(mut commands: Commands) {
    commands.insert_resource(ClearColor(Color::srgb(0.08, 0.08, 0.12)));
    commands.spawn((
        Camera2dBundle {
            camera: Camera {
                order: 1,
                ..default()
            },
            ..default()
        },
        MenuCamera,
    ));
    commands
        .spawn((
            MenuRoot,
            NodeBundle {
                style: Style {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    row_gap: Val::Px(20.0),
                    ..default()
                },
                ..default()
            },
        ))
        .with_children(|p| {
            p.spawn(
                TextBundle::from_section(
                    "Voxel Game",
                    TextStyle {
                        font_size: 80.0,
                        color: Color::srgb(0.95, 0.95, 0.5),
                        ..default()
                    },
                )
                .with_style(Style {
                    margin: UiRect::bottom(Val::Px(50.0)),
                    ..default()
                }),
            );

            spawn_btn(p, "Create World", MenuButton::CreateWorld);
            spawn_btn(p, "Quit", MenuButton::Quit);

            p.spawn(
                TextBundle::from_section(
                    "Click to lock cursor  |  Esc - unlock  |  Esc again - back to menu",
                    TextStyle {
                        font_size: 17.0,
                        color: Color::srgb(0.5, 0.5, 0.5),
                        ..default()
                    },
                )
                .with_style(Style {
                    margin: UiRect::top(Val::Px(40.0)),
                    ..default()
                }),
            );
        });
}

fn spawn_btn(parent: &mut ChildBuilder, label: &str, tag: MenuButton) {
    parent
        .spawn((
            tag,
            ButtonBundle {
                style: Style {
                    width: Val::Px(300.0),
                    height: Val::Px(64.0),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                background_color: BackgroundColor(Color::srgb(0.15, 0.18, 0.30)),
                border_color: BorderColor(Color::srgb(0.4, 0.5, 0.8)),
                ..default()
            },
        ))
        .with_children(|b| {
            b.spawn(TextBundle::from_section(
                label,
                TextStyle {
                    font_size: 30.0,
                    color: Color::WHITE,
                    ..default()
                },
            ));
        });
}

fn teardown_menu(
    mut commands: Commands,
    menu_q: Query<Entity, With<MenuRoot>>,
    cam_q: Query<Entity, With<MenuCamera>>,
) {
    for e in &menu_q {
        commands.entity(e).despawn_recursive();
    }
    for e in &cam_q {
        commands.entity(e).despawn_recursive();
    }
}

fn menu_button_system(
    mut q: Query<(&Interaction, &MenuButton, &mut BackgroundColor), Changed<Interaction>>,
    mut next_state: ResMut<NextState<AppState>>,
    mut app_exit: EventWriter<AppExit>,
) {
    for (interaction, button, mut bg) in &mut q {
        match interaction {
            Interaction::Hovered => bg.0 = Color::srgb(0.28, 0.35, 0.55),
            Interaction::Pressed => {
                bg.0 = Color::srgb(0.10, 0.45, 0.25);
                match button {
                    MenuButton::CreateWorld => next_state.set(AppState::InGame),
                    MenuButton::Quit => {
                        app_exit.send(AppExit::Success);
                    }
                }
            }
            Interaction::None => bg.0 = Color::srgb(0.15, 0.18, 0.30),
        }
    }
}

// =============================================================================
// MAIN
// =============================================================================

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Voxel Game".into(),
                resolution: (1280.0, 720.0).into(),
                ..default()
            }),
            ..default()
        }))
        .init_state::<AppState>()
        .init_resource::<WorldManager>()
        .add_systems(Startup, setup_materials)
        // MENU
        .add_systems(OnEnter(AppState::Menu), setup_menu)
        .add_systems(OnExit(AppState::Menu), teardown_menu)
        .add_systems(Update, menu_button_system.run_if(in_state(AppState::Menu)))
        // GAME
        .add_systems(OnEnter(AppState::InGame), (setup_world, setup_hud).chain())
        .add_systems(OnExit(AppState::InGame), teardown_world)
        .add_systems(
            Update,
            (
                stream_chunks,
                frustum_culling_system,
                rebuild_dirty_chunks.after(frustum_culling_system),
                player_look,
                player_move,
                grab_cursor,
                escape_key,
            )
                .run_if(in_state(AppState::InGame)),
        )
        .run();
}
