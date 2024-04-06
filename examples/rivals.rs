use std::path::PathBuf;

use bevy::{
    prelude::*,
    render::texture::{ImageLoaderSettings, ImageSampler},
};
use serde::{Deserialize, Serialize};
use yabuil::{
    views::NodeEntityMut, ActiveLayout, LayoutApp, LayoutAttribute, LayoutBundle, LayoutPlugin,
};

#[derive(Deserialize, Serialize, Component, Copy, Clone, PartialEq, Eq, Reflect)]
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
    const NAME: &'static str = "MainMenuButton";

    fn apply(&self, mut world: NodeEntityMut) {
        world.insert(*self);
        if matches!(self, MainMenuButton::LocalPlay) {
            world
                .insert(FocusedMenuButton)
                .layout()
                .play_animation("select")
                .unwrap();
        }
    }
}

#[derive(Component)]
pub struct FocusedMenuButton;

fn update_menu_buttons(
    mut commands: Commands,
    menu_move_sfx: Res<MenuMoveSfx>,
    gamepads: Res<Gamepads>,
    buttons: Res<ButtonInput<GamepadButton>>,
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
            NodeEntityMut::from_entity_world_mut(entity)
                .layout()
                .play_animation("unselect")
                .unwrap();
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
            NodeEntityMut::from_entity_world_mut(entity)
                .layout()
                .play_animation("select")
                .unwrap();
        });
}

#[derive(Deserialize, Serialize, Reflect)]
pub struct ReplaceImage {
    id: String,
    path: PathBuf,
}

#[derive(Deserialize, Serialize, Reflect)]
pub struct ReplaceText {
    id: String,
    text: String,
}

impl LayoutAttribute for ReplaceImage {
    const NAME: &'static str = "ReplaceImage";
    fn apply(&self, mut world: NodeEntityMut) {
        let mut child = world.child(self.id.as_str());

        let handle = child
            .world()
            .resource::<AssetServer>()
            .load_with_settings::<_, ImageLoaderSettings>(self.path.clone(), |settings| {
                settings.sampler = ImageSampler::nearest();
            });

        child.image().set_image(handle);
    }
}

impl LayoutAttribute for ReplaceText {
    const NAME: &'static str = "ReplaceText";

    fn apply(&self, mut world: NodeEntityMut) {
        world
            .child(self.id.as_str())
            .text()
            .set_text(self.text.clone());
    }
}

#[derive(Deserialize, Serialize, Reflect)]
pub struct NearestNeighbor {}

impl LayoutAttribute for NearestNeighbor {
    const NAME: &'static str = "NearestNeighbor";
    fn apply(&self, mut world: NodeEntityMut) {
        let handle = world.image().image().id();

        world
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .get_mut(handle)
            .unwrap()
            .sampler = ImageSampler::nearest();
    }
}

fn spawn_layout(mut commands: Commands, assets: Res<AssetServer>) {
    commands
        .spawn((
            Camera2dBundle {
                camera: Camera {
                    clear_color: ClearColorConfig::Custom(Color::rgb_u8(178, 168, 213)),
                    ..default()
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
        .add_plugins((DefaultPlugins, LayoutPlugin::default()))
        .add_systems(Startup, spawn_layout)
        .add_systems(Update, update_menu_buttons)
        .register_layout_attribute::<NearestNeighbor>()
        .register_layout_attribute::<ReplaceImage>()
        .register_layout_attribute::<ReplaceText>()
        .register_layout_attribute::<MainMenuButton>()
        .init_resource::<MenuMoveSfx>()
        .run();
}
