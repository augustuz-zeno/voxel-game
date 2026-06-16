// =============================================================================
// Block types and their texture / tint mappings
// =============================================================================

/// Which face texture slot to use when building a mesh face.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum BlockTexture {
    Dirt = 0,
    GrassTop = 1,
    GrassSide = 2,
    Stone = 3,
    Sand = 4,
    Water = 5,
    WoodTop = 6,
    WoodSide = 7,
    Leaves = 8,
    Bedrock = 9,
}

impl BlockTexture {
    pub fn is_transparent(self) -> bool {
        matches!(self, BlockTexture::Water | BlockTexture::Leaves)
    }

    pub fn from_block(block: BlockType, normal: [i32; 3]) -> Self {
        match block {
            BlockType::Air => BlockTexture::Dirt,
            BlockType::Grass => {
                if normal == [0, 1, 0] { BlockTexture::GrassTop }
                else if normal == [0, -1, 0] { BlockTexture::Dirt }
                else { BlockTexture::GrassSide }
            }
            BlockType::Dirt    => BlockTexture::Dirt,
            BlockType::Stone   => BlockTexture::Stone,
            BlockType::Sand    => BlockTexture::Sand,
            BlockType::Water   => BlockTexture::Water,
            BlockType::Wood    => if normal[1] != 0 { BlockTexture::WoodTop } else { BlockTexture::WoodSide },
            BlockType::Leaves  => BlockTexture::Leaves,
            BlockType::Bedrock => BlockTexture::Bedrock,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[repr(u8)]
pub enum BlockType {
    Air     = 0,
    Grass   = 1,
    Dirt    = 2,
    Stone   = 3,
    Sand    = 4,
    Water   = 5,
    Wood    = 6,
    Leaves  = 7,
    Bedrock = 8,
}

impl BlockType {
    #[inline] pub fn is_transparent(self) -> bool {
        matches!(self, BlockType::Air | BlockType::Water | BlockType::Leaves)
    }
    #[inline] pub fn is_solid(self) -> bool {
        !matches!(self, BlockType::Air | BlockType::Water)
    }

    pub fn tint(self, normal: [i32; 3]) -> [f32; 4] {
        match self {
            BlockType::Grass if normal == [0, 1, 0] => [0.45, 0.73, 0.33, 1.0],
            BlockType::Leaves => [0.3, 0.6, 0.2, 1.0],
            BlockType::Water  => [0.2, 0.4, 0.8, 0.8],
            _ => [1.0, 1.0, 1.0, 1.0],
        }
    }
}
