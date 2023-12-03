use std::path::PathBuf;

use bevy::{
    core_pipeline::clear_color::ClearColorConfig,
    prelude::*,
    render::texture::{ImageLoaderSettings, ImageSampler},
};
use serde::{Deserialize, Serialize};
use yabuil::{views::NodeWorldViewMut, ActiveLayout, LayoutApp, LayoutAttribute, LayoutBundle};

#[derive(Deserialize, Serialize, Component, Copy, Clone, PartialEq, Eq)]
pub enum MainMenuButton {
    LocalPlay,
    OnlinePlay,
    Extras,
    Milestones,
    Options,
    Exit,
}

impl MainMenuButton {
    pub fn above(&self) -> Self {
        match self {
            Self::LocalPlay => Self::Exit,
            Self::OnlinePlay => Self::LocalPlay,
            Self::Extras => Self::OnlinePlay,
            Self::Milestones => Self::Extras,
            Self::Options => Self::Milestones,
            Self::Exit => Self::Options,
        }
    }

    pub fn below(&self) -> Self {
        match self {
            Self::LocalPlay => Self::OnlinePlay,
            Self::OnlinePlay => Self::Extras,
            Self::Extras => Self::Milestones,
            Self::Milestones => Self::Options,
            Self::Options => Self::Exit,
            Self::Exit => Self::LocalPlay,
        }
    }
}

impl LayoutAttribute for MainMenuButton {
    fn apply(&self, world: &mut yabuil::views::NodeWorldViewMut) {
        world.as_entity_world_mut().insert(*self);
        if matches!(self, MainMenuButton::LocalPlay) {
            world.as_entity_world_mut().insert(FocusedMenuButton);
            world.as_layout_node_mut().unwrap().play_animation("select");
        }
    }
}

#[derive(Component)]
pub struct FocusedMenuButton;

fn update_menu_buttons(
    mut commands: Commands,
    menu_move_sfx: Res<MenuMoveSfx>,
    gamepads: Res<Gamepads>,
    buttons: Res<Input<GamepadButton>>,
    main_menu_buttons: Query<(Entity, &MainMenuButton, Has<FocusedMenuButton>)>,
) {
    let is_down = gamepads
        .iter()
        .any(|gp| buttons.just_pressed(GamepadButton::new(gp, GamepadButtonType::DPadDown)));
    let is_up = gamepads
        .iter()
        .any(|gp| buttons.just_pressed(GamepadButton::new(gp, GamepadButtonType::DPadUp)));
    if is_down == is_up {
        return;
    }

    commands.spawn(AudioBundle {
        source: menu_move_sfx.0.clone(),
        settings: PlaybackSettings {
            mode: bevy::audio::PlaybackMode::Despawn,
            ..default()
        },
    });

    let (entity, button, _) = main_menu_buttons
        .iter()
        .find(|(_, _, is_focus)| *is_focus)
        .unwrap();
    commands
        .entity(entity)
        .remove::<FocusedMenuButton>()
        .add(|entity: EntityWorldMut| {
            let mut node = NodeWorldViewMut::new(entity).unwrap();
            node.as_layout_node_mut()
                .unwrap()
                .play_animation("unselect");
        });

    let next = if is_down {
        button.below()
    } else {
        button.above()
    };

    let (entity, _, _) = main_menu_buttons
        .iter()
        .find(|(_, button, _)| **button == next)
        .unwrap();

    commands
        .entity(entity)
        .insert(FocusedMenuButton)
        .add(|entity: EntityWorldMut| {
            let mut node = NodeWorldViewMut::new(entity).unwrap();
            node.as_layout_node_mut().unwrap().play_animation("select");
        });
}

#[derive(Deserialize, Serialize)]
pub struct ReplaceImage {
    id: String,
    path: PathBuf,
}

#[derive(Deserialize, Serialize)]
pub struct ReplaceText {
    id: String,
    text: String,
}

impl LayoutAttribute for ReplaceImage {
    fn apply(&self, world: &mut yabuil::views::NodeWorldViewMut) {
        world.child_scope(self.id.as_str(), |node| {
            let Some(mut node) = node else {
                log::warn!("No child by the name of {}", self.id);
                return;
            };

            let handle = node
                .world()
                .resource::<AssetServer>()
                .load_with_settings::<_, ImageLoaderSettings>(self.path.clone(), |settings| {
                    settings.sampler = ImageSampler::nearest()
                });

            node.as_image_node_mut().unwrap().set_image(handle);
        });
    }
}

impl LayoutAttribute for ReplaceText {
    fn apply(&self, world: &mut yabuil::views::NodeWorldViewMut) {
        world.child_scope(self.id.as_str(), |node| {
            let Some(mut node) = node else {
                log::warn!("No child by the name of {}", self.id);
                return;
            };

            node.as_text_node_mut().unwrap().set_text(self.text.clone());
        });
    }
}

#[derive(Deserialize, Serialize)]
pub struct NearestNeighbor {}

#[derive(Deserialize, Serialize)]
pub struct ImageTint {
    color: [f32; 4],
}

impl LayoutAttribute for NearestNeighbor {
    fn apply(&self, world: &mut yabuil::views::NodeWorldViewMut) {
        let handle = world.as_image_node().unwrap().image();

        world.as_entity_world_mut().world_scope(|world| {
            world
                .resource_mut::<Assets<Image>>()
                .get_mut(handle.id())
                .unwrap()
                .sampler = ImageSampler::nearest();
        });
    }
}

impl LayoutAttribute for ImageTint {
    fn apply(&self, world: &mut yabuil::views::NodeWorldViewMut) {
        world.as_image_node_mut().unwrap().update_sprite(|sprite| {
            sprite.color = Color::rgba(self.color[0], self.color[1], self.color[2], self.color[3]);
        });
    }
}

fn spawn_layout(mut commands: Commands, assets: Res<AssetServer>) {
    commands
        .spawn((
            Camera2dBundle {
                camera_2d: Camera2d {
                    clear_color: ClearColorConfig::Custom(Color::rgb_u8(178, 168, 213)),
                },
                ..default()
            },
            VisibilityBundle::default(),
        ))
        .with_children(|children| {
            children.spawn((
                ActiveLayout,
                LayoutBundle::new(assets.load("layouts/rivals_main_menu1.layout.json")),
            ));
        });

    commands.spawn(AudioBundle {
        source: assets.load("audio/rivals_main_menu.ogg"),
        settings: PlaybackSettings {
            mode: bevy::audio::PlaybackMode::Loop,
            ..default()
        },
    });
}

#[derive(Resource)]
pub struct MenuMoveSfx(Handle<AudioSource>);

impl FromWorld for MenuMoveSfx {
    fn from_world(world: &mut World) -> Self {
        Self(
            world
                .resource::<AssetServer>()
                .load("audio/menu_move_sfx.ogg"),
        )
    }
}

pub fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            yabuil::LayoutPlugin::default(),
            // WorldInspectorPlugin::default(),
        ))
        .add_systems(Startup, spawn_layout)
        .add_systems(Update, update_menu_buttons)
        .register_layout_attribute::<NearestNeighbor>("NearestNeighbor")
        .register_layout_attribute::<ImageTint>("ImageTint")
        .register_layout_attribute::<ReplaceImage>("ReplaceImage")
        .register_layout_attribute::<ReplaceText>("ReplaceText")
        .register_layout_attribute::<MainMenuButton>("MainMenuButton")
        .init_resource::<MenuMoveSfx>()
        .run();
}
