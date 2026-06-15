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
        view::RenderLayers,
    },
    utils::HashMap,
    window::{CursorGrabMode, PresentMode, WindowPlugin},
};

// =============================================================================
// CONSTANTS
// =============================================================================

const CHUNK_SIZE: usize = 16;
const CHUNK_VOL: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;
const RENDER_DISTANCE: i32 = 4;

const MOUSE_SENSITIVITY: f32 = 0.003;
const MOVE_SPEED: f32 = 4.5;         // Blocks per second (like Minecraft: 4.317)
const SPRINT_MULTIPLIER: f32 = 1.3;  // Sprint is ~30% faster (like Minecraft: 5.612)
const SNEAK_MULTIPLIER: f32 = 0.3;   // Sneak is 30% of walk speed

// Ground physics tuning (Minecraft-like)
const GROUND_ACCEL: f32 = 50.0;      // How fast we reach max speed on ground
const AIR_ACCEL: f32 = 3.0;          // Very little air control
const GROUND_FRICTION: f32 = 0.88;   // Per-tick velocity multiplier on ground
const AIR_FRICTION: f32 = 0.9965;    // Almost no drag in air (momentum)
const FLY_SPEED: f32 = 10.0;         // Fly mode target speed

const FOV_DEG: f32 = 90.0;           // Field of view in degrees
const VIEW_MODEL_LAYER: usize = 1;   // Render layer for hand viewmodel

const GRAVITY: f32 = 25.0;
const JUMP_VELOCITY: f32 = 8.5;
const PLAYER_WIDTH: f32 = 0.6;
const PLAYER_HEIGHT_STAND: f32 = 1.8;
const PLAYER_HEIGHT_SNEAK: f32 = 1.5;
const CAMERA_HEIGHT_STAND: f32 = 1.6;
const CAMERA_HEIGHT_SNEAK: f32 = 1.3;
const FLY_TOGGLE_TIME: f32 = 0.3;

const SEED: u64 = 42;
const TERRAIN_SCALE: f64 = 0.07;
const TERRAIN_HEIGHT: f64 = 12.0;
const WATER_LEVEL: usize = 7;

type FaceData = ([i32; 3], [[f32; 3]; 4], [[i32; 3]; 4]);

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
// BLOCK TYPES & TEXTURES
// =============================================================================

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum BlockTexture {
    Dirt,
    GrassTop,
    GrassSide,
    Stone,
    Sand,
    Water,
    WoodTop,
    WoodSide,
    Leaves,
    Bedrock,
}

impl BlockTexture {
    fn from_block(block: BlockType, normal: [i32; 3]) -> Self {
        match block {
            BlockType::Air => BlockTexture::Dirt,
            BlockType::Grass => {
                if normal == [0, 1, 0] {
                    BlockTexture::GrassTop
                } else if normal == [0, -1, 0] {
                    BlockTexture::Dirt
                } else {
                    BlockTexture::GrassSide
                }
            }
            BlockType::Dirt => BlockTexture::Dirt,
            BlockType::Stone => BlockTexture::Stone,
            BlockType::Sand => BlockTexture::Sand,
            BlockType::Water => BlockTexture::Water,
            BlockType::Wood => {
                if normal[1] != 0 {
                    BlockTexture::WoodTop
                } else {
                    BlockTexture::WoodSide
                }
            }
            BlockType::Leaves => BlockTexture::Leaves,
            BlockType::Bedrock => BlockTexture::Bedrock,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
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
    
    pub fn is_solid(self) -> bool {
        !matches!(self, BlockType::Air | BlockType::Water)
    }

    pub fn tint(self, normal: [i32; 3]) -> [f32; 4] {
        match self {
            BlockType::Grass if normal == [0, 1, 0] => [0.45, 0.73, 0.33, 1.0],
            BlockType::Leaves => [0.3, 0.6, 0.2, 1.0],
            BlockType::Water => [0.2, 0.4, 0.8, 0.8],
            _ => [1.0, 1.0, 1.0, 1.0],
        }
    }
}

// =============================================================================
// SIMPLE NOISE
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
    ((n / 1.75) * TERRAIN_HEIGHT + 3.0).clamp(1.0, CHUNK_SIZE as f64 - 2.0) as usize
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
    pub fn get_safe(&self, x: i32, y: i32, z: i32) -> BlockType {
        if x < 0 || y < 0 || z < 0 || x >= CHUNK_SIZE as i32 || y >= CHUNK_SIZE as i32 || z >= CHUNK_SIZE as i32 {
            BlockType::Air
        } else {
            self.get(x as usize, y as usize, z as usize)
        }
    }
}

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
                if wy < 0 { continue; }
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
// CHUNK COMPONENTS
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

#[derive(Component)]
pub struct ChunkMeshNode;

// =============================================================================
// MESH BUILDER
// =============================================================================

fn vertex_ao(side1: bool, side2: bool, corner: bool) -> f32 {
    let mut solid_count = 0;
    if side1 { solid_count += 1; }
    if side2 { solid_count += 1; }
    if corner && (side1 || side2) { solid_count += 1; }
    
    match solid_count {
        0 => 1.0,
        1 => 0.8,
        2 => 0.6,
        _ => 0.4,
    }
}

type MeshBuilderData = (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<[f32; 4]>, Vec<u32>);

pub fn build_chunk_meshes(chunk: &ChunkData) -> HashMap<BlockTexture, Mesh> {
    let mut builders: HashMap<BlockTexture, MeshBuilderData> = HashMap::new();

    const FACES: [FaceData; 6] = [
        ([1, 0, 0], [[1., 0., 0.], [1., 1., 0.], [1., 1., 1.], [1., 0., 1.]], [[1, -1, -1], [1, 1, -1], [1, 1, 1], [1, -1, 1]]),
        ([-1, 0, 0], [[0., 0., 1.], [0., 1., 1.], [0., 1., 0.], [0., 0., 0.]], [[-1, -1, 1], [-1, 1, 1], [-1, 1, -1], [-1, -1, -1]]),
        ([0, 1, 0], [[0., 1., 0.], [0., 1., 1.], [1., 1., 1.], [1., 1., 0.]], [[-1, 1, -1], [-1, 1, 1], [1, 1, 1], [1, 1, -1]]),
        ([0, -1, 0], [[0., 0., 1.], [0., 0., 0.], [1., 0., 0.], [1., 0., 1.]], [[-1, -1, 1], [-1, -1, -1], [1, -1, -1], [1, -1, 1]]),
        ([0, 0, 1], [[1., 0., 1.], [1., 1., 1.], [0., 1., 1.], [0., 0., 1.]], [[1, -1, 1], [1, 1, 1], [-1, 1, 1], [-1, -1, 1]]),
        ([0, 0, -1], [[0., 0., 0.], [0., 1., 0.], [1., 1., 0.], [1., 0., 0.]], [[-1, -1, -1], [-1, 1, -1], [1, 1, -1], [1, -1, -1]]),
    ];

    for z in 0..CHUNK_SIZE {
        for y in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let block = chunk.get(x, y, z);
                if block == BlockType::Air { continue; }

                for (dir, corners, ao_offsets) in &FACES {
                    let nx = x as i32 + dir[0];
                    let ny = y as i32 + dir[1];
                    let nz = z as i32 + dir[2];
                    
                    let nb = chunk.get_safe(nx, ny, nz);
                    if !nb.is_transparent() { continue; }
                    if block == BlockType::Water && nb == BlockType::Water { continue; }

                    let tex = BlockTexture::from_block(block, *dir);
                    let builder = builders.entry(tex).or_insert_with(|| (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new()));
                    
                    let tint = block.tint(*dir);
                    
                    let mut ao_vals = [1.0; 4];
                    for i in 0..4 {
                        let s1 = chunk.get_safe(x as i32 + ao_offsets[i][0], y as i32 + ao_offsets[i][1], z as i32 + dir[2]).is_solid();
                        let s2 = chunk.get_safe(x as i32 + dir[0], y as i32 + ao_offsets[i][1], z as i32 + ao_offsets[i][2]).is_solid();
                        let corner = chunk.get_safe(x as i32 + ao_offsets[i][0], y as i32 + ao_offsets[i][1], z as i32 + ao_offsets[i][2]).is_solid();
                        ao_vals[i] = vertex_ao(s1, s2, corner);
                    }

                    let y_offset = if block == BlockType::Water && *dir == [0, 1, 0] { -0.15 } else { 0.0 };
                    let base_idx = builder.0.len() as u32;

                    for (i, c) in corners.iter().enumerate() {
                        builder.0.push([x as f32 + c[0], y as f32 + c[1] + (if c[1] > 0.5 { y_offset } else { 0.0 }), z as f32 + c[2]]);
                        builder.1.push([dir[0] as f32, dir[1] as f32, dir[2] as f32]);
                        
                        let mut final_color = tint;
                        let ao = ao_vals[i];
                        let light = if dir[1] > 0 { 1.0 } else if dir[1] < 0 { 0.6 } else if dir[2] != 0 { 0.8 } else { 0.7 };
                        
                        final_color[0] *= ao * light;
                        final_color[1] *= ao * light;
                        final_color[2] *= ao * light;
                        
                        builder.3.push(final_color);
                    }
                    builder.2.extend_from_slice(&[[0., 1.], [0., 0.], [1., 0.], [1., 1.]]);
                    
                    if ao_vals[0] + ao_vals[2] < ao_vals[1] + ao_vals[3] {
                        builder.4.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2, base_idx, base_idx + 2, base_idx + 3]);
                    } else {
                        builder.4.extend_from_slice(&[base_idx + 1, base_idx + 2, base_idx + 3, base_idx + 1, base_idx + 3, base_idx]);
                    }
                }
            }
        }
    }

    let mut result = HashMap::new();
    for (tex, (pos, norm, uv, color, idx)) in builders {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, norm);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, color);
        mesh.insert_indices(Indices::U32(idx));
        result.insert(tex, mesh);
    }
    result
}

// =============================================================================
// MATERIALS & ICONS
// =============================================================================

#[derive(Resource)]
pub struct BlockMaterials {
    materials: HashMap<BlockTexture, Handle<StandardMaterial>>,
}

#[derive(Resource)]
pub struct BlockIcons {
    icons: HashMap<BlockType, Handle<Image>>,
}

fn setup_materials(mut commands: Commands, asset_server: Res<AssetServer>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let mut mat_map = HashMap::new();
    let mut icon_map = HashMap::new();
    
    let tex_paths = [
        (BlockTexture::Dirt, "textures/block/dirt.png"),
        (BlockTexture::GrassTop, "textures/block/grass_block_top.png"),
        (BlockTexture::GrassSide, "textures/block/grass_block_side.png"),
        (BlockTexture::Stone, "textures/block/stone.png"),
        (BlockTexture::Sand, "textures/block/sand.png"),
        (BlockTexture::Water, "textures/block/water_still.png"),
        (BlockTexture::WoodTop, "textures/block/oak_log_top.png"),
        (BlockTexture::WoodSide, "textures/block/oak_log.png"),
        (BlockTexture::Leaves, "textures/block/oak_leaves.png"),
        (BlockTexture::Bedrock, "textures/block/bedrock.png"),
    ];

    for (tex, path) in tex_paths {
        let handle = asset_server.load(path);
        
        let alpha_mode = if tex == BlockTexture::Water || tex == BlockTexture::Leaves {
            AlphaMode::Blend
        } else {
            AlphaMode::Opaque
        };

        let mat = materials.add(StandardMaterial {
            base_color_texture: Some(handle.clone()),
            perceptual_roughness: 0.9,
            reflectance: 0.05,
            alpha_mode,
            cull_mode: if tex == BlockTexture::Leaves || tex == BlockTexture::Water { None } else { Some(bevy::render::render_resource::Face::Back) },
            ..default()
        });
        mat_map.insert(tex, mat);

        // Map icons for UI
        match tex {
            BlockTexture::Dirt => { icon_map.insert(BlockType::Dirt, handle.clone()); }
            BlockTexture::GrassSide => { icon_map.insert(BlockType::Grass, handle.clone()); }
            BlockTexture::Stone => { icon_map.insert(BlockType::Stone, handle.clone()); }
            BlockTexture::Sand => { icon_map.insert(BlockType::Sand, handle.clone()); }
            BlockTexture::Water => { icon_map.insert(BlockType::Water, handle.clone()); }
            BlockTexture::WoodSide => { icon_map.insert(BlockType::Wood, handle.clone()); }
            BlockTexture::Leaves => { icon_map.insert(BlockType::Leaves, handle.clone()); }
            BlockTexture::Bedrock => { icon_map.insert(BlockType::Bedrock, handle.clone()); }
            _ => {}
        }
    }

    commands.insert_resource(BlockMaterials { materials: mat_map });
    commands.insert_resource(BlockIcons { icons: icon_map });
}

// =============================================================================
// PLAYER & HOTBAR
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
    sneaking: bool,
    last_space_time: f32,
}

impl Default for PlayerPhysics {
    fn default() -> Self {
        Self { velocity: Vec3::ZERO, on_ground: false, fly_mode: false, sneaking: false, last_space_time: 0.0 }
    }
}

#[derive(Component)]
struct PlayerCamera;

#[derive(Component)]
struct ViewModelCamera;

#[derive(Component)]
struct PlayerHand;

#[derive(Resource)]
struct HotbarState {
    active_index: usize,
    blocks: [BlockType; 9],
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
struct HotbarSlot {
    index: usize,
}

// =============================================================================
// DEBUG OVERLAY & WORLD MGR
// =============================================================================

#[derive(Resource, Default)]
struct DebugState { visible: bool }
#[derive(Component)]
struct DebugOverlay;

#[derive(Resource, Default)]
struct WorldManager {
    loaded_chunks: std::collections::HashSet<IVec3>,
}

// =============================================================================
// IN-GAME SETUP / TEARDOWN
// =============================================================================

#[derive(Component)]
struct InGameEntity;

fn setup_world(
    mut commands: Commands,
    mut world_mgr: ResMut<WorldManager>,
    mut meshes: ResMut<Assets<Mesh>>,
    block_materials: Res<BlockMaterials>,
) {
    world_mgr.loaded_chunks.clear();

    commands.spawn((
        DirectionalLightBundle {
            directional_light: DirectionalLight { illuminance: 12_000.0, shadows_enabled: false, ..default() },
            transform: Transform::from_xyz(50.0, 100.0, 50.0).looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        },
        InGameEntity,
    ));

    commands.insert_resource(AmbientLight { color: Color::WHITE, brightness: 400.0 });
    commands.insert_resource(ClearColor(Color::srgb(0.45, 0.65, 0.95)));

    let spawn_y = terrain_height(0, 0) as f32 + 2.5;

    // Hand mesh — a block-shaped cuboid held in view
    let hand_mesh = meshes.add(Cuboid::new(0.35, 0.65, 0.35));
    let hand_mat = block_materials.materials
        .get(&BlockTexture::Stone)
        .cloned()
        .unwrap_or_else(|| block_materials.materials.values().next().cloned().unwrap());

    commands
        .spawn((
            Player { pitch: 0.0, yaw: 0.0 },
            PlayerPhysics::default(),
            InGameEntity,
            TransformBundle::from(Transform::from_xyz(0.0, spawn_y, 0.0)),
            VisibilityBundle::default(),
        ))
        .with_children(|p| {
            p.spawn((
                Camera3dBundle {
                    camera: Camera { order: 0, ..default() },
                    projection: Projection::Perspective(PerspectiveProjection {
                        fov: FOV_DEG.to_radians(),
                        ..default()
                    }),
                    transform: Transform::from_xyz(0.0, CAMERA_HEIGHT_STAND, 0.0),
                    ..default()
                },
                PlayerCamera,
                RenderLayers::layer(0),
            ))
            .with_children(|cam| {
                // View model camera: renders on top of world without z-fighting
                cam.spawn((
                    Camera3dBundle {
                        camera: Camera {
                            order: 1,
                            clear_color: ClearColorConfig::None,
                            ..default()
                        },
                        projection: Projection::Perspective(PerspectiveProjection {
                            fov: 70.0_f32.to_radians(),
                            ..default()
                        }),
                        ..default()
                    },
                    ViewModelCamera,
                    RenderLayers::layer(VIEW_MODEL_LAYER),
                ));
                // Hand block in bottom-right
                cam.spawn((
                    PbrBundle {
                        mesh: hand_mesh,
                        material: hand_mat,
                        transform: Transform::from_xyz(0.52, -0.58, -0.85)
                            .with_rotation(Quat::from_euler(
                                EulerRot::XYZ, 0.15, -0.4, 0.1,
                            )),
                        ..default()
                    },
                    PlayerHand,
                    RenderLayers::layer(VIEW_MODEL_LAYER),
                ));
            });
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
    for e in &entities_q { commands.entity(e).despawn_recursive(); }
    for e in &chunk_q { commands.entity(e).despawn_recursive(); }
    world_mgr.loaded_chunks.clear();
    ambient.brightness = 80.0;
    clear_color.0 = Color::srgb(0.08, 0.08, 0.12);
}

// =============================================================================
// CHUNK LOGIC & RAYCASTING
// =============================================================================

fn stream_chunks(
    mut commands: Commands,
    player_q: Query<&Transform, With<Player>>,
    mut world_mgr: ResMut<WorldManager>,
    chunks_q: Query<(Entity, &Chunk)>,
) {
    let Ok(ptf) = player_q.get_single() else { return; };
    let px = (ptf.translation.x / CHUNK_SIZE as f32).floor() as i32;
    let pz = (ptf.translation.z / CHUNK_SIZE as f32).floor() as i32;

    // Unload chunks that are too far away
    for (entity, chunk) in &chunks_q {
        let dx = (chunk.grid_pos.x - px).abs();
        let dz = (chunk.grid_pos.z - pz).abs();
        if dx > RENDER_DISTANCE + 1 || dz > RENDER_DISTANCE + 1 {
            commands.entity(entity).despawn_recursive();
            world_mgr.loaded_chunks.remove(&chunk.grid_pos);
        }
    }

    // Load new chunks in range
    for dz in -RENDER_DISTANCE..=RENDER_DISTANCE {
        for dx in -RENDER_DISTANCE..=RENDER_DISTANCE {
            let grid = IVec3::new(px + dx, 0, pz + dz);
            if !world_mgr.loaded_chunks.contains(&grid) {
                let data = generate_chunk(grid);
                commands.spawn((
                    Chunk::new(grid, data),
                    ChunkVisible,
                    TransformBundle::from_transform(Transform::from_translation(grid.as_vec3() * CHUNK_SIZE as f32)),
                    VisibilityBundle::default(),
                ));
                world_mgr.loaded_chunks.insert(grid);
            }
        }
    }
}

pub fn frustum_culling_system(
    mut commands: Commands,
    camera_q: Query<&Frustum, With<Camera3d>>,
    chunks_q: Query<(Entity, &Transform), With<Chunk>>,
) {
    let Ok(frustum) = camera_q.get_single() else { return; };
    let half = CHUNK_SIZE as f32 * 0.5;
    let radius = (half * half * 3.0_f32).sqrt();
    for (entity, transform) in &chunks_q {
        let center = transform.translation + Vec3::splat(half);
        if frustum.intersects_sphere(&Sphere { center: Vec3A::from(center), radius }, true) {
            commands.entity(entity).insert(ChunkVisible);
        } else {
            commands.entity(entity).remove::<ChunkVisible>();
        }
    }
}

fn rebuild_dirty_chunks(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    block_materials: Res<BlockMaterials>,
    mut chunks_q: Query<(Entity, &mut Chunk)>,
    children_q: Query<&Children>,
    mesh_nodes_q: Query<Entity, With<ChunkMeshNode>>,
) {
    let mut rebuilt = 0;
    for (entity, mut chunk) in &mut chunks_q {
        if !chunk.dirty { continue; }
        if rebuilt >= 4 { break; } // Throttle: max 4 chunk rebuilds per frame
        
        if let Ok(children) = children_q.get(entity) {
            for child in children {
                if mesh_nodes_q.contains(*child) {
                    commands.entity(*child).despawn_recursive();
                }
            }
        }

        let chunk_meshes = build_chunk_meshes(&chunk.data);
        for (tex, mesh) in chunk_meshes {
            if let Some(mat) = block_materials.materials.get(&tex) {
                let mesh_handle = meshes.add(mesh);
                let child = commands.spawn((
                    PbrBundle { mesh: mesh_handle, material: mat.clone(), ..default() },
                    ChunkMeshNode,
                )).id();
                commands.entity(entity).add_child(child);
            }
        }
        chunk.dirty = false;
        rebuilt += 1;
    }
}

fn get_world_block(wx: i32, wy: i32, wz: i32, chunks_q: &Query<&mut Chunk>) -> BlockType {
    if wy < 0 { return BlockType::Bedrock; }
    let cx = wx.div_euclid(CHUNK_SIZE as i32);
    let cy = wy.div_euclid(CHUNK_SIZE as i32);
    let cz = wz.div_euclid(CHUNK_SIZE as i32);
    let lx = wx.rem_euclid(CHUNK_SIZE as i32) as usize;
    let ly = wy.rem_euclid(CHUNK_SIZE as i32) as usize;
    let lz = wz.rem_euclid(CHUNK_SIZE as i32) as usize;

    let grid = IVec3::new(cx, cy, cz);
    for chunk in chunks_q.iter() {
        if chunk.grid_pos == grid { return chunk.data.get(lx, ly, lz); }
    }
    BlockType::Air
}

fn set_world_block(wx: i32, wy: i32, wz: i32, block: BlockType, chunks_q: &mut Query<&mut Chunk>) {
    if wy < 0 { return; }
    let cx = wx.div_euclid(CHUNK_SIZE as i32);
    let cy = wy.div_euclid(CHUNK_SIZE as i32);
    let cz = wz.div_euclid(CHUNK_SIZE as i32);
    let lx = wx.rem_euclid(CHUNK_SIZE as i32) as usize;
    let ly = wy.rem_euclid(CHUNK_SIZE as i32) as usize;
    let lz = wz.rem_euclid(CHUNK_SIZE as i32) as usize;

    let target_grid = IVec3::new(cx, cy, cz);
    let mut neighbors_to_update = Vec::new();
    if lx == 0 { neighbors_to_update.push(target_grid + IVec3::new(-1, 0, 0)); }
    if lx == CHUNK_SIZE - 1 { neighbors_to_update.push(target_grid + IVec3::new(1, 0, 0)); }
    if ly == 0 { neighbors_to_update.push(target_grid + IVec3::new(0, -1, 0)); }
    if ly == CHUNK_SIZE - 1 { neighbors_to_update.push(target_grid + IVec3::new(0, 1, 0)); }
    if lz == 0 { neighbors_to_update.push(target_grid + IVec3::new(0, 0, -1)); }
    if lz == CHUNK_SIZE - 1 { neighbors_to_update.push(target_grid + IVec3::new(0, 0, 1)); }

    for mut chunk in chunks_q.iter_mut() {
        if chunk.grid_pos == target_grid {
            chunk.data.set(lx, ly, lz, block);
            chunk.dirty = true;
        } else if neighbors_to_update.contains(&chunk.grid_pos) {
            chunk.dirty = true;
        }
    }
}

fn check_aabb_collision(pos: Vec3, half_w: f32, height: f32, chunks_q: &Query<&mut Chunk>) -> bool {
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
                    let bmin = Vec3::new(bx as f32, by as f32, bz as f32);
                    let bmax = bmin + Vec3::ONE;
                    let pmin = Vec3::new(pos.x - half_w, pos.y, pos.z - half_w);
                    let pmax = Vec3::new(pos.x + half_w, pos.y + height, pos.z + half_w);
                    if pmin.x < bmax.x && pmax.x > bmin.x && pmin.y < bmax.y && pmax.y > bmin.y && pmin.z < bmax.z && pmax.z > bmin.z {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn raycast_blocks(start: Vec3, dir: Vec3, max_dist: f32, chunks_q: &Query<&mut Chunk>) -> Option<(IVec3, IVec3)> {
    let mut t = 0.0;
    let mut current_pos = start;
    let step = 0.05;
    let mut last_voxel = IVec3::new(start.x.floor() as i32, start.y.floor() as i32, start.z.floor() as i32);

    while t < max_dist {
        t += step;
        current_pos += dir * step;
        let voxel = IVec3::new(current_pos.x.floor() as i32, current_pos.y.floor() as i32, current_pos.z.floor() as i32);
        
        if voxel != last_voxel {
            let block = get_world_block(voxel.x, voxel.y, voxel.z, chunks_q);
            if block.is_solid() {
                return Some((voxel, last_voxel));
            }
            last_voxel = voxel;
        }
    }
    None
}

fn block_interaction(
    mouse: Res<ButtonInput<MouseButton>>,
    player_q: Query<&GlobalTransform, With<PlayerCamera>>,
    mut chunks_q: Query<&mut Chunk>,
    hotbar: Res<HotbarState>,
) {
    if !mouse.just_pressed(MouseButton::Left) && !mouse.just_pressed(MouseButton::Right) { return; }
    let Ok(cam_tf) = player_q.get_single() else { return; };
    
    let start = cam_tf.translation();
    let dir = cam_tf.forward().normalize();
    
    if let Some((hit, prev)) = raycast_blocks(start, dir, 5.0, &chunks_q) {
        if mouse.just_pressed(MouseButton::Left) {
            set_world_block(hit.x, hit.y, hit.z, BlockType::Air, &mut chunks_q);
        } else if mouse.just_pressed(MouseButton::Right) {
            let block = hotbar.blocks[hotbar.active_index];
            if block != BlockType::Air {
                set_world_block(prev.x, prev.y, prev.z, block, &mut chunks_q);
            }
        }
    }
}

// =============================================================================
// PLAYER CONTROLS
// =============================================================================

fn player_look(
    mut mouse_events: EventReader<MouseMotion>,
    mut player_q: Query<(&mut Transform, &mut Player)>,
    mut camera_q: Query<(&mut Transform, &PlayerCamera), Without<Player>>,
    windows_q: Query<&Window>,
) {
    let Ok(window) = windows_q.get_single() else { return; };
    if window.cursor.grab_mode == CursorGrabMode::None { return; }
    let mut delta = Vec2::ZERO;
    for ev in mouse_events.read() { delta += ev.delta; }
    if delta == Vec2::ZERO { return; }
    
    let Ok((mut ptf, mut player)) = player_q.get_single_mut() else { return; };
    let Ok((mut ctf, _)) = camera_q.get_single_mut() else { return; };
    
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
    mut camera_q: Query<(&mut Transform, &PlayerCamera), Without<Player>>,
    chunks_q: Query<&mut Chunk>,
) {
    let Ok((mut tf, _, mut physics)) = player_q.get_single_mut() else { return; };
    let dt = time.delta_seconds();
    if dt <= 0.0 { return; }

    // --- Input State ---
    physics.sneaking = keys.pressed(KeyCode::ShiftLeft);
    let sprint = keys.pressed(KeyCode::ControlLeft) && !physics.sneaking;

    // --- Camera height (smooth lerp) ---
    if let Ok((mut ctf, _)) = camera_q.get_single_mut() {
        let target_cam_y = if physics.sneaking && !physics.fly_mode { CAMERA_HEIGHT_SNEAK } else { CAMERA_HEIGHT_STAND };
        ctf.translation.y += (target_cam_y - ctf.translation.y) * 12.0 * dt;
        ctf.translation.x = 0.0;
        ctf.translation.z = 0.0;
    }

    // --- Fly mode toggle (double-tap Space) ---
    if keys.just_pressed(KeyCode::Space) {
        let now = time.elapsed_seconds();
        if physics.last_space_time > 0.0 && (now - physics.last_space_time) < FLY_TOGGLE_TIME {
            physics.fly_mode = !physics.fly_mode;
            if physics.fly_mode {
                // Entered fly: reset velocity for clean start
                physics.velocity = Vec3::ZERO;
            } else {
                // Exited fly: keep XZ momentum so player can steer while falling
                physics.velocity.y = 0.0;
            }
            physics.last_space_time = 0.0;
        } else {
            physics.last_space_time = now;
        }
    }

    // --- Wish direction (flat, based on player rotation) ---
    let fwd = tf.forward();
    let right = tf.right();
    let fwd_flat = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or_zero();
    let right_flat = Vec3::new(right.x, 0.0, right.z).normalize_or_zero();

    let mut wish_dir = Vec3::ZERO;
    if keys.pressed(KeyCode::KeyW) { wish_dir += fwd_flat; }
    if keys.pressed(KeyCode::KeyS) { wish_dir -= fwd_flat; }
    if keys.pressed(KeyCode::KeyA) { wish_dir -= right_flat; }
    if keys.pressed(KeyCode::KeyD) { wish_dir += right_flat; }
    wish_dir = wish_dir.normalize_or_zero();

    let half_w = PLAYER_WIDTH * 0.5;
    let h = if physics.sneaking && !physics.fly_mode { PLAYER_HEIGHT_SNEAK } else { PLAYER_HEIGHT_STAND };
    let speed_mult = if physics.sneaking && !physics.fly_mode { SNEAK_MULTIPLIER } else if sprint { SPRINT_MULTIPLIER } else { 1.0 };
    let target_speed = MOVE_SPEED * speed_mult;

    if physics.fly_mode {
        // ----------------------------------------------------------------
        // FLY PHYSICS: Instant-feel directional flight
        // ----------------------------------------------------------------
        let fly_speed = FLY_SPEED * if sprint { SPRINT_MULTIPLIER } else { 1.0 };

        // Y axis: Space=up, Shift=down (independent of horizontal movement)
        let mut y_input = 0.0;
        if keys.pressed(KeyCode::Space) { y_input += 1.0; }
        if keys.pressed(KeyCode::ShiftLeft) { y_input -= 1.0; }

        // Accelerate towards wish direction
        let target_xz = wish_dir * fly_speed;
        physics.velocity.x += (target_xz.x - physics.velocity.x) * (1.0 - 0.1_f32.powf(dt * 15.0));
        physics.velocity.z += (target_xz.z - physics.velocity.z) * (1.0 - 0.1_f32.powf(dt * 15.0));
        let target_y = y_input * fly_speed;
        physics.velocity.y += (target_y - physics.velocity.y) * (1.0 - 0.1_f32.powf(dt * 20.0));

        let mut pos = tf.translation;
        pos.x += physics.velocity.x * dt;
        if check_aabb_collision(pos, half_w, h, &chunks_q) {
            pos.x -= physics.velocity.x * dt;
            physics.velocity.x = 0.0;
        }
        pos.z += physics.velocity.z * dt;
        if check_aabb_collision(pos, half_w, h, &chunks_q) {
            pos.z -= physics.velocity.z * dt;
            physics.velocity.z = 0.0;
        }
        pos.y += physics.velocity.y * dt;
        if check_aabb_collision(pos, half_w, h, &chunks_q) {
            // Auto-land when flying down into ground
            if physics.velocity.y < 0.0 {
                physics.fly_mode = false;
                physics.on_ground = true;
            }
            pos.y -= physics.velocity.y * dt;
            physics.velocity.y = 0.0;
        }
        tf.translation = pos;

    } else {
        // ----------------------------------------------------------------
        // SURVIVAL PHYSICS: Minecraft-accurate ground/air acceleration
        // ----------------------------------------------------------------

        // Minecraft model: accelerate towards wish_dir * target_speed using
        // different acceleration rates on ground vs air.  Then apply friction.
        let accel = if physics.on_ground { GROUND_ACCEL } else { AIR_ACCEL };
        let friction = if physics.on_ground { GROUND_FRICTION } else { AIR_FRICTION };

        // How much extra velocity we can add towards wish direction
        let current_speed_in_dir = Vec3::new(physics.velocity.x, 0.0, physics.velocity.z).dot(wish_dir);
        let add_speed = (target_speed - current_speed_in_dir).max(0.0);
        let accel_amount = (accel * target_speed * dt).min(add_speed);

        physics.velocity.x += wish_dir.x * accel_amount;
        physics.velocity.z += wish_dir.z * accel_amount;

        // Apply friction (exponential decay)
        physics.velocity.x *= friction;
        physics.velocity.z *= friction;

        // Gravity
        physics.velocity.y -= GRAVITY * dt;
        physics.velocity.y = physics.velocity.y.max(-50.0);

        // Jump
        if keys.pressed(KeyCode::Space) && physics.on_ground {
            physics.velocity.y = JUMP_VELOCITY;
            physics.on_ground = false;
        }

        let mut pos = tf.translation;
        let mut next_x = pos.x + physics.velocity.x * dt;
        let mut next_z = pos.z + physics.velocity.z * dt;

        // Edge safety while sneaking
        if physics.sneaking && physics.on_ground {
            let check_pos_x = Vec3::new(next_x, pos.y - 0.05, pos.z);
            if !check_aabb_collision(check_pos_x, half_w, h, &chunks_q) {
                physics.velocity.x = 0.0;
                next_x = pos.x;
            }
            let check_pos_z = Vec3::new(pos.x, pos.y - 0.05, next_z);
            if !check_aabb_collision(check_pos_z, half_w, h, &chunks_q) {
                physics.velocity.z = 0.0;
                next_z = pos.z;
            }
        }

        // X collision
        pos.x = next_x;
        if check_aabb_collision(pos, half_w, h, &chunks_q) {
            pos.x = tf.translation.x;
            physics.velocity.x = 0.0;
        }

        // Z collision
        pos.z = next_z;
        if check_aabb_collision(pos, half_w, h, &chunks_q) {
            pos.z = tf.translation.z;
            physics.velocity.z = 0.0;
        }

        // Y collision
        pos.y += physics.velocity.y * dt;
        if check_aabb_collision(pos, half_w, h, &chunks_q) {
            if physics.velocity.y <= 0.0 { physics.on_ground = true; }
            pos.y -= physics.velocity.y * dt;
            physics.velocity.y = 0.0;
        } else {
            physics.on_ground = false;
        }

        tf.translation = pos;
    }
}

// =============================================================================
// HUD & HOTBAR
// =============================================================================

fn setup_hud(mut commands: Commands, asset_server: Res<AssetServer>, icons: Res<BlockIcons>, hotbar: Res<HotbarState>) {
    let font = asset_server.load("fonts/VCROSDMonoNova.ttf");
    
    // Debug
    commands.spawn((
        DebugOverlay,
        TextBundle::from_section("", TextStyle { font: font.clone(), font_size: 16.0, color: Color::srgb(0.9, 1.0, 0.9) })
        .with_style(Style { position_type: PositionType::Absolute, top: Val::Px(10.0), left: Val::Px(10.0), display: Display::None, ..default() }),
        InGameEntity,
    ));

    // Crosshair
    for (w, h, ml, mt) in [(2.0f32, 18.0f32, -1.0f32, -9.0f32), (18.0, 2.0, -9.0, -1.0)] {
        commands.spawn((
            NodeBundle {
                style: Style { position_type: PositionType::Absolute, width: Val::Px(w), height: Val::Px(h), left: Val::Percent(50.0), top: Val::Percent(50.0), margin: UiRect { left: Val::Px(ml), top: Val::Px(mt), ..default() }, ..default() },
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
                asset_server.load("textures/block/dirt.png") // Fallback, though we won't show it ideally
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

// =============================================================================
// MENU & APP CORE (UNCHANGED)
// =============================================================================

fn toggle_debug(keys: Res<ButtonInput<KeyCode>>, mut debug: ResMut<DebugState>, mut query: Query<&mut Style, With<DebugOverlay>>) {
    if keys.just_pressed(KeyCode::F3) {
        debug.visible = !debug.visible;
        for mut style in &mut query { style.display = if debug.visible { Display::Flex } else { Display::None }; }
    }
}

fn update_debug_overlay(
    debug: Res<DebugState>, diagnostics: Res<DiagnosticsStore>,
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

fn grab_cursor(mut windows_q: Query<&mut Window>, mouse: Res<ButtonInput<MouseButton>>) {
    let Ok(mut window) = windows_q.get_single_mut() else { return; };
    if mouse.just_pressed(MouseButton::Left) && window.cursor.grab_mode == CursorGrabMode::None {
        window.cursor.grab_mode = CursorGrabMode::Locked;
        window.cursor.visible = false;
    }
}

fn escape_key(keys: Res<ButtonInput<KeyCode>>, mut windows_q: Query<&mut Window>, mut next_state: ResMut<NextState<AppState>>) {
    if !keys.just_pressed(KeyCode::Escape) { return; }
    let Ok(mut window) = windows_q.get_single_mut() else { return; };
    if window.cursor.grab_mode == CursorGrabMode::Locked {
        window.cursor.grab_mode = CursorGrabMode::None;
        window.cursor.visible = true;
    } else {
        next_state.set(AppState::Menu);
    }
}

#[derive(Component)]
struct MenuRoot;
#[derive(Component)]
struct MenuCamera;
#[derive(Component)]
enum MenuButton { CreateWorld, Quit }

fn setup_menu(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/VCROSDMonoNova.ttf");
    commands.insert_resource(ClearColor(Color::srgb(0.08, 0.08, 0.12)));
    commands.spawn((Camera2dBundle { camera: Camera { order: 1, ..default() }, ..default() }, MenuCamera));
    commands.spawn((
        MenuRoot,
        NodeBundle { style: Style { width: Val::Percent(100.0), height: Val::Percent(100.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, justify_content: JustifyContent::Center, row_gap: Val::Px(20.0), ..default() }, ..default() },
    )).with_children(|p| {
        p.spawn(TextBundle::from_section("Воксельная Игра", TextStyle { font: font.clone(), font_size: 80.0, color: Color::srgb(0.95, 0.95, 0.5) }).with_style(Style { margin: UiRect::bottom(Val::Px(50.0)), ..default() }));
        spawn_btn(p, "Создать Мир", MenuButton::CreateWorld, font.clone());
        spawn_btn(p, "Выход", MenuButton::Quit, font);
    });
}
fn spawn_btn(parent: &mut ChildBuilder, label: &str, tag: MenuButton, font: Handle<Font>) {
    parent.spawn((tag, ButtonBundle { style: Style { width: Val::Px(300.0), height: Val::Px(64.0), align_items: AlignItems::Center, justify_content: JustifyContent::Center, border: UiRect::all(Val::Px(2.0)), ..default() }, background_color: BackgroundColor(Color::srgb(0.15, 0.18, 0.30)), border_color: BorderColor(Color::srgb(0.4, 0.5, 0.8)), ..default() }))
    .with_children(|b| { b.spawn(TextBundle::from_section(label, TextStyle { font, font_size: 30.0, color: Color::WHITE })); });
}
fn teardown_menu(mut commands: Commands, menu_q: Query<Entity, With<MenuRoot>>, cam_q: Query<Entity, With<MenuCamera>>) {
    for e in &menu_q { commands.entity(e).despawn_recursive(); }
    for e in &cam_q { commands.entity(e).despawn_recursive(); }
}
fn menu_button_system(mut q: Query<(&Interaction, &MenuButton, &mut BackgroundColor), Changed<Interaction>>, mut next_state: ResMut<NextState<AppState>>, mut app_exit: EventWriter<AppExit>) {
    for (interaction, button, mut bg) in &mut q {
        match interaction {
            Interaction::Hovered => bg.0 = Color::srgb(0.28, 0.35, 0.55),
            Interaction::Pressed => { bg.0 = Color::srgb(0.10, 0.45, 0.25); match button { MenuButton::CreateWorld => next_state.set(AppState::InGame), MenuButton::Quit => { app_exit.send(AppExit::Success); } } }
            Interaction::None => bg.0 = Color::srgb(0.15, 0.18, 0.30),
        }
    }
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "Воксельная Игра".into(), resolution: (1280.0, 720.0).into(), present_mode: PresentMode::AutoVsync, ..default() }),
            ..default()
        }).set(ImagePlugin::default_nearest()))
        .add_plugins(FrameTimeDiagnosticsPlugin)
        .init_state::<AppState>()
        .init_resource::<WorldManager>()
        .init_resource::<DebugState>()
        .init_resource::<HotbarState>()
        .add_systems(Startup, setup_materials)
        .add_systems(OnEnter(AppState::Menu), setup_menu)
        .add_systems(OnExit(AppState::Menu), teardown_menu)
        .add_systems(Update, menu_button_system.run_if(in_state(AppState::Menu)))
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
                update_hotbar_ui,
                block_interaction,
                grab_cursor,
                escape_key,
                toggle_debug,
                update_debug_overlay,
            ).run_if(in_state(AppState::InGame)),
        )
        .run();
}
