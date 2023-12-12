use std::any::TypeId;

use bevy::{prelude::*, utils::hashbrown::HashMap, ecs::world::unsafe_world_cell::UnsafeWorldCell};
use camino::{Utf8Path, Utf8PathBuf};

use crate::{node::LayoutHandle, LayoutNodeId, views::NodeMut};

use serde::{Deserialize, Serialize};

pub(crate) struct StaticTypeInfo {
    pub name: &'static str,
    pub type_path: &'static str,
    pub type_id: TypeId,
}

#[derive(Default, Deserialize, Serialize, Copy, Clone)]
pub enum TimeBezierCurve {
    #[default]
    Linear,
    Quadratic(Vec2),
    Cubic(Vec2, Vec2),
}

impl TimeBezierCurve {
    pub fn map(&self, current: f32) -> f32 {
        let point = match self {
            Self::Linear => Vec2::new(0.0, current),
            Self::Quadratic(quad) => {
                Vec2::ZERO + 2.0 * (1.0 - current) * current * *quad + Vec2::ONE * current.powi(2)
            }
            Self::Cubic(a, b) => {
                Vec2::ZERO
                    + 3.0 * (1.0 - current).powi(2) * current * *a
                    + (1.0 - current) * current.powi(2) * *b
                    + Vec2::ONE * current.powi(3)
            }
        };

        point.y
    }
}

pub struct DynamicAnimationTarget {
    type_info: StaticTypeInfo,
    data: *mut (),
    // SAFETY: The caller must ensure that the type of data being passed into BOTH parameters
    //          is the same type that created this animation node.
    interpolate: unsafe fn(*const (), Option<*const ()>, NodeMut, ResourceRestrictedWorld, f32),
}

unsafe impl Send for DynamicAnimationTarget {}
unsafe impl Sync for DynamicAnimationTarget {}

impl DynamicAnimationTarget {
    pub(crate) fn new<T: LayoutAnimationTarget>(data: T) -> Self {
        Self {
            type_info: StaticTypeInfo {
                name: T::NAME,
                type_path: T::short_type_path(),
                type_id: TypeId::of::<T>(),
            },
            data: (Box::leak(Box::new(data)) as *mut T).cast::<()>(),
            // We cannot create an unsafe closure, but this is good enough for our purposes since
            // we are coallescing it into an unsafe function pointer
            interpolate: |current, prev, node, world, progress| unsafe {
                let current = &*current.cast::<T>();
                let prev = prev.map(|prev| &*prev.cast::<T>());
                current.interpolate(prev, node, world, progress);
            },
        }
    }

    pub fn name(&self) -> &str {
        self.type_info.name.as_ref()
    }

    pub fn target_type_path(&self) -> &str {
        self.type_info.type_path
    }

    pub fn target_type_id(&self) -> TypeId {
        self.type_info.type_id
    }

    pub fn is_type<T: 'static>(&self) -> bool {
        self.type_info.type_id == TypeId::of::<T>()
    }

    pub fn interpolate_from_start(&self, node: NodeMut, world: ResourceRestrictedWorld, progress: f32) {
        // SAFETY: we are providing the owned pointer that we created ont ype construction, it is
        // going to be the same type
        unsafe { (self.interpolate)(self.data, None, node, world, progress) }
    }

    pub fn interpolate_with_previous(
        &self,
        previous: &DynamicAnimationTarget,
        node: NodeMut,
        world: ResourceRestrictedWorld,
        progress: f32,
    ) {
        #[inline(never)]
        #[cold]
        fn panic_wrong_type(got: &'static str, expected: &'static str) {
            panic!("Attempting to interpolate incorrect type. Expected type {expected}, got type {got}");
        }

        if self.type_info.type_id != previous.type_info.type_id {
            panic_wrong_type(self.type_info.type_path, self.type_info.type_path);
        }

        // SAFETY: we have ensured that the type T is the same type that is used to represent
        // this target
        unsafe {
            (self.interpolate)(self.data, Some(previous.data), node, world, progress);
        }
    }
}

pub struct RawKeyframe {
    pub timestamp_ms: usize,
    pub time_scale: TimeBezierCurve,
    pub targets: Vec<DynamicAnimationTarget>,
}

pub struct Keyframe {
    pub timestamp_ms: usize,
    pub time_scale: TimeBezierCurve,
    pub target: DynamicAnimationTarget,
}

pub struct KeyframeChannel {
    pub type_id: TypeId,
    pub keyframes: Vec<Keyframe>,
}

pub struct Keyframes {
    max_length: usize,
    channels: Vec<KeyframeChannel>,
}

impl Keyframes {
    /// Flattens a list of keyframes into individual channels based off of their type id
    ///
    /// This can be used to more efficiently animate each target during the animation systems
    pub(crate) fn flatten_raw_keyframes(keyframes: Vec<RawKeyframe>) -> Self {
        let mut map_of_targets: HashMap<TypeId, Vec<Keyframe>> = HashMap::new();
        for keyframe in keyframes {
            for target in keyframe.targets {
                map_of_targets
                    .entry(target.target_type_id())
                    .or_default()
                    .push(Keyframe {
                        timestamp_ms: keyframe.timestamp_ms,
                        time_scale: keyframe.time_scale,
                        target,
                    });
            }
        }

        let channels: Vec<_> = map_of_targets
            .into_iter()
            .map(|(type_id, mut list)| {
                list.sort_by_key(|kf| kf.timestamp_ms);
                KeyframeChannel {
                    type_id,
                    keyframes: list,
                }
            })
            .collect();

        let max_length = channels
            .iter()
            .map(|channel| {
                channel
                    .keyframes
                    .last()
                    .expect("should be at least one keyframe")
                    .timestamp_ms
            })
            .max()
            .unwrap_or_default();

        Self {
            max_length,
            channels,
        }
    }
}

#[derive(Default)]
pub(crate) struct RawLayoutAnimations(
    pub(crate) HashMap<String, HashMap<String, Vec<RawKeyframe>>>,
);

/// An asset type for a layout animation
///
/// Layout animations are loaded as labeled assets on an animation.
#[derive(Asset, Deref, DerefMut, TypePath)]
pub struct LayoutAnimation(pub(crate) HashMap<Utf8PathBuf, Keyframes>);

pub struct ResourceRestrictedWorld<'w>(UnsafeWorldCell<'w>);

impl ResourceRestrictedWorld<'_> {
    #[track_caller]
    pub fn resource<R: Resource>(&self) -> &R {
        self.get_resource::<R>().unwrap()
    }

    pub fn get_resource<R: Resource>(&self) -> Option<&R> {
        unsafe {
            self.0.get_resource::<R>()
        }
    }

    #[track_caller]
    pub fn resource_mut<R: Resource>(&mut self) -> Mut<'_, R> {
        self.get_resource_mut::<R>().unwrap()
    }

    pub fn get_resource_mut<R: Resource>(&mut self) -> Option<Mut<'_, R>> {
        unsafe {
            self.0.get_resource_mut::<R>()
        }
    }
}

pub trait LayoutAnimationTarget: TypePath + Send + Sync + 'static {
    const NAME: &'static str;

    fn interpolate(&self, previous: Option<&Self>, node: NodeMut, world: ResourceRestrictedWorld<'_>, progress: f32);
}

#[derive(Debug)]
enum InternalPlaybackState {
    Stopped,
    Paused { progress: usize, is_reverse: bool },
    Playing { progress: usize, is_reverse: bool },
}

pub enum PlaybackState {
    Stopped,
    Paused,
    Playing,
}

impl PlaybackState {
    fn from_internal(internal: &InternalPlaybackState) -> Self {
        match internal {
            InternalPlaybackState::Stopped => Self::Stopped,
            InternalPlaybackState::Paused { .. } => Self::Paused,
            InternalPlaybackState::Playing { .. } => Self::Playing,
        }
    }
}

#[derive(Component, Default)]
pub struct LayoutAnimationPlaybackState(HashMap<String, InternalPlaybackState>);

impl LayoutAnimationPlaybackState {
    pub(crate) fn new(
        asset_server: &AssetServer,
        handles: impl Iterator<Item = AssetId<LayoutAnimation>>,
    ) -> Self {
        let mut map = HashMap::new();
        for handle in handles {
            let path = asset_server
                .get_path(handle)
                .expect("handle for id should be present when building playback state");
            let label = path.label().expect("handle should have a label");
            map.insert(label.to_string(), InternalPlaybackState::Stopped);
        }

        Self(map)
    }

    pub fn is_playing_any(&self) -> bool {
        self.0
            .values()
            .any(|state| matches!(state, InternalPlaybackState::Playing { .. }))
    }

    pub fn playback_state(&self, name: &str) -> Option<PlaybackState> {
        self.0.get(name).map(PlaybackState::from_internal)
    }

    pub fn play_animation(&mut self, name: &str) -> bool {
        if let Some(state) = self.0.get_mut(name) {
            *state = InternalPlaybackState::Playing {
                progress: 0,
                is_reverse: false,
            };
            true
        } else {
            false
        }
    }

    pub fn stop_animation(&mut self, name: &str) -> bool {
        if let Some(state) = self.0.get_mut(name) {
            *state = InternalPlaybackState::Stopped;
            true
        } else {
            false
        }
    }

    pub fn pause_animation(&mut self, name: &str) -> bool {
        if let Some(state) = self.0.get_mut(name) {
            match state {
                InternalPlaybackState::Playing {
                    progress,
                    is_reverse,
                } => {
                    *state = InternalPlaybackState::Paused {
                        progress: *progress,
                        is_reverse: *is_reverse,
                    }
                }
                _ => {}
            }

            true
        } else {
            false
        }
    }

    pub fn pause_all_animations(&mut self) {
        for state in self.0.values_mut() {
            match state {
                InternalPlaybackState::Playing {
                    progress,
                    is_reverse,
                } => {
                    *state = InternalPlaybackState::Paused {
                        progress: *progress,
                        is_reverse: *is_reverse,
                    }
                }
                _ => {}
            }
        }
    }

    pub fn resume_animation(&mut self, name: &str) -> bool {
        if let Some(state) = self.0.get_mut(name) {
            match state {
                InternalPlaybackState::Paused {
                    progress,
                    is_reverse,
                } => {
                    *state = InternalPlaybackState::Playing {
                        progress: *progress,
                        is_reverse: *is_reverse,
                    }
                }
                _ => {}
            }

            true
        } else {
            false
        }
    }

    pub fn resume_all_animations(&mut self) {
        for state in self.0.values_mut() {
            match state {
                InternalPlaybackState::Paused {
                    progress,
                    is_reverse,
                } => {
                    *state = InternalPlaybackState::Playing {
                        progress: *progress,
                        is_reverse: *is_reverse,
                    }
                }
                _ => {}
            }
        }
    }

    pub fn reverse_animation(&mut self, name: &str) -> bool {
        if let Some(state) = self.0.get_mut(name) {
            match state {
                InternalPlaybackState::Paused { is_reverse, .. }
                | InternalPlaybackState::Playing { is_reverse, .. } => {
                    *is_reverse = true;
                }
                _ => {}
            }
            true
        } else {
            false
        }
    }

    pub fn play_or_reverse_animation(&mut self, name: &str) -> bool {
        if let Some(state) = self.0.get_mut(name) {
            match state {
                InternalPlaybackState::Paused { is_reverse, .. }
                | InternalPlaybackState::Playing { is_reverse, .. } => {
                    *is_reverse = !*is_reverse;
                }
                InternalPlaybackState::Stopped => {
                    *state = InternalPlaybackState::Playing {
                        progress: usize::MAX,
                        is_reverse: true,
                    };
                }
            }
            true
        } else {
            false
        }
    }
}

type IsLayoutNodeFilter = (With<LayoutAnimationPlaybackState>, With<LayoutHandle>);

enum DescendantId {
    None,
    This,
    Other(Entity),
}

/// Gets the ID of the descendant referenced by the path
///
/// If the path is empty, and contains no components, this will return the ID of the entity
/// that was passed in
fn try_get_descendant_id(world: &World, entity: EntityRef<'_>, id: &Utf8Path) -> DescendantId {
    let start_id = entity.id();
    let mut current = start_id;
    'search: for expecting in id.iter() {
        let entity = world.entity(current);
        let Some(children) = entity.get::<Children>() else {
            log::warn!("Entity {current:?} does not have any children");
            return DescendantId::None;
        };

        for child_id in children.iter().copied() {
            let child = world.entity(child_id);
            let Some(layout_id) = child.get::<LayoutNodeId>() else {
                log::trace!("Entity {child_id:?} is in the layout tree but has no LayoutNodeId");
                continue;
            };

            if expecting == layout_id.name() {
                current = child_id;
                continue 'search;
            }
        }

        log::error!("Entity {current:?} did not have a child by the name of '{expecting}'");
        return DescendantId::None;
    }

    if start_id == current {
        DescendantId::This
    } else {
        DescendantId::Other(current)
    }
}

pub(crate) fn update_animations(world: &mut World) {
    let delta_ms = world.resource::<Time>().delta().as_millis();
    let asset_server = world.resource::<AssetServer>().clone();

    world.resource_scope::<Assets<LayoutAnimation>, _>(move |world, animations| {
        let mut query = world.query_filtered::<EntityMut, IsLayoutNodeFilter>();

        let world = world.as_unsafe_world_cell();

        // SAFETY: This is going to be safe because we are only going to get the EntityMut of
        // descendants of a node while it is getting accessed
        unsafe { query.iter_unchecked(world) }.for_each(|mut entity| {
            // SAFETY: We ensure via the query filter that this entity has a LayoutHandle
            let layout_handle_id =
                unsafe { entity.get::<LayoutHandle>().unwrap_unchecked().0.id() };

            let Some(path) = asset_server.get_path(layout_handle_id).map(|path| path.into_owned()) else {
                log::warn!(
                    "Failed to get asset path for loaded layout with id {:?}",
                    layout_handle_id
                );
                return;
            };

            let mut state = std::mem::take(
                // SAFETY: We ensure via the query filter that this entity has
                // LayoutAnimationPlaybackState
                unsafe {
                    entity
                        .get_mut::<LayoutAnimationPlaybackState>()
                        .unwrap_unchecked()
                        .bypass_change_detection() // We bypass change detection here in case
                                                   // we don't end up updating it
                },
            );

            let mut changed = false;
            for (name, state) in state.0.iter_mut() {
                let InternalPlaybackState::Playing {
                    progress,
                    is_reverse,
                } = state
                else {
                    continue;
                };

                let path = path.clone().with_label(name.clone());
                let Some(animation_handle) = asset_server.get_handle::<LayoutAnimation>(&path)
                else {
                    log::warn!("Failed to get asset handle for layout animation '{path:?}'");
                    continue;
                };

                let Some(animation) = animations.get(animation_handle) else {
                    log::warn!("Failed to get layout animation data for '{path:?}");
                    continue;
                };

                changed |= true;

                if *progress == usize::MAX {
                    *progress = animation.values().map(|kf| kf.max_length).max().unwrap_or_default();
                }

                if *is_reverse {
                    // Performing an "as" conversion here is fine, if your game takes over
                    // usize::MAX milliseconds you probably have other concerns than your
                    // layouts animating
                    *progress = progress.saturating_sub(delta_ms as usize);
                } else {
                    *progress = progress.saturating_add(delta_ms as usize);
                }

                let mut are_keyframes_finished = true;

                for (node_id, keyframes) in animation.iter() {
                    if keyframes.channels.is_empty() {
                        log::error!("Keyframes should not be empty! This could be a hard error in the future");
                        continue;
                    }

                    let readonly = entity.as_readonly();
                    // SAFETY: This is safe since we remove the only other active mutable reference
                    // into the world by making it readonly (we will use it as mutable again later
                    // but for all intents and purposes this is safe)
                    let mut node = match try_get_descendant_id(unsafe { world.world() }, readonly, node_id) {
                        DescendantId::None => continue, // We don't log anything because that's done in
                                                        // the function
                        // SAFETY: We are repurposing the EntityMut that we had earlier, it is
                        // still the only exclusive reference
                        DescendantId::This => unsafe { NodeMut::try_new(world, readonly.id()).unwrap() },
                        // SAFETY: This is safe since we have confirmed that it is not the same
                        // entity (therefore no double mutable reference) and we are not iterating
                        // in parallel so we have exclusive access to this entity
                        DescendantId::Other(id) => unsafe { NodeMut::try_new(world, id).unwrap() }
                    };

                    for channel in keyframes.channels.iter() {
                        let index = if let Some(index) = channel.keyframes.iter().position(|kf| {
                            *progress < kf.timestamp_ms
                        }) {
                            are_keyframes_finished = false;
                            index
                        } else {
                            channel.keyframes.len() - 1 // we can safely subtract 1
                                                        // since we check if it is empty
                                                        // above
                        };

                        let kf = &channel.keyframes[index];
                        log::trace!("Animating target {}", kf.target.name());
                        // we are at the start of the animation, no prev keyframe
                        // to interpolate frame
                        if index == 0 {
                            let progress = if kf.timestamp_ms == 0 {
                                1.0
                            } else {
                                *progress as f32 / kf.timestamp_ms as f32
                            };

                            let progress = kf.time_scale.map(progress.clamp(0.0, 1.0));

                            kf.target.interpolate_from_start(node.reborrow(), ResourceRestrictedWorld(world), progress);
                        } else {
                            let prev_kf = &channel.keyframes[index - 1];
                            // doing a non saturating sub here is safe since we sort the 
                            // keyframe list upon construction
                            let delta_kf = kf.timestamp_ms - prev_kf.timestamp_ms; 
                            let progress = if delta_kf == 0 {
                                1.0
                            } else {
                                (*progress - prev_kf.timestamp_ms) as f32 / delta_kf as f32
                            };

                            let progress = kf.time_scale.map(progress.clamp(0.0, 1.0));

                            kf.target.interpolate_with_previous(&prev_kf.target, node.reborrow(), ResourceRestrictedWorld(world), progress);
                        }
                    }
                }

                if are_keyframes_finished || (*is_reverse && *progress == 0) {
                    *state = InternalPlaybackState::Stopped;
                }
            }
            // SAFETY: We ensure via the query filter that this entity has
            // LayoutAnimationPlaybackState
            let mut ref_state = unsafe {
                entity
                    .get_mut::<LayoutAnimationPlaybackState>()
                    .unwrap_unchecked()
            };

            *ref_state.bypass_change_detection() = state;

            if changed {
                ref_state.set_changed();
            }
        });
    });
}
