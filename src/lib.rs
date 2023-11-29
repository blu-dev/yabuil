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

pub(crate) struct RegisteredAttributeData {
    deserialize:
        fn(serde_value::Value) -> Result<Box<dyn LayoutAttribute>, serde_value::DeserializerError>,
}

pub(crate) struct RegisteredAnimationData {
    deserialize:
        fn(serde_value::Value) -> Result<Box<dyn LayoutAnimation>, serde_value::DeserializerError>,
}

#[derive(Default)]
pub(crate) struct LayoutRegistryInner {
    pub(crate) attributes: HashMap<String, RegisteredAttributeData>,
    pub(crate) animations: HashMap<String, RegisteredAnimationData>,
}

#[derive(Default, Resource)]
pub struct LayoutRegistry {
    inner: Arc<RwLock<LayoutRegistryInner>>,
}

impl LayoutRegistry {
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

    pub fn register_animation<A: LayoutAnimation + DeserializeOwned>(&self, name: impl ToString) {
        self.inner.write().unwrap().animations.insert(
            name.to_string(),
            RegisteredAnimationData {
                deserialize: |value| {
                    A::deserialize(serde_value::ValueDeserializer::<
                        serde_value::DeserializerError,
                    >::new(value))
                    .map(|v| Box::new(v) as Box<dyn LayoutAnimation>)
                },
            },
        );
    }
}

pub struct RestrictedLoadContext<'a, 'b> {
    pub(crate) load_context: &'a mut LoadContext<'b>,
}

impl<'a, 'b> RestrictedLoadContext<'a, 'b> {
    pub fn load<'c, A: Asset>(&mut self, path: impl Into<AssetPath<'c>>) -> Handle<A> {
        self.load_context.load(path)
    }

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

pub struct LayoutPlugin;

impl Plugin for LayoutPlugin {
    fn build(&self, app: &mut App) {
        let registry = LayoutRegistry::default();

        registry.register_attribute::<InputDetection>("InputDetection");
        registry.register_animation::<PositionAnimation>("Position");
        registry.register_animation::<SizeAnimation>("Size");
        registry.register_animation::<ImageColorAnimation>("ImageColor");
        registry.register_animation::<TextColorAnimation>("TextColor");

        app.register_type::<node::Node>()
            .register_type::<LayoutInfo>()
            .register_type::<NodeKind>()
            .register_type::<node::Anchor>()
            .register_type::<LayoutId>()
            .register_type::<LayoutNodeId>();

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

pub trait LayoutAttribute: Send + Sync + 'static {
    fn apply(&self, world: &mut NodeWorldViewMut);

    #[allow(unused_variables)]
    fn initialize_dependencies(&mut self, context: &mut RestrictedLoadContext) {}

    #[allow(unused_variables)]
    fn visit_dependencies(&self, visit_fn: &mut dyn FnMut(UntypedAssetId)) {}
}

pub trait LayoutAnimation: Send + Sync + 'static {
    fn apply(&self, layout: &mut NodeWorldViewMut);
    fn interpolate(&self, node: &mut NodeViewMut, interpolation: f32);

    #[allow(unused_variables)]
    fn initialize_dependencies(&mut self, context: &mut RestrictedLoadContext) {}

    #[allow(unused_variables)]
    fn visit_dependencies(&self, visit_fn: &mut dyn FnMut(UntypedAssetId)) {}
}

pub trait LayoutApp {
    fn register_layout_attribute<A: LayoutAttribute + DeserializeOwned>(
        &mut self,
        name: impl ToString,
    ) -> &mut Self;

    fn register_layout_animation<A: LayoutAnimation + DeserializeOwned>(
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

    fn register_layout_animation<A: LayoutAnimation + DeserializeOwned>(
        &mut self,
        name: impl ToString,
    ) -> &mut Self {
        self.world
            .resource::<LayoutRegistry>()
            .register_animation::<A>(name);
        self
    }
}
