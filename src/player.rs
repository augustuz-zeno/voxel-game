// =============================================================================
// Player movement, looking, and interaction with the world
// =============================================================================

use bevy::input::mouse::MouseMotion;
use bevy::math::Vec3A;
use bevy::prelude::*;
use bevy::render::primitives::{Frustum, Sphere};
use bevy::render::view::RenderLayers;
use bevy::window::CursorGrabMode;

use crate::blocks::BlockType;
use crate::constants::*;
use crate::render::BlockMaterials;
use crate::state::{AppState, InGameEntity};
use crate::ui::HotbarState;
use crate::world::{get_world_block, set_world_block, terrain_height, Chunk, ChunkVisible, WorldManager};

#[derive(Component)]
pub struct Player {
    pub pitch: f32,
    pub yaw: f32,
}

#[derive(Component)]
pub struct PlayerPhysics {
    pub velocity: Vec3,
    pub on_ground: bool,
    pub fly_mode: bool,
    pub sneaking: bool,
    pub last_space_time: f32,
}

impl Default for PlayerPhysics {
    fn default() -> Self {
        Self { velocity: Vec3::ZERO, on_ground: false, fly_mode: false, sneaking: false, last_space_time: 0.0 }
    }
}

#[derive(Component)] pub struct PlayerCamera;
#[derive(Component)] pub struct ViewModelCamera;
#[derive(Component)] pub struct PlayerHand;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(OnEnter(AppState::InGame), setup_world)
            .add_systems(OnExit(AppState::InGame), teardown_world)
            .add_systems(
                Update,
                (
                    frustum_culling_system,
                    player_look,
                    player_move,
                    block_interaction,
                ).run_if(in_state(AppState::InGame)),
            );
    }
}

fn setup_world(
    mut commands: Commands,
    mut world_mgr: ResMut<WorldManager>,
    mut meshes: ResMut<Assets<Mesh>>,
    block_materials: Res<BlockMaterials>,
) {
    world_mgr.loaded_chunks.clear();

    commands.spawn((
        DirectionalLightBundle {
            directional_light: DirectionalLight { 
                illuminance: 10_000.0, // Bright sunlight
                shadows_enabled: false,
                color: Color::srgb(1.0, 0.98, 0.95),
                ..default() 
            },
            transform: Transform::from_xyz(50.0, 100.0, 50.0).looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        },
        InGameEntity,
    ));

    commands.insert_resource(AmbientLight { color: Color::srgb(0.6, 0.7, 1.0), brightness: 300.0 });
    commands.insert_resource(ClearColor(Color::srgb(0.45, 0.65, 1.0))); // Vibrant Minecraft sky blue

    let spawn_y = terrain_height(0, 0) as f32 + 2.5;

    // Hand mesh — a block-shaped cuboid held in view
    let hand_mesh = meshes.add(Cuboid::new(0.35, 0.65, 0.35));
    let hand_mat = block_materials.materials
        .get(&crate::blocks::BlockTexture::Stone)
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
                    tonemapping: bevy::core_pipeline::tonemapping::Tonemapping::None,
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
                        tonemapping: bevy::core_pipeline::tonemapping::Tonemapping::None,
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
    world_mgr.chunk_map.clear();
    ambient.brightness = 80.0;
    clear_color.0 = Color::srgb(0.08, 0.08, 0.12);
}

pub fn frustum_culling_system(
    mut commands: Commands,
    camera_q: Query<&Frustum, With<PlayerCamera>>,
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

fn check_aabb_collision(pos: Vec3, half_w: f32, height: f32, chunks_q: &Query<&mut Chunk>, chunk_map: &bevy::utils::HashMap<IVec3, Entity>) -> bool {
    let min_bx = (pos.x - half_w).floor() as i32;
    let max_bx = (pos.x + half_w - 0.001).floor() as i32;
    let min_by = pos.y.floor() as i32;
    let max_by = (pos.y + height - 0.001).floor() as i32;
    let min_bz = (pos.z - half_w).floor() as i32;
    let max_bz = (pos.z + half_w - 0.001).floor() as i32;

    for by in min_by..=max_by {
        for bz in min_bz..=max_bz {
            for bx in min_bx..=max_bx {
                let block = get_world_block(bx, by, bz, chunks_q, chunk_map);
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

fn raycast_blocks(start: Vec3, dir: Vec3, max_dist: f32, chunks_q: &Query<&mut Chunk>, chunk_map: &bevy::utils::HashMap<IVec3, Entity>) -> Option<(IVec3, IVec3)> {
    let mut t = 0.0;
    let mut current_pos = start;
    let step = 0.05;
    let mut last_voxel = IVec3::new(start.x.floor() as i32, start.y.floor() as i32, start.z.floor() as i32);

    while t < max_dist {
        t += step;
        current_pos += dir * step;
        let voxel = IVec3::new(current_pos.x.floor() as i32, current_pos.y.floor() as i32, current_pos.z.floor() as i32);
        
        if voxel != last_voxel {
            let block = get_world_block(voxel.x, voxel.y, voxel.z, chunks_q, chunk_map);
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
    cam_q: Query<&GlobalTransform, With<PlayerCamera>>,
    player_q: Query<(&Transform, &PlayerPhysics), With<Player>>,
    mut chunks_q: Query<&mut Chunk>,
    hotbar: Res<HotbarState>,
    world_mgr: Res<WorldManager>,
) {
    if !mouse.just_pressed(MouseButton::Left) && !mouse.just_pressed(MouseButton::Right) { return; }
    let Ok(cam_tf) = cam_q.get_single() else { return; };
    let Ok((player_tf, physics)) = player_q.get_single() else { return; };
    
    let start = cam_tf.translation();
    let dir = cam_tf.forward().normalize();
    
    if let Some((hit, prev)) = raycast_blocks(start, dir, 5.0, &chunks_q, &world_mgr.chunk_map) {
        if mouse.just_pressed(MouseButton::Left) {
            set_world_block(hit.x, hit.y, hit.z, BlockType::Air, &mut chunks_q);
        } else if mouse.just_pressed(MouseButton::Right) {
            let block = hotbar.blocks[hotbar.active_index];
            if block != BlockType::Air {
                // Check if placing block intersects the player
                let p_pos = player_tf.translation;
                let half_w = PLAYER_WIDTH * 0.5;
                let h = if physics.sneaking && !physics.fly_mode { PLAYER_HEIGHT_SNEAK } else { PLAYER_HEIGHT_STAND };
                
                let p_min = Vec3::new(p_pos.x - half_w, p_pos.y, p_pos.z - half_w);
                let p_max = Vec3::new(p_pos.x + half_w, p_pos.y + h, p_pos.z + half_w);
                
                let b_min = Vec3::new(prev.x as f32, prev.y as f32, prev.z as f32);
                let b_max = b_min + Vec3::ONE;
                
                let intersects = p_min.x < b_max.x && p_max.x > b_min.x && 
                                 p_min.y < b_max.y && p_max.y > b_min.y && 
                                 p_min.z < b_max.z && p_max.z > b_min.z;
                                 
                if !intersects {
                    set_world_block(prev.x, prev.y, prev.z, block, &mut chunks_q);
                }
            }
        }
    }
}

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
    world_mgr: Res<WorldManager>,
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
                physics.velocity = Vec3::ZERO;
            } else {
                physics.velocity.y = 0.0;
            }
            physics.last_space_time = 0.0;
        } else {
            physics.last_space_time = now;
        }
    }

    // --- Wish direction ---
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
        let fly_speed = FLY_SPEED * if sprint { SPRINT_MULTIPLIER } else { 1.0 };

        let mut y_input = 0.0;
        if keys.pressed(KeyCode::Space) { y_input += 1.0; }
        if keys.pressed(KeyCode::ShiftLeft) { y_input -= 1.0; }

        let target_xz = wish_dir * fly_speed;
        physics.velocity.x += (target_xz.x - physics.velocity.x) * (1.0 - 0.1_f32.powf(dt * 15.0));
        physics.velocity.z += (target_xz.z - physics.velocity.z) * (1.0 - 0.1_f32.powf(dt * 15.0));
        let target_y = y_input * fly_speed;
        physics.velocity.y += (target_y - physics.velocity.y) * (1.0 - 0.1_f32.powf(dt * 20.0));

        let mut pos = tf.translation;
        pos.x += physics.velocity.x * dt;
        if check_aabb_collision(pos, half_w, h, &chunks_q, &world_mgr.chunk_map) {
            pos.x -= physics.velocity.x * dt;
            physics.velocity.x = 0.0;
        }
        pos.z += physics.velocity.z * dt;
        if check_aabb_collision(pos, half_w, h, &chunks_q, &world_mgr.chunk_map) {
            pos.z -= physics.velocity.z * dt;
            physics.velocity.z = 0.0;
        }
        pos.y += physics.velocity.y * dt;
        if check_aabb_collision(pos, half_w, h, &chunks_q, &world_mgr.chunk_map) {
            if physics.velocity.y < 0.0 {
                physics.fly_mode = false;
                physics.on_ground = true;
            }
            pos.y -= physics.velocity.y * dt;
            physics.velocity.y = 0.0;
        }
        tf.translation = pos;

    } else {
        let accel = if physics.on_ground { GROUND_ACCEL } else { AIR_ACCEL };
        let friction = if physics.on_ground { GROUND_FRICTION } else { AIR_FRICTION };

        let current_speed_in_dir = Vec3::new(physics.velocity.x, 0.0, physics.velocity.z).dot(wish_dir);
        let add_speed = (target_speed - current_speed_in_dir).max(0.0);
        let accel_amount = (accel * target_speed * dt).min(add_speed);

        physics.velocity.x += wish_dir.x * accel_amount;
        physics.velocity.z += wish_dir.z * accel_amount;

        physics.velocity.x *= friction;
        physics.velocity.z *= friction;

        physics.velocity.y -= GRAVITY * dt;
        physics.velocity.y = physics.velocity.y.max(-50.0);

        if keys.pressed(KeyCode::Space) && physics.on_ground {
            physics.velocity.y = JUMP_VELOCITY;
            physics.on_ground = false;
        }

        let mut pos = tf.translation;
        let mut next_x = pos.x + physics.velocity.x * dt;
        let mut next_z = pos.z + physics.velocity.z * dt;

        if physics.sneaking && physics.on_ground {
            let check_pos_x = Vec3::new(next_x, pos.y - 0.05, pos.z);
            if !check_aabb_collision(check_pos_x, half_w, h, &chunks_q, &world_mgr.chunk_map) {
                physics.velocity.x = 0.0;
                next_x = pos.x;
            }
            let check_pos_z = Vec3::new(pos.x, pos.y - 0.05, next_z);
            if !check_aabb_collision(check_pos_z, half_w, h, &chunks_q, &world_mgr.chunk_map) {
                physics.velocity.z = 0.0;
                next_z = pos.z;
            }
        }

        pos.x = next_x;
        if check_aabb_collision(pos, half_w, h, &chunks_q, &world_mgr.chunk_map) {
            pos.x = tf.translation.x;
            physics.velocity.x = 0.0;
        }

        pos.z = next_z;
        if check_aabb_collision(pos, half_w, h, &chunks_q, &world_mgr.chunk_map) {
            pos.z = tf.translation.z;
            physics.velocity.z = 0.0;
        }

        pos.y += physics.velocity.y * dt;
        if check_aabb_collision(pos, half_w, h, &chunks_q, &world_mgr.chunk_map) {
            if physics.velocity.y <= 0.0 { physics.on_ground = true; }
            pos.y -= physics.velocity.y * dt;
            physics.velocity.y = 0.0;
        } else {
            physics.on_ground = false;
        }

        tf.translation = pos;
    }
}
