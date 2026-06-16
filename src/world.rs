// =============================================================================
// World generation, Chunk streaming, Meshing, and WorldManager
// =============================================================================

use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use bevy::utils::HashMap;
use futures_lite::future;

use crate::blocks::{BlockTexture, BlockType};
use crate::constants::*;
use crate::render::BlockMaterials;
use crate::state::AppState;
use crate::player::Player; // Needed for stream_chunks

#[derive(Resource, Default)]
pub struct WorldManager {
    pub loaded_chunks: std::collections::HashSet<IVec3>,
    pub generating_chunks: std::collections::HashSet<IVec3>,
    pub chunk_map: HashMap<IVec3, Entity>, // O(1) block lookup
}

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<WorldManager>()
            .add_systems(
                Update,
                (
                    stream_chunks,
                    poll_chunk_tasks,
                    rebuild_dirty_chunks, // Should run after frustum culling ideally, but Bevy's scheduler handles this. We will order them in main or player.rs.
                ).run_if(in_state(AppState::InGame)),
            );
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

pub fn terrain_height(wx: i32, wz: i32) -> usize {
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

#[derive(Component)]
pub struct ChunkGenerationTask(pub Task<(IVec3, ChunkData, HashMap<BlockTexture, Mesh>)>);

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

#[derive(Clone, Copy, PartialEq)]
struct FaceInfo {
    tex: BlockTexture,
    ao: [f32; 4],
    tint: [f32; 4],
}

type MeshBuilderData = (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<[f32; 4]>, Vec<u32>);

pub fn build_chunk_meshes(chunk: &ChunkData) -> HashMap<BlockTexture, Mesh> {
    let mut builders: HashMap<BlockTexture, MeshBuilderData> = HashMap::new();

    // 0: +X, 1: -X, 2: +Y, 3: -Y, 4: +Z, 5: -Z
    let dirs = [
        ([1, 0, 0], 2, 1),   // +X: U=Z, V=Y
        ([-1, 0, 0], 2, 1),  // -X: U=Z, V=Y
        ([0, 1, 0], 0, 2),   // +Y: U=X, V=Z
        ([0, -1, 0], 0, 2),  // -Y: U=X, V=Z
        ([0, 0, 1], 0, 1),   // +Z: U=X, V=Y
        ([0, 0, -1], 0, 1),  // -Z: U=X, V=Y
    ];

    let ao_offsets = [
        [[1, -1, -1], [1, 1, -1], [1, 1, 1], [1, -1, 1]],     // +X
        [[-1, -1, 1], [-1, 1, 1], [-1, 1, -1], [-1, -1, -1]], // -X
        [[-1, 1, -1], [-1, 1, 1], [1, 1, 1], [1, 1, -1]],     // +Y
        [[-1, -1, 1], [-1, -1, -1], [1, -1, -1], [1, -1, 1]], // -Y
        [[1, -1, 1], [1, 1, 1], [-1, 1, 1], [-1, -1, 1]],     // +Z
        [[-1, -1, -1], [-1, 1, -1], [1, 1, -1], [1, -1, -1]], // -Z
    ];

    for (face, &((dir), u_axis, v_axis)) in dirs.iter().enumerate() {
        let fixed_axis = 3 - u_axis - v_axis;
        
        for slice in 0..CHUNK_SIZE {
            let mut mask: [Option<FaceInfo>; CHUNK_SIZE * CHUNK_SIZE] = [None; CHUNK_SIZE * CHUNK_SIZE];

            // 1. Build mask
            for v in 0..CHUNK_SIZE {
                for u in 0..CHUNK_SIZE {
                    let mut pos = [0; 3];
                    pos[u_axis] = u;
                    pos[v_axis] = v;
                    pos[fixed_axis] = slice;

                    let block = chunk.get(pos[0], pos[1], pos[2]);
                    if block == BlockType::Air { continue; }

                    let nx = pos[0] as i32 + dir[0];
                    let ny = pos[1] as i32 + dir[1];
                    let nz = pos[2] as i32 + dir[2];

                    let nb = chunk.get_safe(nx, ny, nz);
                    if !nb.is_transparent() { continue; }
                    if block == BlockType::Water && nb == BlockType::Water { continue; }

                    let tex = BlockTexture::from_block(block, dir);
                    let mut tint = block.tint(dir);
                    
                    let mut ao_vals = [1.0; 4];
                    for i in 0..4 {
                        let offsets = ao_offsets[face][i];
                        let s1 = chunk.get_safe(pos[0] as i32 + offsets[0], pos[1] as i32 + offsets[1], pos[2] as i32 + dir[2]).is_solid();
                        let s2 = chunk.get_safe(pos[0] as i32 + dir[0], pos[1] as i32 + offsets[1], pos[2] as i32 + offsets[2]).is_solid();
                        let corner = chunk.get_safe(pos[0] as i32 + offsets[0], pos[1] as i32 + offsets[1], pos[2] as i32 + offsets[2]).is_solid();
                        ao_vals[i] = vertex_ao(s1, s2, corner);
                    }

                    // Apply directional lighting
                    let light = if dir[1] > 0 { 1.0 } else if dir[1] < 0 { 0.6 } else if dir[2] != 0 { 0.8 } else { 0.7 };
                    for i in 0..3 { tint[i] *= light; }

                    mask[u + v * CHUNK_SIZE] = Some(FaceInfo { tex, ao: ao_vals, tint });
                }
            }

            // 2. Greedy Sweep
            for v in 0..CHUNK_SIZE {
                for u in 0..CHUNK_SIZE {
                    if let Some(info) = mask[u + v * CHUNK_SIZE] {
                        let mut w = 1;
                        // Sweep U
                        while u + w < CHUNK_SIZE && mask[u + w + v * CHUNK_SIZE] == Some(info) {
                            w += 1;
                        }

                        let mut h = 1;
                        // Sweep V
                        'outer: while v + h < CHUNK_SIZE {
                            for i in 0..w {
                                if mask[u + i + (v + h) * CHUNK_SIZE] != Some(info) {
                                    break 'outer;
                                }
                            }
                            h += 1;
                        }

                        // Clear merged faces from mask
                        for j in 0..h {
                            for i in 0..w {
                                mask[u + i + (v + j) * CHUNK_SIZE] = None;
                            }
                        }

                        // Build Quad
                        let builder = builders.entry(info.tex).or_insert_with(|| (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new()));
                        let base_idx = builder.0.len() as u32;

                        let mut pos_min = [0; 3];
                        pos_min[u_axis] = u;
                        pos_min[v_axis] = v;
                        pos_min[fixed_axis] = slice;
                        
                        let px = pos_min[0] as f32;
                        let py = pos_min[1] as f32;
                        let pz = pos_min[2] as f32;
                        let wf = w as f32;
                        let hf = h as f32;

                        let y_off = if info.tex == BlockTexture::Water && dir[1] == 1 { -0.15 } else { 0.0 };

                        let corners = match face {
                            0 => [[px+1., py, pz], [px+1., py+hf, pz], [px+1., py+hf, pz+wf], [px+1., py, pz+wf]],
                            1 => [[px, py, pz+wf], [px, py+hf, pz+wf], [px, py+hf, pz], [px, py, pz]],
                            2 => [[px, py+1.+y_off, pz], [px, py+1.+y_off, pz+hf], [px+wf, py+1.+y_off, pz+hf], [px+wf, py+1.+y_off, pz]],
                            3 => [[px, py, pz+hf], [px, py, pz], [px+wf, py, pz], [px+wf, py, pz+hf]],
                            4 => [[px+wf, py, pz+1.], [px+wf, py+hf, pz+1.], [px, py+hf, pz+1.], [px, py, pz+1.]],
                            5 => [[px, py, pz], [px, py+hf, pz], [px+wf, py+hf, pz], [px+wf, py, pz]],
                            _ => unreachable!(),
                        };

                        for i in 0..4 {
                            builder.0.push(corners[i]);
                            builder.1.push([dir[0] as f32, dir[1] as f32, dir[2] as f32]);
                            
                            let mut color = info.tint;
                            let ao = info.ao[i];
                            color[0] *= ao; color[1] *= ao; color[2] *= ao;
                            builder.3.push(color);
                        }

                        // UVs that tile perfectly
                        builder.2.extend_from_slice(&[[0., hf], [0., 0.], [wf, 0.], [wf, hf]]);

                        if info.ao[0] + info.ao[2] < info.ao[1] + info.ao[3] {
                            builder.4.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2, base_idx, base_idx + 2, base_idx + 3]);
                        } else {
                            builder.4.extend_from_slice(&[base_idx + 1, base_idx + 2, base_idx + 3, base_idx + 1, base_idx + 3, base_idx]);
                        }
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
// SYSTEMS
// =============================================================================

pub fn stream_chunks(
    mut commands: Commands,
    player_q: Query<&Transform, With<Player>>,
    mut world_mgr: ResMut<WorldManager>,
    chunks_q: Query<(Entity, &Chunk)>,
) {
    let Ok(ptf) = player_q.get_single() else { return; };
    let px = (ptf.translation.x / CHUNK_SIZE as f32).floor() as i32;
    let pz = (ptf.translation.z / CHUNK_SIZE as f32).floor() as i32;

    let pool = AsyncComputeTaskPool::get();

    // Unload chunks that are too far away
    for (entity, chunk) in &chunks_q {
        let dx = (chunk.grid_pos.x - px).abs();
        let dz = (chunk.grid_pos.z - pz).abs();
        if dx > RENDER_DISTANCE + 1 || dz > RENDER_DISTANCE + 1 {
            commands.entity(entity).despawn_recursive();
            world_mgr.loaded_chunks.remove(&chunk.grid_pos);
            world_mgr.chunk_map.remove(&chunk.grid_pos);
        }
    }

    // Load new chunks in range
    for dz in -RENDER_DISTANCE..=RENDER_DISTANCE {
        for dx in -RENDER_DISTANCE..=RENDER_DISTANCE {
            let grid = IVec3::new(px + dx, 0, pz + dz);
            if !world_mgr.loaded_chunks.contains(&grid) && !world_mgr.generating_chunks.contains(&grid) {
                world_mgr.generating_chunks.insert(grid);
                let task = pool.spawn(async move {
                    let data = generate_chunk(grid);
                    let meshes = build_chunk_meshes(&data);
                    (grid, data, meshes)
                });
                commands.spawn(ChunkGenerationTask(task));
            }
        }
    }
}

pub fn poll_chunk_tasks(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    block_materials: Res<BlockMaterials>,
    mut tasks_q: Query<(Entity, &mut ChunkGenerationTask)>,
    mut world_mgr: ResMut<WorldManager>,
) {
    for (entity, mut task) in &mut tasks_q {
        if let Some((grid, data, chunk_meshes)) = future::block_on(future::poll_once(&mut task.0)) {
            world_mgr.generating_chunks.remove(&grid);
            
            // It's possible the player moved away before this finished.
            // Check if we still want this chunk.
            if !world_mgr.loaded_chunks.contains(&grid) {
                let chunk_entity = commands.spawn((
                    Chunk::new(grid, data),
                    ChunkVisible,
                    TransformBundle::from_transform(Transform::from_translation(grid.as_vec3() * CHUNK_SIZE as f32)),
                    VisibilityBundle::default(),
                )).id();

                for (tex, mesh) in chunk_meshes {
                    if let Some(mat) = block_materials.materials.get(&tex) {
                        let mesh_handle = meshes.add(mesh);
                        let child = commands.spawn((
                            PbrBundle { mesh: mesh_handle, material: mat.clone(), ..default() },
                            ChunkMeshNode,
                        )).id();
                        commands.entity(chunk_entity).add_child(child);
                    }
                }
                
                commands.entity(entity).despawn();
                world_mgr.loaded_chunks.insert(grid);
                world_mgr.chunk_map.insert(grid, chunk_entity);
            } else {
                commands.entity(entity).despawn();
            }
        }
    }
}

pub fn rebuild_dirty_chunks(
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

pub fn get_world_block(wx: i32, wy: i32, wz: i32, chunks_q: &Query<&mut Chunk>, chunk_map: &HashMap<IVec3, Entity>) -> BlockType {
    if wy < 0 { return BlockType::Bedrock; }
    let cx = wx.div_euclid(CHUNK_SIZE as i32);
    let cy = wy.div_euclid(CHUNK_SIZE as i32);
    let cz = wz.div_euclid(CHUNK_SIZE as i32);
    let lx = wx.rem_euclid(CHUNK_SIZE as i32) as usize;
    let ly = wy.rem_euclid(CHUNK_SIZE as i32) as usize;
    let lz = wz.rem_euclid(CHUNK_SIZE as i32) as usize;

    let grid = IVec3::new(cx, cy, cz);
    if let Some(&entity) = chunk_map.get(&grid) {
        if let Ok(chunk) = chunks_q.get(entity) {
            return chunk.data.get(lx, ly, lz);
        }
    }
    BlockType::Air
}

pub fn set_world_block(wx: i32, wy: i32, wz: i32, block: BlockType, chunks_q: &mut Query<&mut Chunk>) {
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
