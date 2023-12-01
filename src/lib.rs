use animation::{ImageColorAnimation, PositionAnimation, SizeAnimation, TextColorAnimation};
use asset::{Layout, LayoutLoader};
use bevy::{
    app::App,
    asset::{meta::Settings, Asset, AssetApp, AssetPath, Handle, LoadContext, UntypedAssetId},
    ecs::system::Resource,
    prelude::*,
    render::view::VisibilitySystems,
    transform::TransformSystem,
    utils::HashMap,
};
use components::NodeKind;
use input_detection::InputDetection;
use node::LayoutInfo;
use serde::de::DeserializeOwned;
use std::sync::{Arc, RwLock};
use views::{NodeViewMut, NodeWorldViewMut};

pub mod animation;
pub mod asset;
pub mod components;
pub mod input_detection;
pub mod node;
pub mod views;

pub use components::{ActiveLayout, LayoutBundle, LayoutId, LayoutNodeId};

/// Manages registered deserialization methods for attributes
pub(crate) struct RegisteredAttributeData {
    deserialize:
        fn(serde_value::Value) -> Result<Box<dyn LayoutAttribute>, serde_value::DeserializerError>,
}

/// Manages registered deserialization methods for animations
pub(crate) struct RegisteredAnimationData {
    deserialize: fn(
        serde_value::Value,
    ) -> Result<Box<dyn LayoutAnimationTarget>, serde_value::DeserializerError>,
}

/// Internal registry of layout animations/attributes
///
/// This is internal to yabuil, and external users should rely on [`LayoutRegistry`]
#[derive(Default)]
pub(crate) struct LayoutRegistryInner {
    pub(crate) attributes: HashMap<String, RegisteredAttributeData>,
    pub(crate) animations: HashMap<String, RegisteredAnimationData>,
}

/// Registry of layout attributes and animations
///
/// Users can use this registry directly by referencing it as a [`Resource`] once the
/// [`LayoutPlugin`] has been added to [their app](App), or they can use the registration methods
/// from the [`LayoutApp`] extension trait.
///
/// If an attribute or an animation is not registered with this registry, layout assets will fail to deserialize
/// and errors will show up in the bevy asset logs as opposed to this crate, so make sure to keep an eye out.
#[derive(Default, Resource)]
pub struct LayoutRegistry {
    inner: Arc<RwLock<LayoutRegistryInner>>,
}

impl LayoutRegistry {
    /// Registers an attribute for use with a layout asset
    ///
    /// Types registered as an attribute will be able to be deserialized from the layout asset.
    /// If the deserializer encounters an attribute name that it does not recognize, it will produce
    /// an error in the deserializer and the asset will fail to load.
    ///
    /// For more information, see the [`LayoutAttribute`] trait.
    pub fn register_attribute<A: LayoutAttribute + DeserializeOwned>(&self, name: impl ToString) {
        self.inner.write().unwrap().attributes.insert(
            name.to_string(),
            RegisteredAttributeData {
                deserialize: |value| {
                    A::deserialize(serde_value::ValueDeserializer::<
                        serde_value::DeserializerError,
                    >::new(value))
                    .map(|v| Box::new(v) as Box<dyn LayoutAttribute>)
                },
            },
        );
    }

    /// Registers an animation for use with a layout asset
    ///
    /// Types registered as an animation will be able to be deserialized from the layout asset.
    /// If the deserializer encounters an animation name that it does not recognize, it will produce
    /// an error in the deserializer and the asset will fail to load.
    ///
    /// For more information, see the [`LayoutAnimation`] trait.
    pub fn register_animation<A: LayoutAnimationTarget + DeserializeOwned>(
        &self,
        name: impl ToString,
    ) {
        self.inner.write().unwrap().animations.insert(
            name.to_string(),
            RegisteredAnimationData {
                deserialize: |value| {
                    A::deserialize(serde_value::ValueDeserializer::<
                        serde_value::DeserializerError,
                    >::new(value))
                    .map(|v| Box::new(v) as Box<dyn LayoutAnimationTarget>)
                },
            },
        );
    }
}

/// A restricted load context for [`LayoutAttribute`]/[`LayoutAnimation`] to use when initializing
/// the assets they depend on during layout asset loading.
pub struct RestrictedLoadContext<'a, 'b> {
    pub(crate) load_context: &'a mut LoadContext<'b>,
}

impl<'a, 'b> RestrictedLoadContext<'a, 'b> {
    /// Loads an asset by path and with the default settings.
    ///
    /// For more context, see the `load` method on [`LoadContext`]
    pub fn load<'c, A: Asset>(&mut self, path: impl Into<AssetPath<'c>>) -> Handle<A> {
        self.load_context.load(path)
    }

    /// Loads an asset by path and with custom settings.
    ///
    /// For more context, see the `load_with_settings` method on [`LoadContext`]
    pub fn load_with_settings<'c, A: Asset, S: Settings + Default>(
        &mut self,
        path: impl Into<AssetPath<'c>>,
        update: impl Fn(&mut S) + Send + Sync + 'static,
    ) -> Handle<A> {
        self.load_context.load_with_settings(path, update)
    }
}

/// The systems that power the layouting engine
///
/// Use these to properly apply your systems/updates for the most responsive experience.
#[derive(SystemSet, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum LayoutSystems {
    /// Applies updates that have happened to [`Node`](node::Node) components to the
    /// transform system.
    ///
    /// This is guaranteed to run before [`TransformSystem::TransformPropagate`] to ensure
    /// that updates applied to nodes during the [`Update`] are represented.
    ///
    /// Any changes to [`Nodes`](node::Node) that take place before the [`PostUpdate`] schedule,
    /// as well as during [`PostUpdate`] but before this system, will be represented.
    PropagateToTransforms,
}

/// Plugin to add to an [`App`] that enables support for yabuil layouts
pub struct LayoutPlugin;

impl Plugin for LayoutPlugin {
    fn build(&self, app: &mut App) {
        let registry = LayoutRegistry::default();

        registry.register_attribute::<InputDetection>("InputDetection");
        registry.register_animation::<PositionAnimation>("Position");
        registry.register_animation::<SizeAnimation>("Size");
        registry.register_animation::<ImageColorAnimation>("ImageColor");
        registry.register_animation::<TextColorAnimation>("TextColor");

        // Register the types so that they can be used in reflection (also debugging with bevy_inspector_egui)
        app.register_type::<node::Node>()
            .register_type::<LayoutInfo>()
            .register_type::<NodeKind>()
            .register_type::<node::Anchor>()
            .register_type::<LayoutId>()
            .register_type::<LayoutNodeId>();

        // Register the asset/asset loader
        app.register_asset_loader(LayoutLoader(registry.inner.clone()))
            .insert_resource(registry)
            .init_asset::<Layout>();

        app.add_systems(
            Update,
            (
                components::spawn_layout_system,
                input_detection::update_input_detection_nodes,
                animation::update_ui_layout_animations,
            ),
        )
        .add_systems(
            PostUpdate,
            (
                node::propagate_to_transforms.before(components::update_ui_layout_transform),
                components::update_ui_layout_transform.before(TransformSystem::TransformPropagate),
                components::update_ui_layout_visibility
                    .before(VisibilitySystems::VisibilityPropagate),
                node::propagate_to_bounding_box.after(TransformSystem::TransformPropagate),
            ),
        );
    }
}

/// A trait for arbitrary entity mutations to be applied to layouts
pub trait LayoutAttribute: Send + Sync + 'static {
    /// Runs whenever a node that has this attribute gets spawned into the ECS world
    fn apply(&self, world: &mut NodeWorldViewMut);

    /// Runs during asset loading to help ensure that the [recursive load state](bevy::asset::RecursiveDependencyLoadState)
    /// is accurate and reflects the state of all attributes
    #[allow(unused_variables)]
    fn initialize_dependencies(&mut self, context: &mut RestrictedLoadContext) {}

    /// Used to help ensure that the [recursive load state](bevy::asset::RecursiveDependencyLoadState)
    /// is accurate and reflects the state of all attributes
    #[allow(unused_variables)]
    fn visit_dependencies(&self, visit_fn: &mut dyn FnMut(UntypedAssetId)) {}
}

/// A trait for arbitrary entity animations to be applied to nodes
pub trait LayoutAnimationTarget: Send + Sync + 'static {
    /// Runs when the animation is playing.
    ///
    /// # Parameters
    /// - `node` is the node that is being animated with this target.
    /// - `interpolation` is the progress of the animation, where `0.0`
    ///     should act as if the animation is at the beginning and `1.0`
    ///     should act as if the animation is at the end. The actual value
    ///     can be outside of the range `[0.0, 1.0]` depending on the time scale
    ///     of the animation
    ///
    /// # Note
    /// Unlike [`LayoutAttribute::apply`], this method takes a [`NodeViewMut`] instead of a
    /// [`NodeWorldViewMut`], which means that you can reference and mutate the nodes however
    /// you cannot add/remove components, despawn, etc.
    fn interpolate(&self, node: &mut NodeViewMut, interpolation: f32);

    /// Runs during asset loading to help ensure that the [recursive load state](bevy::asset::RecursiveDependencyLoadState)
    /// is accurate and reflects the state of all animations
    #[allow(unused_variables)]
    fn initialize_dependencies(&mut self, context: &mut RestrictedLoadContext) {}

    /// Used to help ensure that the [recursive load state](bevy::asset::RecursiveDependencyLoadState)
    /// is accurate and reflects the state of all animations
    #[allow(unused_variables)]
    fn visit_dependencies(&self, visit_fn: &mut dyn FnMut(UntypedAssetId)) {}
}

pub trait LayoutApp {
    fn register_layout_attribute<A: LayoutAttribute + DeserializeOwned>(
        &mut self,
        name: impl ToString,
    ) -> &mut Self;

    fn register_layout_animation<A: LayoutAnimationTarget + DeserializeOwned>(
        &mut self,
        name: impl ToString,
    ) -> &mut Self;
}

impl LayoutApp for App {
    fn register_layout_attribute<A: LayoutAttribute + DeserializeOwned>(
        &mut self,
        name: impl ToString,
    ) -> &mut Self {
        self.world
            .resource::<LayoutRegistry>()
            .register_attribute::<A>(name);
        self
    }

    fn register_layout_animation<A: LayoutAnimationTarget + DeserializeOwned>(
        &mut self,
        name: impl ToString,
    ) -> &mut Self {
        self.world
            .resource::<LayoutRegistry>()
            .register_animation::<A>(name);
        self
    }
}
