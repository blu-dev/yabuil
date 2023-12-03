use std::{hint::unreachable_unchecked, path::Path, sync::Arc};

use bevy::{ecs::query::WorldQuery, prelude::*, utils::HashMap};

use crate::{views::NodeViewMut, LayoutAnimationTarget, LayoutNodeId};
use serde::{Deserialize, Serialize};

#[derive(Default, Deserialize, Serialize)]
pub(crate) enum TimeBezierCurve {
    #[default]
    Linear,
    Quadratic(f32),
    Cubic(f32, f32),
}

impl TimeBezierCurve {
    pub fn map(&self, current: f32) -> f32 {
        match self {
            Self::Linear => current,
            Self::Quadratic(quad) => {
                0.0 + 2.0 * (1.0 - current) * current * *quad + current.powi(2)
            }
            Self::Cubic(a, b) => {
                0.0 + 3.0 * (1.0 - current).powi(2) * current * *a
                    + (1.0 - current) * current.powi(2) * *b
                    + current.powi(3)
            }
        }
    }
}

pub struct NodeAnimation {
    pub(crate) id: String,
    pub(crate) time_ms: f32,
    pub(crate) time_scale: TimeBezierCurve,
    pub(crate) target: Box<dyn LayoutAnimationTarget>,
}

#[derive(Clone, Component, Default)]
pub struct Animations(pub(crate) Arc<HashMap<String, Vec<NodeAnimation>>>);

#[derive(Component, Default)]
pub(crate) enum AnimationPlayerState {
    #[default]
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

#[derive(WorldQuery)]
#[world_query(mutable)]
struct SomeQuery {
    item: &'static Children,
    other_item: &'static mut AnimationPlayerState,
}

pub(crate) fn update_ui_layout_animations(
    mut disjoint: ParamSet<(Res<Time>, Query<EntityMut<'_>, With<crate::node::Node>>)>,
) {
    let delta = disjoint.p0().delta_seconds() * 1000.0;
    let query = disjoint.p1();
    // SAFETY: This is safe because we are still only accessing one entity at a time
    // (see safety comments below)
    for mut entity in unsafe { query.iter_unsafe() } {
        let Some(mut state) = entity.get_mut::<AnimationPlayerState>() else {
            continue;
        };

        if !state.is_playing() {
            continue;
        }

        let mut state = std::mem::take(&mut *state);

        let AnimationPlayerState::Playing {
            animation,
            time_elapsed_ms,
        } = &mut state
        else {
            // SAFETY: we ensure above
            unsafe { unreachable_unchecked() }
        };

        let mut is_finished = true;
        *time_elapsed_ms += delta;
        let progress = *time_elapsed_ms;

        let anims = entity.get::<Animations>().unwrap().clone();

        if let Some(animation) = anims.0.get(animation.as_str()) {
            'outer: for animation in animation.iter() {
                let mut entity = entity.reborrow();
                'id_search: for id in Path::new(animation.id.as_str()).components() {
                    let id = id.as_os_str().to_str().unwrap();
                    let children = entity.get::<Children>().unwrap();
                    for child in children.iter().copied() {
                        // SAFETY: This is safe because we are iterating over the components serially
                        //          and therefore we won't be holding a reference to any of the children
                        let child = unsafe { query.get_unchecked(child).unwrap() };

                        let node_id = child.get::<LayoutNodeId>().unwrap();
                        if node_id.name() == id {
                            entity = child;
                            continue 'id_search;
                        }
                    }
                    log::warn!("Could not find node '{}' for animation", animation.id);
                    continue 'outer;
                }
                is_finished &= progress >= animation.time_ms;
                let interp = (progress / animation.time_ms).clamp(0.0, 1.0);

                let mut node_view = NodeViewMut::new(entity).unwrap();

                animation
                    .target
                    .interpolate(&mut node_view, animation.time_scale.map(interp));
            }
        } else {
            log::warn!("Failed to get animation {} for node", animation);
        }

        if is_finished {
            state = AnimationPlayerState::NotPlaying;
        }

        *entity.get_mut::<AnimationPlayerState>().unwrap() = state;
    }
}

#[derive(Serialize, Deserialize)]
pub struct PositionAnimation {
    pub start: Vec2,
    pub end: Vec2,
}

#[derive(Serialize, Deserialize)]
pub struct SizeAnimation {
    pub start: Vec2,
    pub end: Vec2,
}

#[derive(Serialize, Deserialize)]
struct ColorAnimation {
    start: [f32; 4],
    end: [f32; 4],
}

#[derive(Serialize, Deserialize)]
pub struct ImageColorAnimation(ColorAnimation);

#[derive(Serialize, Deserialize)]
pub struct TextColorAnimation(ColorAnimation);

impl LayoutAnimationTarget for PositionAnimation {
    fn interpolate(&self, node: &mut NodeViewMut, interpolation: f32) {
        node.node_mut().position = self.start * (1.0 - interpolation) + self.end * interpolation;
    }
}

impl LayoutAnimationTarget for SizeAnimation {
    fn interpolate(&self, node: &mut NodeViewMut, interpolation: f32) {
        node.node_mut().size = self.start * (1.0 - interpolation) + self.end * interpolation;
    }
}

impl ColorAnimation {
    fn interpolate(&self, interp: f32) -> Color {
        let [r1, g1, b1, a1] = self.start;
        let [r2, g2, b2, a2] = self.end;
        let start_color = Color::rgb(r1, g1, b1).as_hsla();
        let end_color = Color::rgb(r2, g2, b2).as_hsla();

        let mut color = start_color * (1.0 - interp) + end_color * interp;

        color.set_a(a1 * (1.0 - interp) + a2 * interp);
        color
    }
}

impl LayoutAnimationTarget for ImageColorAnimation {
    fn interpolate(&self, node: &mut NodeViewMut, interpolation: f32) {
        node.as_image_node_mut().unwrap().update_sprite(|style| {
            style.color = self.0.interpolate(interpolation);
        })
    }
}

impl LayoutAnimationTarget for TextColorAnimation {
    fn interpolate(&self, node: &mut NodeViewMut, interpolation: f32) {
        node.as_text_node_mut()
            .unwrap()
            .set_color(self.0.interpolate(interpolation));
    }
}
