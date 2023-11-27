use asset::{Layout, LayoutLoader};
use bevy::{
    app::App,
    asset::{Asset, AssetApp, AssetPath, Handle, LoadContext, UntypedAssetId},
    ecs::system::Resource,
    prelude::*,
    render::view::VisibilitySystems,
    transform::TransformSystem,
    utils::HashMap,
};
use input_detection::InputDetection;
use serde::de::DeserializeOwned;
use std::sync::{Arc, RwLock};
use views::NodeWorldViewMut;

pub mod animation;
pub mod asset;
mod components;
pub mod input_detection;
pub mod views;

pub use components::{
    ActiveLayout, ComputedLayoutNodeMetadata, LayoutBundle, LayoutId, LayoutNodeId,
    LayoutNodeMetadata,
};

pub(crate) struct RegisteredAttributeData {
    deserialize:
        fn(serde_value::Value) -> Result<Box<dyn LayoutAttribute>, serde_value::DeserializerError>,
}

#[derive(Default)]
pub(crate) struct AttributeRegistryInner {
    pub(crate) map: HashMap<String, RegisteredAttributeData>,
}

#[derive(Default, Resource)]
pub struct AttributeRegistry {
    inner: Arc<RwLock<AttributeRegistryInner>>,
}

impl AttributeRegistry {
    pub fn register<A: LayoutAttribute + DeserializeOwned>(&self, name: impl ToString) {
        self.inner.write().unwrap().map.insert(
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
}

pub struct RestrictedLoadContext<'a, 'b> {
    pub(crate) load_context: &'a mut LoadContext<'b>,
}

impl<'a, 'b> RestrictedLoadContext<'a, 'b> {
    pub fn load<'c, A: Asset>(&mut self, path: impl Into<AssetPath<'c>>) -> Handle<A> {
        self.load_context.load(path)
    }
}

pub struct LayoutPlugin;

impl Plugin for LayoutPlugin {
    fn build(&self, app: &mut App) {
        let registry = AttributeRegistry::default();

        registry.register::<InputDetection>("InputDetection");

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
                components::node_metadata_propagate.before(components::update_ui_layout_transform),
                components::update_ui_layout_transform.before(TransformSystem::TransformPropagate),
                components::update_ui_layout_visibility
                    .before(VisibilitySystems::VisibilityPropagate),
            ),
        );
    }
}

pub trait LayoutAttribute: Send + Sync + 'static {
    fn apply(&self, world: &mut NodeWorldViewMut);
    fn initialize_dependencies(&mut self, context: &mut RestrictedLoadContext) {}
    fn visit_dependencies(&self, visit_fn: &mut dyn FnMut(UntypedAssetId)) {}
}

pub trait LayoutApp {
    fn register_layout_attribute<A: LayoutAttribute + DeserializeOwned>(
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
            .resource::<AttributeRegistry>()
            .register::<A>(name);
        self
    }
}
