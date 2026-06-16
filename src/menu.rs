// =============================================================================
// Menu: main menu UI and navigation
// =============================================================================

use bevy::prelude::*;
use crate::state::AppState;

#[derive(Component)] pub struct MenuRoot;
#[derive(Component)] pub struct MenuCamera;
#[derive(Component)] pub enum MenuButton { CreateWorld, Quit }

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(OnEnter(AppState::Menu), setup_menu)
            .add_systems(OnExit(AppState::Menu),  teardown_menu)
            .add_systems(Update, menu_button_system.run_if(in_state(AppState::Menu)));
    }
}

fn setup_menu(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/VCROSDMonoNova.ttf");
    commands.insert_resource(ClearColor(Color::srgb(0.08, 0.08, 0.12)));
    commands.spawn((
        Camera2dBundle { camera: Camera { order: 1, ..default() }, ..default() },
        MenuCamera,
    ));
    commands.spawn((
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
    )).with_children(|p| {
        p.spawn(
            TextBundle::from_section(
                "Воксельная Игра",
                TextStyle { font: font.clone(), font_size: 80.0, color: Color::srgb(0.95, 0.95, 0.5) },
            ).with_style(Style { margin: UiRect::bottom(Val::Px(50.0)), ..default() }),
        );
        spawn_btn(p, "Создать Мир", MenuButton::CreateWorld, font.clone());
        spawn_btn(p, "Выход",       MenuButton::Quit,        font);
    });
}

fn spawn_btn(parent: &mut ChildBuilder, label: &str, tag: MenuButton, font: Handle<Font>) {
    parent.spawn((
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
    )).with_children(|b| {
        b.spawn(TextBundle::from_section(
            label,
            TextStyle { font, font_size: 30.0, color: Color::WHITE },
        ));
    });
}

fn teardown_menu(
    mut commands: Commands,
    menu_q: Query<Entity, With<MenuRoot>>,
    cam_q:  Query<Entity, With<MenuCamera>>,
) {
    for e in &menu_q { commands.entity(e).despawn_recursive(); }
    for e in &cam_q  { commands.entity(e).despawn_recursive(); }
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
                    MenuButton::Quit        => { app_exit.send(AppExit::Success); }
                }
            }
            Interaction::None => bg.0 = Color::srgb(0.15, 0.18, 0.30),
        }
    }
}
