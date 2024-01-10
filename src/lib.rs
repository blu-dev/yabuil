use animation::{DynamicAnimationTarget, LayoutAnimation, LayoutAnimationTarget, StaticTypeInfo};
use asset::{Layout, LayoutLoader};
use bevy::{
    app::App,
    asset::{meta::Settings, Asset, AssetApp, AssetPath, Handle, LoadContext, UntypedAssetId},
    ecs::{schedule::ScheduleLabel, system::Resource},
    prelude::*,
    render::view::VisibilitySystems,
    transform::TransformSystem,
    utils::HashMap,
};
use builtin::{
    ColorAnimation, PositionAnimation, RotationAnimation, ScaleAnimation, SizeAnimation,
};
use components::{LoadedLayout, NodeKind};
use input_detection::{controller::UiInputMap, InputDetection};
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
    pub(crate) ignore_unknown_registry_data: bool,
}

impl LayoutRegistryInner {
    pub fn new(ignore_unknown_registry_data: bool) -> Self {
        Self {
            animations: Default::default(),
            attributes: Default::default(),
            ignore_unknown_registry_data,
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
    pub(crate) fn new(ignore_unknown_registry_data: bool) -> Self {
        Self {
            inner: Arc::new(RwLock::new(LayoutRegistryInner::new(
                ignore_unknown_registry_data,
            ))),
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

/// Due to the strict siloing of layout logic, and the callback based system,
/// yabuil's core layouting logic relies on having exclusive access to the world.
///
/// This schedule runs during the [`Update`] schedule, and it's recommended that if you have
/// any logic you need to run that requires exclusive access and is related to UI to also put that
/// in this system.
#[derive(ScheduleLabel, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct LayoutSchedule;

/// The systems that power the layouting engine
///
/// Use these to properly apply your systems/updates for the most responsive experience.
#[derive(SystemSet, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum LayoutSystems {
    /// Looks over every layout that has been spawned but is waiting to be loaded into the ECS
    /// world. This will log exactly once if a layout asset has failed to load (per component the
    /// layout is attached to) and will recursively spawn in a UI layout once the layout has been
    /// successfully loaded.
    ///
    /// This runs in the [`LayoutSchedule`]
    SpawnLayouts,

    /// Detects changes made to [`ZIndex`] components, and will regenerate a [`ZIndex`] for every
    /// node in the tree.
    ///
    /// It is not recommended to change the [`ZIndex`] of nodes frequently on large layout trees
    /// since this cannot parallelize the operation.
    ///
    /// This runs in the [`PostUpdate`] schedule
    PropagateZIndex,

    /// Applies updates that have happened to [`Node`](node::Node) components to the
    /// transform system.
    ///
    /// This is guaranteed to run before [`TransformSystem::TransformPropagate`] to ensure
    /// that updates applied to nodes during the [`Update`] are represented.
    ///
    /// Any changes to [`Nodes`](node::Node) that take place before the [`PostUpdate`] schedule,
    /// as well as during [`PostUpdate`] but before this system, will be represented.
    ///
    /// This runs in the [`PostUpdate`] schedule
    PropagateToTransforms,

    /// Updates the scale of layout roots to scale them to the size of the window (based on the
    /// layout resolution).
    ///
    /// This runs between [`Self::PropagateToTransforms`] and [`TransformSystem::TransformPropagate`],
    /// in the [`PostUpdate`] schedule
    UpdateLayoutScaling,

    /// Performs focus detection on layout nodes. This will call the focus/unfocus commands for
    /// nodes whose focus state has changed.
    ///
    /// This runs in the [`LayoutSchedule`]
    FocusDetection,

    /// Performs UI input detection on layout nodes. This will run the appropriate
    /// callbacks/commands for any entity that has registered callbacks, and, if the node has an
    /// associated focus state, will only run the commands if the node is focused.
    ///
    /// This runs in the [`LayoutSchedule`]
    InputDetection,

    /// Performs layout node animation. This runs after all other layouting logic to ensure
    /// that any changes intended to be represented this frame are represented.
    AnimateLayouts,

    /// Applies updates that have happened to [`Node`](node::Node) components to the
    /// [`ComputedBoundingBox`] component, if it exists on the node
    ///
    /// This runs after transform propagation, as transforms are used to determine the pixel
    /// coordinates of a node
    ///
    /// Any changes made to the [`Node`](node::Node) component after [`Self::PropagateToTransforms`]
    /// will not be reflected in bounding boxes
    ///
    /// This runs in the [`PostUpdate`] schedule
    PropagateToBoundingBox,

    /// Sets layout visibility to [`Visibility::Hidden`] when they are not set as an [`ActiveLayout`].
    ///
    /// This runs in the [`PostUpdate`] schedule
    UpdateLayoutVisibility,
}

/// Plugin to add to an [`App`] that enables support for yabuil layouts
#[derive(Default)]
pub struct LayoutPlugin {
    pub ignore_unknown_registry_data: bool,
}

impl Plugin for LayoutPlugin {
    fn build(&self, app: &mut App) {
        let registry = LayoutRegistry::new(self.ignore_unknown_registry_data);

        registry.register_attribute::<InputDetection>();
        registry.register_animation::<PositionAnimation>();
        registry.register_animation::<SizeAnimation>();
        registry.register_animation::<ScaleAnimation>();
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
            .register_type::<ScaleAnimation>()
            .register_type::<ColorAnimation>()
            .register_type::<RotationAnimation>()
            .register_type::<InputDetection>()
            .add_event::<LoadedLayout>()
            .init_resource::<UiInputMap>();

        // Register the asset/asset loader
        app.register_asset_loader(LayoutLoader(registry.inner.clone()))
            .insert_resource(registry)
            .init_asset::<Layout>()
            .init_asset::<LayoutAnimation>();

        app.add_systems(Update, |world: &mut World| {
            world.run_schedule(LayoutSchedule)
        });

        app.edit_schedule(LayoutSchedule, |sched| {
            sched.configure_sets(
                (
                    LayoutSystems::SpawnLayouts,
                    LayoutSystems::FocusDetection,
                    LayoutSystems::InputDetection,
                    LayoutSystems::AnimateLayouts,
                )
                    .chain(),
            );

            sched.add_systems((
                components::spawn_layout_system.in_set(LayoutSystems::SpawnLayouts),
                input_detection::controller::update_focus_nodes
                    .in_set(LayoutSystems::FocusDetection),
                input_detection::controller::update_input_detection
                    .in_set(LayoutSystems::InputDetection),
                animation::update_animations.in_set(LayoutSystems::AnimateLayouts),
            ));
        });

        app.edit_schedule(PostUpdate, |sched| {
            sched.configure_sets(
                (
                    LayoutSystems::PropagateZIndex,
                    LayoutSystems::PropagateToTransforms,
                    LayoutSystems::UpdateLayoutScaling,
                    TransformSystem::TransformPropagate,
                    LayoutSystems::PropagateToBoundingBox,
                )
                    .chain(),
            );

            sched.configure_sets(
                (
                    LayoutSystems::UpdateLayoutVisibility,
                    VisibilitySystems::VisibilityPropagate,
                )
                    .chain(),
            );

            sched.add_systems((
                node::refresh_z_index.in_set(LayoutSystems::PropagateZIndex),
                node::propagate_to_transforms.in_set(LayoutSystems::PropagateToTransforms),
                components::update_ui_layout_transform.in_set(LayoutSystems::UpdateLayoutScaling),
                components::update_ui_layout_visibility
                    .in_set(LayoutSystems::UpdateLayoutVisibility),
                node::propagate_to_bounding_box.in_set(LayoutSystems::PropagateToBoundingBox),
            ));
        });
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
