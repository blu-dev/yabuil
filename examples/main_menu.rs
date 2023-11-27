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
    views::NodeWorldViewMut,
    ActiveLayout, ComputedLayoutNodeMetadata, LayoutApp, LayoutAttribute, LayoutBundle,
    LayoutNodeMetadata, LayoutPlugin,
};

#[derive(Component)]
pub struct AnimateMenuButton {
    starting_image_color: Color,
    target_image_color: Color,
    starting_text_color: Color,
    target_text_color: Color,
    time: f32,
    progress: f32,
}

pub fn animate_menu_button_system(
    time: Res<Time>,
    mut commands: Commands,
    mut buttons: Query<(Entity, &mut AnimateMenuButton), With<MainMenuButton>>,
) {
    let delta = time.delta_seconds() * 1000.0;
    for (entity, mut state) in buttons.iter_mut() {
        state.progress += delta;

        let (image_color, text_color) = if state.progress >= state.time {
            commands.entity(entity).remove::<AnimateMenuButton>();
            (state.target_image_color, state.target_text_color)
        } else {
            let interp = state.progress / state.time;
            (
                state.starting_image_color * (1.0f32 - interp) + state.target_image_color * interp,
                state.starting_text_color * (1.0f32 - interp) + state.target_text_color * interp,
            )
        };

        commands.entity(entity).add(move |entity: EntityWorldMut| {
            let mut node = NodeWorldViewMut::new(entity).unwrap();
            node.child_scope("button_image", |node| {
                let mut node = node.unwrap();
                node.as_image_node_mut().unwrap().update_sprite(|sprite| {
                    sprite.color = image_color;
                });
            });

            node.child_scope("button_text", |node| {
                let mut node = node.unwrap();
                node.as_text_node_mut().unwrap().set_color(text_color);
            });
        });
    }
}

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
        ));

        let id = world.as_entity().id();

        for sibling in [
            "local_play_button",
            "online_play_button",
            "settings_button",
            "exit_button",
        ] {
            world.sibling_scope(sibling, |node| {
                println!("{sibling}");
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
            &mut LayoutNodeMetadata,
            &ComputedLayoutNodeMetadata,
            &mut LayoutCursorPosition,
            &GlobalTransform,
        ),
        With<ControllerCursor>,
    >,
) {
    let Ok((mut node, computed, mut cursor, trans)) = nodes.get_single_mut() else {
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
    cursor.position =
        (computed.bounding_box().center() + direction * 5.0) * trans.compute_transform().scale.xy();
}

#[derive(Debug, Serialize, Deserialize, Component, Copy, Clone)]
pub enum MainMenuButton {
    LocalPlay,
    OnlinePlay,
    Settings,
    Exit,
}

fn animate_menu_button(button: &mut NodeWorldViewMut, on: bool) {
    let (start, target) = if on {
        (Color::GRAY.as_hsla(), Color::WHITE.as_hsla())
    } else {
        (Color::WHITE.as_hsla(), Color::GRAY.as_hsla())
    };

    let animation = if on { "select" } else { "unselect" };

    button.parent_scope(|parent| {
        let mut parent = parent.unwrap();
        parent
            .as_layout_node_mut()
            .unwrap()
            .play_animation(animation);
        let parent = parent.as_entity_world_mut();
        let progress = if let Some(state) = parent.get::<AnimateMenuButton>() {
            100.0 - state.progress
        } else {
            0.0
        };

        parent.insert(AnimateMenuButton {
            starting_image_color: start,
            target_image_color: target,
            starting_text_color: start,
            target_text_color: target,
            time: 100.0,
            progress,
        });
    });
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
        .add_systems(
            Update,
            (animate_menu_button_system, update_controller_cursor_node),
        )
        .add_systems(Update, bevy::window::close_on_esc)
        .run();
}
