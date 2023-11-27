use std::{hint::unreachable_unchecked, sync::Arc};

use bevy::{math::Vec2, prelude::*, utils::HashMap};
use serde::Deserialize;

use crate::{components::LayoutNodeMetadata, views::NodeWorldViewMut};

#[derive(Deserialize, Clone)]
pub enum NodeAnimationTarget {
    Position { start: Vec2, end: Vec2 },
    Size { start: Vec2, end: Vec2 },
    ImageColor { start: [f32; 4], end: [f32; 4] },
    TextColor { start: [f32; 4], end: [f32; 4] },
}

#[derive(Deserialize, Clone)]
pub struct NodeAnimation {
    id: String,
    time_ms: f32,
    target: NodeAnimationTarget,
}

#[derive(Clone, Component, Default)]
pub struct Animations(pub(crate) Arc<HashMap<String, Vec<NodeAnimation>>>);

impl<'de> Deserialize<'de> for Animations {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        <HashMap<String, Vec<NodeAnimation>>>::deserialize(deserializer).map(|m| Self(Arc::new(m)))
    }
}

#[derive(Component)]
pub(crate) enum AnimationPlayerState {
    NotPlaying,
    Playing {
        animation: String,
        time_elapsed_ms: f32,
    },
}

impl AnimationPlayerState {
    fn is_playing(&self) -> bool {
        matches!(self, Self::Playing { .. })
    }
}

pub(crate) fn update_ui_layout_animations(
    commands: ParallelCommands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut AnimationPlayerState, &Animations, &Children)>,
    metadata: Query<&mut LayoutNodeMetadata>,
) {
    let delta = time.delta_seconds() * 1000.0;
    query
        .par_iter_mut()
        .for_each(|(entity, mut state, anims, children)| {
            if !state.is_playing() {
                return;
            }

            let AnimationPlayerState::Playing {
                animation,
                time_elapsed_ms,
            } = &mut *state
            else {
                // SAFETY: we ensure above
                unsafe { unreachable_unchecked() }
            };

            let mut is_finished = true;
            *time_elapsed_ms += delta;
            let progress = *time_elapsed_ms;

            if let Some(animation) = anims.0.get(animation.as_str()) {
                'outer: for animation in animation.iter() {
                    for child in children.iter().copied() {
                        // SAFETY: this system will ensure exclusive acess to the components,
                        // and we are only calling these on this node's children, where a
                        // child entity can only be the child of one entity
                        let mut metadata = unsafe { metadata.get_unchecked(child).unwrap() };
                        if metadata.id().name() == animation.id.as_str() {
                            is_finished &= progress >= animation.time_ms;
                            let interp = (progress / animation.time_ms).clamp(0.0, 1.0);
                            match &animation.target {
                                NodeAnimationTarget::Position { start, end } => {
                                    metadata.position = *start * (1.0 - interp) + *end * interp;
                                }
                                NodeAnimationTarget::Size { start, end } => {
                                    metadata.size = *start * (1.0 - interp) + *end * interp;
                                }
                                NodeAnimationTarget::ImageColor { start, end } => {
                                    let start_color =
                                        Color::rgb(start[0], start[1], start[2]).as_hsla();
                                    let end_color = Color::rgb(end[0], end[1], end[2]).as_hsla();

                                    let mut color =
                                        start_color * (1.0 - interp) + end_color * interp;

                                    color.set_a(start[3] * (1.0 - interp) + end[3] * interp);

                                    commands.command_scope(move |mut commands| {
                                        commands.entity(child).add(
                                            move |entity: EntityWorldMut| {
                                                let mut node =
                                                    NodeWorldViewMut::new(entity).unwrap();
                                                node.as_image_node_mut()
                                                    .unwrap()
                                                    .update_sprite(|sprite| sprite.color = color);
                                            },
                                        );
                                    });
                                }
                                NodeAnimationTarget::TextColor { start, end } => {
                                    let start_color =
                                        Color::rgb(start[0], start[1], start[2]).as_hsla();
                                    let end_color = Color::rgb(end[0], end[1], end[2]).as_hsla();

                                    let mut color =
                                        start_color * (1.0 - interp) + end_color * interp;

                                    color.set_a(start[3] * (1.0 - interp) + end[3] * interp);
                                    commands.command_scope(move |mut commands| {
                                        commands.entity(child).add(
                                            move |entity: EntityWorldMut| {
                                                let mut node =
                                                    NodeWorldViewMut::new(entity).unwrap();
                                                node.as_text_node_mut().unwrap().set_color(color);
                                            },
                                        );
                                    });
                                }
                            }
                            continue 'outer;
                        }
                    }

                    log::warn!("Could not find node '{}' for animation", animation.id);
                }
            } else {
                log::warn!("Failed to get animation {} for node", animation);
            }

            if is_finished {
                *state = AnimationPlayerState::NotPlaying;
            }
        });
}
