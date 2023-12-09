use animation::{DynamicAnimationTarget, LayoutAnimation, LayoutAnimationTarget, StaticTypeInfo};
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
use builtin::{ColorAnimation, PositionAnimation, RotationAnimation, SizeAnimation};
use components::{LoadedLayout, NodeKind};
use input_detection::InputDetection;
use node::LayoutInfo;
use serde::de::DeserializeOwned;
use std::{
    any::TypeId,
    sync::{Arc, RwLock},
};
use views::NodeEntityMut;

pub mod animation;
pub mod asset;
pub mod builtin;
pub mod components;
pub mod input_detection;
pub mod node;
pub mod views;

pub use components::{ActiveLayout, LayoutBundle, LayoutId, LayoutNodeId};

pub struct DynamicAttribute {
    type_info: StaticTypeInfo,
    data: *mut (),
    // SAFETY: The caller must ensure that the data provided to this function via pointer
    //          is the same type as what was used to create the function
    apply: unsafe fn(*const (), NodeEntityMut),
    // SAFETY: The caller must ensure that the data provided to this function via pointer
    //          is the same type as what was used to create the function, and also that it has an
    //          exclusive reference on the data passed in
    initialize_dependencies: unsafe fn(*mut (), &mut RestrictedLoadContext),
    // SAFETY: The caller must ensure that the data provided to this function via pointer
    //          is the same type as what was used to create the function
    visit_dependencies: unsafe fn(*const (), &mut dyn FnMut(UntypedAssetId)),
}

unsafe impl Send for DynamicAttribute {}
unsafe impl Sync for DynamicAttribute {}

impl DynamicAttribute {
    pub(crate) fn new<T: LayoutAttribute>(data: T) -> Self {
        Self {
            type_info: StaticTypeInfo {
                name: T::NAME,
                type_path: T::short_type_path(),
                type_id: TypeId::of::<T>(),
            },
            data: (Box::leak(Box::new(data)) as *mut T).cast(),
            // We cannot create unsafe closures, but this gets coerced from {{closure}} -> fn(...)
            // -> unsafe fn(...)
            apply: |data, node| unsafe {
                let data = &*data.cast::<T>();
                data.apply(node)
            },
            initialize_dependencies: |data, context| unsafe {
                let data = &mut *data.cast::<T>();
                data.initialize_dependencies(context)
            },
            visit_dependencies: |data, visit_fn| unsafe {
                let data = &*data.cast::<T>();
                data.visit_dependencies(visit_fn)
            },
        }
    }

    pub fn name(&self) -> &str {
        self.type_info.name
    }

    pub fn attribute_type_id(&self) -> TypeId {
        self.type_info.type_id
    }

    pub fn attribute_type_path(&self) -> &str {
        self.type_info.type_path
    }

    pub fn apply(&self, node: NodeEntityMut) {
        // SAFETY: We are using the data that we created when we made this object, so it will be
        // the same type
        unsafe { (self.apply)(self.data, node) }
    }

    pub fn initialize_dependencies(&mut self, context: &mut RestrictedLoadContext) {
        // SAFETY: We are using the data that we created when we made this object, so it will be
        // the same type. We also require an exclusive reference to call this method, so we are
        // good there
        unsafe { (self.initialize_dependencies)(self.data, context) }
    }

    pub fn visit_dependencies(&self, visit_fn: &mut dyn FnMut(UntypedAssetId)) {
        // SAFETY: See same safety comments as above
        unsafe { (self.visit_dependencies)(self.data, visit_fn) }
    }
}

/// Manages registered deserialization methods for attributes
pub(crate) struct RegisteredAttributeData {
    deserialize: fn(serde_value::Value) -> Result<DynamicAttribute, serde_value::DeserializerError>,
}

/// Manages registered deserialization methods for animations
pub(crate) struct RegisteredAnimationData {
    deserialize:
        fn(serde_value::Value) -> Result<DynamicAnimationTarget, serde_value::DeserializerError>,
}

/// Internal registry of layout animations/attributes
///
/// This is internal to yabuil, and external users should rely on [`LayoutRegistry`]
pub(crate) struct LayoutRegistryInner {
    pub(crate) attributes: HashMap<String, RegisteredAttributeData>,
    pub(crate) animations: HashMap<String, RegisteredAnimationData>,
}

impl LayoutRegistryInner {
    pub fn new() -> Self {
        Self {
            animations: Default::default(),
            attributes: Default::default(),
        }
    }
}

/// Registry of layout attributes and animations
///
/// Users can use this registry directly by referencing it as a [`Resource`] once the
/// [`LayoutPlugin`] has been added to [their app](App), or they can use the registration methods
/// from the [`LayoutApp`] extension trait.
///
/// If an attribute or an animation is not registered with this registry, layout assets will fail to deserialize
/// and errors will show up in the bevy asset logs as opposed to this crate, so make sure to keep an eye out.
#[derive(Resource)]
pub struct LayoutRegistry {
    inner: Arc<RwLock<LayoutRegistryInner>>,
}

impl LayoutRegistry {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(LayoutRegistryInner::new())),
        }
    }
}

impl LayoutRegistry {
    /// Registers an attribute for use with a layout asset
    ///
    /// Types registered as an attribute will be able to be deserialized from the layout asset.
    /// If the deserializer encounters an attribute name that it does not recognize, it will produce
    /// an error in the deserializer and the asset will fail to load.
    ///
    /// For more information, see the [`LayoutAttribute`] trait.
    pub fn register_attribute<A: LayoutAttribute + DeserializeOwned>(&self) {
        self.inner.write().unwrap().attributes.insert(
            A::NAME.to_string(),
            RegisteredAttributeData {
                deserialize: |value| {
                    A::deserialize(serde_value::ValueDeserializer::<
                        serde_value::DeserializerError,
                    >::new(value))
                    .map(|v| DynamicAttribute::new(v))
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
    pub fn register_animation<A: LayoutAnimationTarget + DeserializeOwned>(&self) {
        self.inner.write().unwrap().animations.insert(
            A::NAME.to_string(),
            RegisteredAnimationData {
                deserialize: |value| {
                    A::deserialize(serde_value::ValueDeserializer::<
                        serde_value::DeserializerError,
                    >::new(value))
                    .map(|v| DynamicAnimationTarget::new(v))
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

    AnimateLayouts,
}

/// Plugin to add to an [`App`] that enables support for yabuil layouts
#[derive(Default)]
pub struct LayoutPlugin {}

impl Plugin for LayoutPlugin {
    fn build(&self, app: &mut App) {
        let registry = LayoutRegistry::new();

        registry.register_attribute::<InputDetection>();
        registry.register_animation::<PositionAnimation>();
        registry.register_animation::<SizeAnimation>();
        registry.register_animation::<ColorAnimation>();
        registry.register_animation::<RotationAnimation>();

        // Register the types so that they can be used in reflection (also debugging with bevy_inspector_egui)
        app.register_type::<node::Node>()
            .register_type::<LayoutInfo>()
            .register_type::<NodeKind>()
            .register_type::<node::Anchor>()
            .register_type::<LayoutId>()
            .register_type::<LayoutNodeId>()
            .register_type::<PositionAnimation>()
            .register_type::<SizeAnimation>()
            .register_type::<ColorAnimation>()
            .register_type::<RotationAnimation>()
            .register_type::<InputDetection>()
            .add_event::<LoadedLayout>();

        // Register the asset/asset loader
        app.register_asset_loader(LayoutLoader(registry.inner.clone()))
            .insert_resource(registry)
            .init_asset::<Layout>()
            .init_asset::<LayoutAnimation>();

        app.add_systems(
            Update,
            (
                components::spawn_layout_system,
                bevy::ecs::schedule::apply_deferred,
                input_detection::update_input_detection_nodes,
                animation::update_animations.in_set(LayoutSystems::AnimateLayouts),
            )
                .chain(),
        )
        .add_systems(
            PostUpdate,
            (
                node::refresh_z_index.before(node::propagate_to_transforms),
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
pub trait LayoutAttribute: TypePath + Send + Sync + 'static {
    const NAME: &'static str;

    /// Runs whenever a node that has this attribute gets spawned into the ECS world
    fn apply(&self, world: NodeEntityMut);

    /// Runs during asset loading to help ensure that the [recursive load state](bevy::asset::RecursiveDependencyLoadState)
    /// is accurate and reflects the state of all attributes
    #[allow(unused_variables)]
    fn initialize_dependencies(&mut self, context: &mut RestrictedLoadContext) {}

    /// Used to help ensure that the [recursive load state](bevy::asset::RecursiveDependencyLoadState)
    /// is accurate and reflects the state of all attributes
    #[allow(unused_variables)]
    fn visit_dependencies(&self, visit_fn: &mut dyn FnMut(UntypedAssetId)) {}
}

pub trait LayoutApp {
    fn register_layout_attribute<A: LayoutAttribute + DeserializeOwned>(&mut self) -> &mut Self;

    fn register_layout_animation<A: LayoutAnimationTarget + DeserializeOwned>(
        &mut self,
    ) -> &mut Self;
}

impl LayoutApp for App {
    fn register_layout_attribute<A: LayoutAttribute + DeserializeOwned>(&mut self) -> &mut Self {
        self.world
            .resource::<LayoutRegistry>()
            .register_attribute::<A>();
        self
    }

    fn register_layout_animation<A: LayoutAnimationTarget + DeserializeOwned>(
        &mut self,
    ) -> &mut Self {
        self.world
            .resource::<LayoutRegistry>()
            .register_animation::<A>();
        self
    }
}
