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
    views::NodeWorldViewMut,
    ActiveLayout, LayoutApp, LayoutAttribute, LayoutBundle, LayoutPlugin,
};

#[derive(Debug, Serialize, Deserialize, Component)]
pub struct ControllerCursor {}

impl LayoutAttribute for ControllerCursor {
    fn apply(&self, world: &mut NodeWorldViewMut) {
        world.as_entity_world_mut().insert((
            ControllerCursor {},
            LayoutCursorPosition {
                position: Vec2::INFINITY,
                left_click: false,
                right_click: false,
                middle_click: false,
            },
            ComputedBoundingBox::default(),
        ));

        let id = world.as_entity().id();

        for sibling in [
            "local_play_button",
            "online_play_button",
            "settings_button",
            "exit_button",
        ] {
            world.sibling_scope(sibling, |node| {
                let mut node = node.unwrap();
                node.child_scope("button_hit", |node| {
                    let mut node = node.unwrap();
                    node.as_entity_mut()
                        .get_mut::<LayoutCursors>()
                        .unwrap()
                        .push(yabuil::input_detection::Cursor::Custom(id));
                });
            });
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

#[derive(Debug, Serialize, Deserialize, Component, Copy, Clone)]
pub enum MainMenuButton {
    LocalPlay,
    OnlinePlay,
    Settings,
    Exit,
}

impl LayoutAttribute for MainMenuButton {
    fn apply(&self, world: &mut NodeWorldViewMut) {
        let name = match self {
            Self::LocalPlay => "LOCAL PLAY",
            Self::OnlinePlay => "ONLINE PLAY",
            Self::Settings => "SETTINGS",
            Self::Exit => "QUIT TO DESKTOP",
        };

        world.child_scope("button_text", |text| {
            let mut text = text.unwrap();
            text.as_text_node_mut().unwrap().set_text(name);
        });

        world.child_scope("button_hit", |hit| {
            let mut hit = hit.unwrap();
            let mut hit = hit.as_entity_mut();
            let mut input_det = hit.get_mut::<LayoutNodeInputDetection>().unwrap();
            input_det.on_global_hover(|_, node| {
                animate_menu_button(node, true);
            });
            input_det.on_global_unhover(|_, node| {
                animate_menu_button(node, false);
            });
        });

        world.as_entity_world_mut().insert(*self);
    }
}

fn animate_menu_button(button: &mut NodeWorldViewMut, on: bool) {
    let animation = if on { "select" } else { "unselect" };

    button.parent_scope(|parent| {
        let mut parent = parent.unwrap();
        parent
            .as_layout_node_mut()
            .unwrap()
            .play_animation(animation);
    });
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
        .add_plugins((DefaultPlugins, LayoutPlugin))
        .register_layout_attribute::<MainMenuButton>("MainMenuButton")
        .register_layout_attribute::<ControllerCursor>("ControllerCursor")
        .add_systems(Startup, startup_system)
        .add_systems(Update, update_controller_cursor_node)
        .add_systems(Update, bevy::window::close_on_esc)
        .run();
}
