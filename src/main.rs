// =============================================================================
// Voxel Game — Bevy 0.14  (Minecraft clone)
// [dependencies]
// bevy = "0.14"
// =============================================================================

use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    input::mouse::MouseMotion,
    math::Vec3A,
    prelude::*,
    render::{
        mesh::{Indices, PrimitiveTopology},
        primitives::{Frustum, Sphere},
        render_asset::RenderAssetUsages,
    },
    window::{CursorGrabMode, PresentMode, WindowPlugin},
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

const GRAVITY: f32 = 24.0;
const JUMP_VELOCITY: f32 = 8.5;
const PLAYER_WIDTH: f32 = 0.6;
const PLAYER_HEIGHT: f32 = 1.8;
const FLY_TOGGLE_TIME: f32 = 0.3;

const SEED: u64 = 42;
const TERRAIN_SCALE: f64 = 0.07;
const TERRAIN_HEIGHT: f64 = 12.0;
const WATER_LEVEL: usize = 4;

/// Type alias for face data — fixes clippy::type_complexity
type FaceData = ([i32; 3], [[f32; 3]; 4], [f32; 3]);

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

    /// Returns true if the block is solid for collision purposes.
    pub fn is_solid(self) -> bool {
        !matches!(self, BlockType::Air | BlockType::Water)
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

    const FACES: [FaceData; 6] = [
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
struct PlayerPhysics {
    velocity: Vec3,
    on_ground: bool,
    fly_mode: bool,
    last_space_time: f32,
}

impl Default for PlayerPhysics {
    fn default() -> Self {
        Self {
            velocity: Vec3::ZERO,
            on_ground: false,
            fly_mode: false,
            last_space_time: 0.0,
        }
    }
}

#[derive(Component)]
struct PlayerCamera;

// =============================================================================
// DEBUG OVERLAY
// =============================================================================

#[derive(Resource, Default)]
struct DebugState {
    visible: bool,
}

#[derive(Component)]
struct DebugOverlay;

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
            PlayerPhysics::default(),
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
                    transform: Transform::from_xyz(0.0, 1.6, 0.0),
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
// COLLISION HELPERS
// =============================================================================

/// Look up a block at world coordinates by finding the corresponding chunk.
fn get_world_block(wx: i32, wy: i32, wz: i32, chunks_q: &Query<&Chunk>) -> BlockType {
    if wy < 0 {
        return BlockType::Bedrock; // below world = solid
    }
    let cx = wx.div_euclid(CHUNK_SIZE as i32);
    let cy = wy.div_euclid(CHUNK_SIZE as i32);
    let cz = wz.div_euclid(CHUNK_SIZE as i32);
    let lx = wx.rem_euclid(CHUNK_SIZE as i32) as usize;
    let ly = wy.rem_euclid(CHUNK_SIZE as i32) as usize;
    let lz = wz.rem_euclid(CHUNK_SIZE as i32) as usize;

    let grid = IVec3::new(cx, cy, cz);
    for chunk in chunks_q.iter() {
        if chunk.grid_pos == grid {
            return chunk.data.get(lx, ly, lz);
        }
    }
    BlockType::Air
}

/// Check if a player AABB at `pos` (feet position) collides with any solid block.
fn check_aabb_collision(
    pos: Vec3,
    half_w: f32,
    height: f32,
    chunks_q: &Query<&Chunk>,
) -> bool {
    let min_bx = (pos.x - half_w).floor() as i32;
    let max_bx = (pos.x + half_w - 0.001).floor() as i32;
    let min_by = pos.y.floor() as i32;
    let max_by = (pos.y + height - 0.001).floor() as i32;
    let min_bz = (pos.z - half_w).floor() as i32;
    let max_bz = (pos.z + half_w - 0.001).floor() as i32;

    for by in min_by..=max_by {
        for bz in min_bz..=max_bz {
            for bx in min_bx..=max_bx {
                let block = get_world_block(bx, by, bz, chunks_q);
                if block.is_solid() {
                    // Precise AABB overlap test
                    let bmin = Vec3::new(bx as f32, by as f32, bz as f32);
                    let bmax = bmin + Vec3::ONE;
                    let pmin = Vec3::new(pos.x - half_w, pos.y, pos.z - half_w);
                    let pmax = Vec3::new(pos.x + half_w, pos.y + height, pos.z + half_w);

                    if pmin.x < bmax.x
                        && pmax.x > bmin.x
                        && pmin.y < bmax.y
                        && pmax.y > bmin.y
                        && pmin.z < bmax.z
                        && pmax.z > bmin.z
                    {
                        return true;
                    }
                }
            }
        }
    }
    false
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
    mut player_q: Query<(&mut Transform, &Player, &mut PlayerPhysics)>,
    chunks_q: Query<&Chunk>,
) {
    let Ok((mut tf, _, mut physics)) = player_q.get_single_mut() else {
        return;
    };

    let dt = time.delta_seconds();
    if dt <= 0.0 {
        return;
    }
    let sprint = keys.pressed(KeyCode::ControlLeft);
    let speed = MOVE_SPEED * if sprint { SPRINT_MULTIPLIER } else { 1.0 };

    // ── Fly mode toggle (double-tap Space) ──
    if keys.just_pressed(KeyCode::Space) {
        let now = time.elapsed_seconds();
        if physics.last_space_time > 0.0 && (now - physics.last_space_time) < FLY_TOGGLE_TIME {
            physics.fly_mode = !physics.fly_mode;
            physics.velocity = Vec3::ZERO;
            physics.on_ground = false;
            physics.last_space_time = 0.0;
        } else {
            physics.last_space_time = now;
        }
    }

    // ── Directional input ──
    let fwd = tf.forward();
    let right = tf.right();
    let fwd_flat = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or_zero();
    let right_flat = Vec3::new(right.x, 0.0, right.z).normalize_or_zero();

    let mut wish_dir = Vec3::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        wish_dir += fwd_flat;
    }
    if keys.pressed(KeyCode::KeyS) {
        wish_dir -= fwd_flat;
    }
    if keys.pressed(KeyCode::KeyA) {
        wish_dir -= right_flat;
    }
    if keys.pressed(KeyCode::KeyD) {
        wish_dir += right_flat;
    }
    wish_dir = wish_dir.normalize_or_zero();

    if physics.fly_mode {
        // ── Creative flight ──
        let mut vel = wish_dir * speed;
        if keys.pressed(KeyCode::Space) {
            vel.y += speed;
        }
        if keys.pressed(KeyCode::ShiftLeft) {
            vel.y -= speed;
        }
        tf.translation += vel * dt;
    } else {
        // ── Survival movement with collision ──
        physics.velocity.x = wish_dir.x * speed;
        physics.velocity.z = wish_dir.z * speed;

        // Gravity
        physics.velocity.y -= GRAVITY * dt;
        physics.velocity.y = physics.velocity.y.max(-50.0); // terminal velocity

        // Jump
        if keys.pressed(KeyCode::Space) && physics.on_ground {
            physics.velocity.y = JUMP_VELOCITY;
            physics.on_ground = false;
        }

        // ── Collision resolution (per-axis) ──
        let half_w = PLAYER_WIDTH * 0.5;
        let h = PLAYER_HEIGHT;
        let mut pos = tf.translation;

        // X axis
        let old_x = pos.x;
        pos.x += physics.velocity.x * dt;
        if check_aabb_collision(pos, half_w, h, &chunks_q) {
            pos.x = old_x;
            physics.velocity.x = 0.0;
        }

        // Z axis
        let old_z = pos.z;
        pos.z += physics.velocity.z * dt;
        if check_aabb_collision(pos, half_w, h, &chunks_q) {
            pos.z = old_z;
            physics.velocity.z = 0.0;
        }

        // Y axis
        let old_y = pos.y;
        pos.y += physics.velocity.y * dt;
        if check_aabb_collision(pos, half_w, h, &chunks_q) {
            if physics.velocity.y <= 0.0 {
                physics.on_ground = true;
            }
            pos.y = old_y;
            physics.velocity.y = 0.0;
        } else {
            physics.on_ground = false;
        }

        tf.translation = pos;
    }
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
// HUD (crosshair + debug overlay)
// =============================================================================

fn setup_hud(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/VCROSDMonoNova.ttf");

    // ── Debug overlay (hidden by default, toggle with F3) ──
    commands.spawn((
        DebugOverlay,
        TextBundle::from_section(
            "",
            TextStyle {
                font: font.clone(),
                font_size: 16.0,
                color: Color::srgb(0.9, 1.0, 0.9),
            },
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

    // ── Crosshair ──
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
// DEBUG SYSTEMS
// =============================================================================

fn toggle_debug(
    keys: Res<ButtonInput<KeyCode>>,
    mut debug: ResMut<DebugState>,
    mut query: Query<&mut Style, With<DebugOverlay>>,
) {
    if keys.just_pressed(KeyCode::F3) {
        debug.visible = !debug.visible;
        for mut style in &mut query {
            style.display = if debug.visible {
                Display::Flex
            } else {
                Display::None
            };
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
    if !debug.visible {
        return;
    }

    let Ok((tf, physics)) = player_q.get_single() else {
        return;
    };
    let Ok(mut text) = text_q.get_single_mut() else {
        return;
    };

    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);

    let frame_time = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);

    let chunk_count = chunks_q.iter().count();
    let pos = tf.translation;
    let mode = if physics.fly_mode {
        "Полёт"
    } else {
        "Ходьба"
    };

    text.sections[0].value = format!(
        "ФПС: {:.0}\nКадр: {:.1} мс\nXYZ: {:.1} / {:.1} / {:.1}\nЧанков: {}\nРежим: {} (2×Space)",
        fps,
        frame_time * 1000.0,
        pos.x,
        pos.y,
        pos.z,
        chunk_count,
        mode,
    );
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

fn setup_menu(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/VCROSDMonoNova.ttf");
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
                    "Воксельная Игра",
                    TextStyle {
                        font: font.clone(),
                        font_size: 80.0,
                        color: Color::srgb(0.95, 0.95, 0.5),
                    },
                )
                .with_style(Style {
                    margin: UiRect::bottom(Val::Px(50.0)),
                    ..default()
                }),
            );

            spawn_btn(p, "Создать Мир", MenuButton::CreateWorld, font.clone());
            spawn_btn(p, "Выход", MenuButton::Quit, font);
        });
}

fn spawn_btn(parent: &mut ChildBuilder, label: &str, tag: MenuButton, font: Handle<Font>) {
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
                    font,
                    font_size: 30.0,
                    color: Color::WHITE,
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
                title: "Воксельная Игра".into(),
                resolution: (1280.0, 720.0).into(),
                present_mode: PresentMode::AutoVsync,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(FrameTimeDiagnosticsPlugin)
        .init_state::<AppState>()
        .init_resource::<WorldManager>()
        .init_resource::<DebugState>()
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
                player_move.after(stream_chunks),
                grab_cursor,
                escape_key,
                toggle_debug,
                update_debug_overlay,
            )
                .run_if(in_state(AppState::InGame)),
        )
        .run();
}
