use bevy::{
    core_pipeline::{
        bloom::{BloomCompositeMode, BloomPrefilterSettings, BloomSettings},
        tonemapping::Tonemapping,
    },
    prelude::*,
};
use serde::{Deserialize, Serialize};
use yabuil::{
    asset::Layout,
    input_detection::{LayoutCursorPosition, LayoutCursors, LayoutNodeInputDetection},
    node::ComputedBoundingBox,
    views::NodeEntityMut,
    ActiveLayout, LayoutApp, LayoutAttribute, LayoutBundle, LayoutPlugin,
};

#[derive(Debug, Serialize, Deserialize, Component, Reflect)]
pub struct ControllerCursor {}

impl LayoutAttribute for ControllerCursor {
    const NAME: &'static str = "ControllerCursor";

    fn apply(&self, mut world: NodeEntityMut) {
        world.insert((
            ControllerCursor {},
            LayoutCursorPosition {
                position: Vec2::INFINITY,
                left_click: false,
                right_click: false,
                middle_click: false,
            },
            ComputedBoundingBox::default(),
        ));

        let id = world.id();

        for sibling in [
            "local_play_button",
            "online_play_button",
            "settings_button",
            "exit_button",
        ] {
            world
                .sibling(sibling)
                .child("button_hit")
                .get_mut::<LayoutCursors>()
                .unwrap()
                .push(yabuil::input_detection::Cursor::Custom(id));
        }
    }
}

fn update_controller_cursor_node(
    gamepads: Res<Gamepads>,
    axes: Res<Axis<GamepadAxis>>,
    mut nodes: Query<
        (
            &mut LayoutCursorPosition,
            &mut yabuil::node::Node,
            &ComputedBoundingBox,
        ),
        With<ControllerCursor>,
    >,
) {
    let Ok((mut cursor, mut node, bbox)) = nodes.get_single_mut() else {
        return;
    };

    let mut direction = Vec2::ZERO;

    for gamepad in gamepads.iter() {
        direction.x += axes
            .get(GamepadAxis::new(gamepad, GamepadAxisType::LeftStickX))
            .unwrap_or_default();
        direction.y += axes
            .get(GamepadAxis::new(gamepad, GamepadAxisType::LeftStickY))
            .unwrap_or_default();
    }

    direction.y *= -1.0;

    node.position += direction * 5.0;
    cursor.position = bbox.center() + direction * 5.0;
}

#[derive(Debug, Serialize, Deserialize, Component, Copy, Clone, Reflect)]
pub enum MainMenuButton {
    LocalPlay,
    OnlinePlay,
    Settings,
    Exit,
}

impl LayoutAttribute for MainMenuButton {
    const NAME: &'static str = "MainMenuButton";

    fn apply(&self, mut world: NodeEntityMut) {
        let name = match self {
            Self::LocalPlay => "LOCAL PLAY",
            Self::OnlinePlay => "ONLINE PLAY",
            Self::Settings => "SETTINGS",
            Self::Exit => "QUIT TO DESKTOP",
        };

        world
            .child("button_content/button_text")
            .text()
            .set_text(name);
        let mut button_hit = world.child("button_hit");
        let mut input_det = button_hit.get_mut::<LayoutNodeInputDetection>().unwrap();
        input_det.on_global_hover(|_, node| {
            animate_menu_button(node, true);
        });
        input_det.on_global_unhover(|_, node| {
            animate_menu_button(node, false);
        });
        world.insert(*self);
    }
}

fn animate_menu_button(mut button: NodeEntityMut, on: bool) {
    if on {
        button.parent().layout().play_animation("select").unwrap();
    } else {
        button
            .parent()
            .layout()
            .play_or_reverse_animation("select")
            .unwrap();
    }
}

fn startup_system(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands
        .spawn((
            Camera2dBundle {
                transform: Transform {
                    translation: Vec3 {
                        x: 0.,
                        y: 0.,
                        z: 1000.,
                    },
                    ..default()
                },
                camera: Camera {
                    hdr: true,
                    ..default()
                },
                tonemapping: Tonemapping::None,
                ..default()
            },
            BloomSettings {
                intensity: 0.20,
                low_frequency_boost: 0.8,
                low_frequency_boost_curvature: 0.95,
                high_pass_frequency: 0.9,
                prefilter_settings: BloomPrefilterSettings {
                    threshold: 0.25,
                    threshold_softness: 0.1,
                },
                composite_mode: BloomCompositeMode::Additive,
            },
            VisibilityBundle::default(),
        ))
        .with_children(|children| {
            children.spawn((
                LayoutBundle::new(asset_server.load::<Layout>("layouts/main_menu.layout.json")),
                ActiveLayout,
            ));
        });
}

pub fn main() {
    App::new()
        .add_plugins((DefaultPlugins, LayoutPlugin::default()))
        .register_type::<MainMenuButton>()
        .register_type::<ControllerCursor>()
        .register_layout_attribute::<MainMenuButton>()
        .register_layout_attribute::<ControllerCursor>()
        .add_systems(Startup, startup_system)
        .add_systems(Update, update_controller_cursor_node)
        .add_systems(Update, bevy::window::close_on_esc)
        .run();
}
