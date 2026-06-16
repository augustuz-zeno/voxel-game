// =============================================================================
// Voxel Game — Bevy 0.14
// =============================================================================

mod blocks;
mod constants;
mod menu;
mod player;
mod render;
mod state;
mod ui;
mod world;

use bevy::{
    diagnostic::FrameTimeDiagnosticsPlugin,
    prelude::*,
    render::{
        settings::{RenderCreation, WgpuSettings},
        RenderPlugin,
    },
    window::{PresentMode, WindowPlugin},
};

use state::AppState;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Воксельная Игра".into(),
                        resolution: (1280.0, 720.0).into(),
                        present_mode: PresentMode::AutoVsync,
                        ..default()
                    }),
                    ..default()
                })
                .set(ImagePlugin {
                    default_sampler: bevy::render::texture::ImageSamplerDescriptor {
                        address_mode_u: bevy::render::texture::ImageAddressMode::Repeat,
                        address_mode_v: bevy::render::texture::ImageAddressMode::Repeat,
                        address_mode_w: bevy::render::texture::ImageAddressMode::Repeat,
                        mag_filter: bevy::render::texture::ImageFilterMode::Nearest,
                        min_filter: bevy::render::texture::ImageFilterMode::Nearest,
                        mipmap_filter: bevy::render::texture::ImageFilterMode::Nearest,
                        ..default()
                    },
                })
                .set(RenderPlugin {
                    render_creation: RenderCreation::Automatic(WgpuSettings {
                        backends: Some(bevy::render::settings::Backends::VULKAN),
                        ..default()
                    }),
                    ..default()
                }),
            FrameTimeDiagnosticsPlugin,
        ))
        .init_state::<AppState>()
        // Game plugins
        .add_plugins((
            render::GameRenderPlugin,
            menu::MenuPlugin,
            world::WorldPlugin,
            player::PlayerPlugin,
            ui::UiPlugin,
        ))
        .run();
}
