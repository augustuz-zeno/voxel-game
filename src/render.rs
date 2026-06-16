// =============================================================================
// World rendering: BlockMaterials and BlockIcons resources
// =============================================================================

use bevy::prelude::*;
use bevy::render::render_resource::Face;
use bevy::utils::HashMap;
use crate::blocks::{BlockTexture, BlockType};
use crate::state::AppState;

#[derive(Resource)]
pub struct BlockMaterials {
    pub materials: HashMap<BlockTexture, Handle<StandardMaterial>>,
}

#[derive(Resource)]
pub struct BlockIcons {
    pub icons: HashMap<BlockType, Handle<Image>>,
}

pub struct GameRenderPlugin;

impl Plugin for GameRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_materials);
    }
}

fn setup_materials(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut mat_map  = HashMap::new();
    let mut icon_map = HashMap::new();

    let tex_paths = [
        (BlockTexture::Dirt,      "textures/block/dirt.png"),
        (BlockTexture::GrassTop,  "textures/block/grass_block_top.png"),
        (BlockTexture::GrassSide, "textures/block/grass_block_side.png"),
        (BlockTexture::Stone,     "textures/block/stone.png"),
        (BlockTexture::Sand,      "textures/block/sand.png"),
        (BlockTexture::Water,     "textures/block/water_still.png"),
        (BlockTexture::WoodTop,   "textures/block/oak_log_top.png"),
        (BlockTexture::WoodSide,  "textures/block/oak_log.png"),
        (BlockTexture::Leaves,    "textures/block/oak_leaves.png"),
        (BlockTexture::Bedrock,   "textures/block/bedrock.png"),
    ];

    for (tex, path) in tex_paths {
        let handle = asset_server.load(path);

        let alpha_mode = if tex == BlockTexture::Water || tex == BlockTexture::Leaves {
            // Use Mask for leaves to look like standard Minecraft
            if tex == BlockTexture::Leaves { AlphaMode::Mask(0.5) } else { AlphaMode::Blend }
        } else {
            AlphaMode::Opaque
        };

        let mat = materials.add(StandardMaterial {
            base_color_texture: Some(handle.clone()),
            perceptual_roughness: 1.0, // Fully matte (Minecraft style)
            reflectance: 0.0,          // No specular highlights
            alpha_mode,
            cull_mode: if tex == BlockTexture::Leaves || tex == BlockTexture::Water {
                None
            } else {
                Some(Face::Back)
            },
            ..default()
        });
        mat_map.insert(tex, mat);

        match tex {
            BlockTexture::Dirt      => { icon_map.insert(BlockType::Dirt,    handle.clone()); }
            BlockTexture::GrassTop  => { icon_map.insert(BlockType::Grass,   handle.clone()); }
            BlockTexture::Stone     => { icon_map.insert(BlockType::Stone,   handle.clone()); }
            BlockTexture::Sand      => { icon_map.insert(BlockType::Sand,    handle.clone()); }
            BlockTexture::Water     => { icon_map.insert(BlockType::Water,   handle.clone()); }
            BlockTexture::WoodSide  => { icon_map.insert(BlockType::Wood,    handle.clone()); }
            BlockTexture::Leaves    => { icon_map.insert(BlockType::Leaves,  handle.clone()); }
            BlockTexture::Bedrock   => { icon_map.insert(BlockType::Bedrock, handle.clone()); }
            _ => {}
        }
    }

    commands.insert_resource(BlockMaterials { materials: mat_map });
    commands.insert_resource(BlockIcons { icons: icon_map });
}
