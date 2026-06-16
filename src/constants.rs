// =============================================================================
// Game constants — edit here to tune gameplay and world generation
// =============================================================================

// World
pub const CHUNK_SIZE: usize = 16;
pub const CHUNK_VOL:  usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;
pub const RENDER_DISTANCE: i32 = 4;

pub const SEED: u64 = 42;
pub const TERRAIN_SCALE: f64 = 0.07;
pub const TERRAIN_HEIGHT: f64 = 12.0;
pub const WATER_LEVEL: usize = 7;

// Camera / View
pub const FOV_DEG: f32 = 90.0;
pub const VIEW_MODEL_LAYER: usize = 1;
pub const MOUSE_SENSITIVITY: f32 = 0.003;

// Player size
pub const PLAYER_WIDTH: f32 = 0.6;
pub const PLAYER_HEIGHT_STAND: f32 = 1.8;
pub const PLAYER_HEIGHT_SNEAK: f32 = 1.5;
pub const CAMERA_HEIGHT_STAND: f32 = 1.6;
pub const CAMERA_HEIGHT_SNEAK: f32 = 1.3;

// Movement
pub const MOVE_SPEED: f32 = 4.5;        // blocks/s (Minecraft: 4.317)
pub const SPRINT_MULTIPLIER: f32 = 1.3; // ~5.85 blocks/s
pub const SNEAK_MULTIPLIER: f32 = 0.3;
pub const FLY_SPEED: f32 = 10.0;
pub const FLY_TOGGLE_TIME: f32 = 0.3;   // seconds between double-tap

// Physics
pub const GROUND_ACCEL: f32 = 50.0;
pub const AIR_ACCEL: f32 = 3.0;
pub const GROUND_FRICTION: f32 = 0.88;
pub const AIR_FRICTION: f32 = 0.9965;
pub const GRAVITY: f32 = 25.0;
pub const JUMP_VELOCITY: f32 = 8.5;
