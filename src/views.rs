use bevy::{
    ecs::{
        archetype::{Archetype, ArchetypeComponentId},
        change_detection::MutUntyped,
        component::{ComponentId, ComponentTicks, Tick},
        query::{Access, FilteredAccess, ReadOnlyWorldQuery, WorldQuery},
        storage::{Table, TableRow},
        world::unsafe_world_cell::UnsafeWorldCell,
    },
    prelude::*,
    ptr::OwningPtr,
};

use camino::Utf8Path;
use smallvec::SmallVec;
use thiserror::Error;

use crate::{
    animation::{LayoutAnimationPlaybackState, PlaybackState},
    asset::Layout,
    components::NodeKind,
    node::Node,
    LayoutId, LayoutNodeId,
};

#[derive(Error, Debug)]
pub enum NodeEntityError {
    #[error("The entity {0:?} does not exist")]
    InvalidEntity(Entity),

    #[error("The entity {0:?} is not a layout node")]
    NotANode(Entity),

    #[error("The node {0:?} has no children")]
    NoChildren(Entity),

    #[error("The node {0:?} has no child with the name {1}")]
    NoChildWithName(Entity, String),

    #[error("The node {0:?} has no parent entity")]
    NoParent(Entity),

    #[error("The node {0:?} is missing the root layout id")]
    NoRootId(Entity),
}

/// Mutable entity accessor with layout tree traversal capabilities
///
/// This is meant to be a drop-in alternative for [`EntityWorldMut`]. Unlike the built-in
/// accessor, this type does **not** keep track of the entity location, which means that
/// it will be ever-so-slightly less performant than using [`EntityWorldMut`], but the convenience
/// factor of layout tree traversal outweighs that.
pub struct NodeEntityMut<'w> {
    world: UnsafeWorldCell<'w>,
    id: Entity,
}

#[derive(Copy, Clone)]
pub struct NodeRef<'w> {
    world: UnsafeWorldCell<'w>,
    id: Entity,
}

pub struct NodeMut<'w> {
    world: UnsafeWorldCell<'w>,
    id: Entity,
}

fn is_entity_a_node(entity: &EntityRef) -> bool {
    entity.contains::<crate::node::Node>()
}

fn get_parent_id(world: &World, start: Entity) -> Result<Entity, NodeEntityError> {
    let entity_ref = world.entity(start);
    let parent = entity_ref
        .get::<Parent>()
        .ok_or(NodeEntityError::NoParent(start))?
        .get();

    if is_entity_a_node(&world.entity(parent)) {
        Ok(parent)
    } else {
        Err(NodeEntityError::NotANode(parent))
    }
}

fn find_child_id(world: &World, start: Entity, id: &Utf8Path) -> Result<Entity, NodeEntityError> {
    let mut entity = start;
    'search: for component in id.components() {
        let expecting = component.as_str();
        let entity_ref = world.entity(entity);
        let children = entity_ref
            .get::<Children>()
            .ok_or(NodeEntityError::NoChildren(entity))?;
        for child_id in children.iter().copied() {
            let child = world.entity(child_id);
            if !is_entity_a_node(&child) {
                continue;
            }

            let Some(node_id) = child.get::<LayoutNodeId>() else {
                continue;
            };

            if node_id.name() == expecting {
                entity = child_id;
                continue 'search;
            }
        }

        return Err(NodeEntityError::NoChildWithName(
            entity,
            expecting.to_string(),
        ));
    }

    Ok(entity)
}

impl<'w> NodeEntityMut<'w> {
    pub fn reborrow<'a>(&'a mut self) -> NodeEntityMut<'a> {
        Self {
            world: self.world,
            id: self.id,
        }
    }

    pub fn try_new(world: &'w mut World, id: Entity) -> Result<Self, NodeEntityError> {
        let entity_ref = world
            .get_entity(id)
            .ok_or(NodeEntityError::InvalidEntity(id))?;
        if !is_entity_a_node(&entity_ref) {
            return Err(NodeEntityError::NotANode(id));
        }

        Ok(Self {
            world: world.as_unsafe_world_cell(),
            id,
        })
    }

    pub fn try_from_entity_world_mut(entity: EntityWorldMut<'w>) -> Result<Self, NodeEntityError> {
        let id = entity.id();
        let world = entity.into_world_mut();
        Self::try_new(world, id)
    }

    #[track_caller]
    pub fn from_entity_world_mut(entity: EntityWorldMut<'w>) -> Self {
        let id = entity.id();
        let world = entity.into_world_mut();
        Self::new(world, id)
    }

    #[track_caller]
    pub fn new(world: &'w mut World, id: Entity) -> Self {
        Self::try_new(world, id).unwrap()
    }

    pub fn get_root<'a>(&'a mut self) -> Result<NodeEntityMut<'a>, NodeEntityError> {
        let root_id = *self
            .get::<LayoutId>()
            .ok_or(NodeEntityError::NoRootId(self.id))?;

        Ok(NodeEntityMut {
            world: self.world,
            id: root_id.0,
        })
    }

    #[track_caller]
    pub fn root<'a>(&'a mut self) -> NodeEntityMut<'a> {
        self.get_root().unwrap()
    }

    pub fn get_child<'a>(
        &'a mut self,
        id: impl AsRef<Utf8Path>,
    ) -> Result<NodeEntityMut<'a>, NodeEntityError> {
        let child_id = find_child_id(self.world(), self.id, id.as_ref())?;

        Ok(NodeEntityMut {
            world: self.world,
            id: child_id,
        })
    }

    pub fn visit_children(&mut self, mut f: impl FnMut(NodeEntityMut)) {
        let children: SmallVec<[Entity; 8]> = self
            .get::<Children>()
            .iter()
            .flat_map(|children| children.iter().copied())
            .collect();
        for child in children {
            f(NodeEntityMut {
                world: self.world,
                id: child,
            });
        }
    }

    #[track_caller]
    pub fn child<'a>(&'a mut self, id: impl AsRef<Utf8Path>) -> NodeEntityMut<'a> {
        self.get_child(id).unwrap()
    }

    pub fn get_parent<'a>(&'a mut self) -> Result<NodeEntityMut<'a>, NodeEntityError> {
        // SAFETY: See above safety remarks in get_child
        let parent_id = get_parent_id(self.world(), self.id)?;
        Ok(NodeEntityMut {
            world: self.world,
            id: parent_id,
        })
    }

    #[track_caller]
    pub fn parent<'a>(&'a mut self) -> NodeEntityMut<'a> {
        self.get_parent().unwrap()
    }

    pub fn get_sibling<'a>(
        &'a mut self,
        id: impl AsRef<Utf8Path>,
    ) -> Result<NodeEntityMut<'a>, NodeEntityError> {
        // SAFETY: See above safety remarks in get_child
        let world = self.world();
        let parent_id = get_parent_id(world, self.id)?;
        let child_id = find_child_id(world, parent_id, id.as_ref())?;
        Ok(NodeEntityMut {
            world: self.world,
            id: child_id,
        })
    }

    #[track_caller]
    pub fn sibling<'a>(&'a mut self, id: impl AsRef<Utf8Path>) -> NodeEntityMut<'a> {
        self.get_sibling(id).unwrap()
    }

    pub fn get_image<'a>(&'a mut self) -> Option<ImageNodeMut<'a>> {
        (*self.get::<NodeKind>().unwrap() == NodeKind::Image)
            .then(|| ImageNodeMut(From::from(self.reborrow())))
    }

    #[track_caller]
    pub fn image<'a>(&'a mut self) -> ImageNodeMut<'a> {
        self.get_image().expect("node should be an image node")
    }

    pub fn get_text<'a>(&'a mut self) -> Option<TextNodeMut<'a>> {
        (*self.get::<NodeKind>().unwrap() == NodeKind::Text)
            .then(|| TextNodeMut(From::from(self.reborrow())))
    }

    #[track_caller]
    pub fn text<'a>(&'a mut self) -> TextNodeMut<'a> {
        self.get_text().expect("node should be a text node")
    }

    pub fn get_layout<'a>(&'a mut self) -> Option<LayoutNodeMut<'a>> {
        (*self.get::<NodeKind>().unwrap() == NodeKind::Layout)
            .then(|| LayoutNodeMut(From::from(self.reborrow())))
    }

    #[track_caller]
    pub fn layout<'a>(&'a mut self) -> LayoutNodeMut<'a> {
        self.get_layout().expect("node should be a layout node")
    }

    pub fn get_group<'a>(&'a mut self) -> Option<GroupNodeMut<'a>> {
        (*self.get::<NodeKind>().unwrap() == NodeKind::Group).then(|| GroupNodeMut {
            world: self.world,
            id: self.id,
        })
    }

    #[track_caller]
    pub fn group<'a>(&'a mut self) -> GroupNodeMut<'a> {
        self.get_group().expect("node should be a group node")
    }

    pub fn world(&self) -> &World {
        // SAFETY: We acquire an exclusive reference to the world on construction of this type,
        //          or any of it's parents. Rust's borrow checker will restrict using more than one
        //          `NodeEntityMut` at a time since it cannot guarantee that they correspond
        //          to different entities, so this is safe
        unsafe { self.world.world() }
    }

    pub fn world_mut(&mut self) -> &mut World {
        // SAFETY: We acquire an exclusive reference to the world on construction of this type,
        //          and we require an exclusive reference to call this method, therefore we
        //          will still have an exclusive reference when calling this.
        //          Any new NodeEntityMut constructed based on this one will have a lesser
        //          lifetime which prevents multiple access to the same UnsafeWorldCell at a time
        unsafe { self.world.world_mut() }
    }

    pub fn get<T: Component>(&self) -> Option<&T> {
        self.world().entity(self.id).get::<T>()
    }

    pub fn get_by_id(&self, component_id: ComponentId) -> Option<bevy::ptr::Ptr<'_>> {
        self.world().entity(self.id).get_by_id(component_id)
    }

    pub fn get_change_ticks<T: Component>(&self) -> Option<ComponentTicks> {
        self.world().entity(self.id).get_change_ticks::<T>()
    }

    pub fn get_change_ticks_by_id(&self, component_id: ComponentId) -> Option<ComponentTicks> {
        self.world()
            .entity(self.id)
            .get_change_ticks_by_id(component_id)
    }

    pub fn get_mut<T: Component>(&mut self) -> Option<Mut<'_, T>> {
        #[inline(never)]
        #[cold]
        fn panic_if_missing_entity(entity: Entity) -> ! {
            panic!("Failed to get entity {entity:?} even though we checked it at construction");
        }

        let Some(entity) = self.world.get_entity(self.id) else {
            panic_if_missing_entity(self.id);
        };

        // SAFETY: We have exclusive ownership over the world, which means that this is the only
        // reference for the component on the entity
        unsafe { entity.get_mut::<T>() }
    }

    pub fn get_mut_by_id(&mut self, component_id: ComponentId) -> Option<MutUntyped<'_>> {
        #[inline(never)]
        #[cold]
        fn panic_if_missing_entity(entity: Entity) -> ! {
            panic!("Failed to get entity {entity:?} even though we checked it at construction");
        }

        let Some(entity) = self.world.get_entity(self.id) else {
            panic_if_missing_entity(self.id);
        };

        // SAFETY: We have exclusive ownership over the world, which means that this is the only
        // reference for the component on the entity
        unsafe { entity.get_mut_by_id(component_id) }
    }

    pub fn insert<T: Bundle>(&mut self, bundle: T) -> &mut Self {
        let id = self.id;
        self.world_mut().entity_mut(id).insert(bundle);
        self
    }

    pub unsafe fn insert_by_id(
        &mut self,
        component_id: ComponentId,
        component: OwningPtr<'_>,
    ) -> &mut Self {
        let id = self.id;
        self.world_mut()
            .entity_mut(id)
            .insert_by_id(component_id, component);
        self
    }

    pub unsafe fn insert_by_ids<'a, I>(
        &mut self,
        component_ids: &[ComponentId],
        components: I,
    ) -> &mut Self
    where
        I: Iterator<Item = OwningPtr<'a>>,
    {
        let id = self.id;
        self.world_mut()
            .entity_mut(id)
            .insert_by_ids(component_ids, components);
        self
    }

    pub fn take<T: Bundle>(&mut self) -> Option<T> {
        let id = self.id;
        self.world_mut().entity_mut(id).take::<T>()
    }

    pub fn remove<T: Bundle>(&mut self) -> &mut Self {
        let id = self.id;
        self.world_mut().entity_mut(id).remove::<T>();
        self
    }

    pub fn id(&self) -> Entity {
        self.id
    }

    pub fn into_world(self) -> &'w mut World {
        let Self { world, .. } = self;
        // SAFETY: See Self::world_mut
        unsafe { world.world_mut() }
    }

    pub fn into_entity_world_mut(self) -> EntityWorldMut<'w> {
        let Self { world, id } = self;
        // SAFETY: See Self::world_mut
        let world = unsafe { world.world_mut() };
        world.entity_mut(id)
    }
}

impl<'w> NodeRef<'w> {
    #[inline(never)]
    #[cold]
    fn panic_if_wrong_entity(entity: Entity) -> ! {
        panic!("Failed to get the entity {entity:?}");
    }

    /// SAFETY: The caller must ensure that there are no exclusive references or accesses to any
    /// component on the provided entity
    pub(crate) unsafe fn try_new(
        world: UnsafeWorldCell<'w>,
        id: Entity,
    ) -> Result<Self, NodeEntityError> {
        let entity = world
            .get_entity(id)
            .ok_or(NodeEntityError::InvalidEntity(id))?;

        if entity.get::<Node>().is_none() {
            return Err(NodeEntityError::NotANode(id));
        }

        Ok(Self { world, id })
    }

    pub fn node_id(&self) -> &LayoutNodeId {
        self.get::<LayoutNodeId>()
            .expect("node should have node id")
    }

    pub fn layout_id(&self) -> LayoutId {
        self.get::<LayoutId>()
            .copied()
            .expect("node should have layout id")
    }

    pub fn node_data(&self) -> &Node {
        self.get::<Node>().expect("node should have node data")
    }

    pub fn node_kind(&self) -> NodeKind {
        *self.get::<NodeKind>().expect("node should have node kind")
    }

    pub fn get_image(&self) -> Option<ImageNodeRef<'w>> {
        (*self.get::<NodeKind>().unwrap() == NodeKind::Image).then(|| ImageNodeRef(*self))
    }

    #[track_caller]
    pub fn image(&self) -> ImageNodeRef<'w> {
        self.get_image().expect("node should be an image node")
    }

    pub fn get_text(&self) -> Option<TextNodeRef<'w>> {
        (*self.get::<NodeKind>().unwrap() == NodeKind::Text).then(|| TextNodeRef(*self))
    }

    #[track_caller]
    pub fn text(&self) -> TextNodeRef<'w> {
        self.get_text().expect("node should be a text node")
    }

    pub fn get_layout(&self) -> Option<LayoutNodeRef<'w>> {
        (*self.get::<NodeKind>().unwrap() == NodeKind::Layout).then(|| LayoutNodeRef(*self))
    }

    #[track_caller]
    pub fn layout(&self) -> LayoutNodeRef<'w> {
        self.get_layout().expect("node should be a layout node")
    }

    pub fn get<T: Component>(&self) -> Option<&T> {
        let Some(entity) = self.world.get_entity(self.id) else {
            Self::panic_if_wrong_entity(self.id)
        };
        // SAFETY: Construction of this type is either from a more powerful ownership
        // (ownership of the whole world) or on the user to guarantee this is the only exclusive
        // access
        unsafe { entity.get::<T>() }
    }

    pub fn get_by_id(&self, component_id: ComponentId) -> Option<bevy::ptr::Ptr<'_>> {
        let Some(entity) = self.world.get_entity(self.id) else {
            Self::panic_if_wrong_entity(self.id)
        };

        // SAFETY: See above comments
        unsafe { entity.get_by_id(component_id) }
    }

    pub fn get_change_ticks<T: Component>(&self) -> Option<ComponentTicks> {
        let Some(entity) = self.world.get_entity(self.id) else {
            Self::panic_if_wrong_entity(self.id)
        };

        // SAFETY: See above comments
        unsafe { entity.get_change_ticks::<T>() }
    }

    pub fn get_change_ticks_by_id(&self, component_id: ComponentId) -> Option<ComponentTicks> {
        let Some(entity) = self.world.get_entity(self.id) else {
            Self::panic_if_wrong_entity(self.id)
        };

        // SAFETY: See above comments
        unsafe { entity.get_change_ticks_by_id(component_id) }
    }
}

impl<'w> NodeMut<'w> {
    #[inline(never)]
    #[cold]
    fn panic_if_wrong_entity(entity: Entity) -> ! {
        panic!("Failed to get the entity {entity:?}");
    }

    /// SAFETY: The caller must ensure that there are no other references or accesses to any
    /// component on the provided entity
    pub(crate) unsafe fn try_new(
        world: UnsafeWorldCell<'w>,
        id: Entity,
    ) -> Result<Self, NodeEntityError> {
        let entity = world
            .get_entity(id)
            .ok_or(NodeEntityError::InvalidEntity(id))?;
        if entity.get::<Node>().is_none() {
            return Err(NodeEntityError::NotANode(id));
        }

        Ok(Self { world, id })
    }

    pub fn node_id(&self) -> &LayoutNodeId {
        self.get::<LayoutNodeId>()
            .expect("node should have node id")
    }

    pub fn layout_id(&self) -> LayoutId {
        self.get::<LayoutId>()
            .copied()
            .expect("node should have layout id")
    }

    pub fn node_data(&self) -> &Node {
        self.get::<Node>().expect("node should have node data")
    }

    pub fn node_kind(&self) -> NodeKind {
        *self.get::<NodeKind>().expect("node should have node kind")
    }

    pub fn get_image<'a>(&'a mut self) -> Option<ImageNodeMut<'a>> {
        (*self.get::<NodeKind>().unwrap() == NodeKind::Image).then(|| ImageNodeMut(self.reborrow()))
    }

    #[track_caller]
    pub fn image<'a>(&'a mut self) -> ImageNodeMut<'a> {
        self.get_image().expect("node should be an image node")
    }

    pub fn get_text<'a>(&'a mut self) -> Option<TextNodeMut<'a>> {
        (*self.get::<NodeKind>().unwrap() == NodeKind::Text).then(|| TextNodeMut(self.reborrow()))
    }

    #[track_caller]
    pub fn text<'a>(&'a mut self) -> TextNodeMut<'a> {
        self.get_text().expect("node should be a text node")
    }

    pub fn get_layout<'a>(&'a mut self) -> Option<LayoutNodeMut<'a>> {
        (*self.get::<NodeKind>().unwrap() == NodeKind::Layout)
            .then(|| LayoutNodeMut(self.reborrow()))
    }

    #[track_caller]
    pub fn layout<'a>(&'a mut self) -> LayoutNodeMut<'a> {
        self.get_layout().expect("node should be a layout node")
    }

    pub fn reborrow<'a>(&'a mut self) -> NodeMut<'a> {
        Self {
            world: self.world,
            id: self.id,
        }
    }

    pub fn get<T: Component>(&self) -> Option<&T> {
        let Some(entity) = self.world.get_entity(self.id) else {
            Self::panic_if_wrong_entity(self.id)
        };
        // SAFETY: Construction of this type is either from a more powerful ownership
        // (ownership of the whole world) or on the user to guarantee this is the only exclusive
        // access
        unsafe { entity.get::<T>() }
    }

    pub fn get_by_id(&self, component_id: ComponentId) -> Option<bevy::ptr::Ptr<'_>> {
        let Some(entity) = self.world.get_entity(self.id) else {
            Self::panic_if_wrong_entity(self.id)
        };

        // SAFETY: See above comments
        unsafe { entity.get_by_id(component_id) }
    }

    pub fn get_change_ticks<T: Component>(&self) -> Option<ComponentTicks> {
        let Some(entity) = self.world.get_entity(self.id) else {
            Self::panic_if_wrong_entity(self.id)
        };

        // SAFETY: See above comments
        unsafe { entity.get_change_ticks::<T>() }
    }

    pub fn get_change_ticks_by_id(&self, component_id: ComponentId) -> Option<ComponentTicks> {
        let Some(entity) = self.world.get_entity(self.id) else {
            Self::panic_if_wrong_entity(self.id)
        };

        // SAFETY: See above comments
        unsafe { entity.get_change_ticks_by_id(component_id) }
    }

    pub fn get_mut<T: Component>(&mut self) -> Option<Mut<'_, T>> {
        let Some(entity) = self.world.get_entity(self.id) else {
            Self::panic_if_wrong_entity(self.id)
        };
        // SAFETY: See above comments
        unsafe { entity.get_mut::<T>() }
    }

    pub fn get_mut_by_id(&mut self, component_id: ComponentId) -> Option<MutUntyped<'_>> {
        let Some(entity) = self.world.get_entity(self.id) else {
            Self::panic_if_wrong_entity(self.id)
        };
        // SAFETY: See above comments
        unsafe { entity.get_mut_by_id(component_id) }
    }
}

impl<'w> From<NodeEntityMut<'w>> for NodeMut<'w> {
    fn from(value: NodeEntityMut<'w>) -> Self {
        Self {
            world: value.world,
            id: value.id,
        }
    }
}

#[derive(Deref)]
pub struct ImageNodeRef<'w>(NodeRef<'w>);

impl ImageNodeRef<'_> {
    #[track_caller]
    pub fn image(&self) -> &Handle<Image> {
        self.0
            .get::<Handle<Image>>()
            .expect("Image node should have a Handle<Image> component, did you remove it?")
    }

    #[track_caller]
    pub fn sprite_data(&self) -> &Sprite {
        self.0
            .get::<Sprite>()
            .expect("Image node should have a Sprite component, did you remove it?")
    }
}

#[derive(Deref, DerefMut)]
pub struct ImageNodeMut<'w>(NodeMut<'w>);

impl ImageNodeMut<'_> {
    #[track_caller]
    pub fn image(&self) -> &Handle<Image> {
        self.0
            .get::<Handle<Image>>()
            .expect("Image node should have a Handle<Image> component, did you remove it?")
    }

    #[track_caller]
    pub fn set_image(&mut self, handle: impl Into<Handle<Image>>) {
        *self
            .0
            .get_mut::<Handle<Image>>()
            .expect("Image node should have a Handle<Image> component, did you remove it?") =
            handle.into();
    }

    #[track_caller]
    pub fn sprite_data(&self) -> &Sprite {
        self.0
            .get::<Sprite>()
            .expect("Image node should have a Sprite component, did you remove it?")
    }

    #[track_caller]
    pub fn sprite_data_mut(&mut self) -> Mut<'_, Sprite> {
        self.0
            .get_mut::<Sprite>()
            .expect("Image node should have a Sprite component, did you remove it?")
    }
}

#[derive(Deref)]
pub struct TextNodeRef<'w>(NodeRef<'w>);

impl TextNodeRef<'_> {
    #[track_caller]
    fn text_component(&self) -> &Text {
        self.0
            .get::<Text>()
            .expect("Text node should have a text component, did you remove it?")
    }

    #[track_caller]
    pub fn text(&self) -> &str {
        self.text_component().sections[0].value.as_str()
    }

    #[track_caller]
    pub fn style(&self) -> &TextStyle {
        &self.text_component().sections[0].style
    }
}

#[derive(Deref, DerefMut)]
pub struct TextNodeMut<'w>(NodeMut<'w>);

impl TextNodeMut<'_> {
    #[track_caller]
    fn text_component(&self) -> &Text {
        self.0
            .get::<Text>()
            .expect("Text node should have a text component, did you remove it?")
    }

    #[track_caller]
    fn text_component_mut(&mut self) -> Mut<'_, Text> {
        self.0
            .get_mut::<Text>()
            .expect("Text node should have a text component, did you remove it?")
    }

    #[track_caller]
    pub fn text(&self) -> &str {
        self.text_component().sections[0].value.as_str()
    }

    #[track_caller]
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text_component_mut().sections[0].value = text.into();
    }

    #[track_caller]
    pub fn style(&self) -> &TextStyle {
        &self.text_component().sections[0].style
    }

    #[track_caller]
    pub fn style_mut(&mut self) -> Mut<'_, TextStyle> {
        self.text_component_mut()
            .map_unchanged(|text| &mut text.sections[0].style)
    }
}

#[derive(Error, Debug)]
pub enum LayoutAnimationError {
    #[error("Animation with name '{0}' was not found and could not be played")]
    NoAnimation(String),
}

#[derive(Deref)]
pub struct LayoutNodeRef<'w>(NodeRef<'w>);

impl LayoutNodeRef<'_> {
    /// Checks if this layout is currently playing ANY animations
    pub fn is_playing_any(&self) -> bool {
        self.0
            .get::<LayoutAnimationPlaybackState>()
            .expect("LayoutNode should have playback state")
            .is_playing_any()
    }

    /// Gets the state of the provided animation, if it exists
    pub fn animation_state(&self, name: impl AsRef<str>) -> Option<PlaybackState> {
        self.0
            .get::<LayoutAnimationPlaybackState>()
            .expect("LayoutNode should have playback state")
            .playback_state(name.as_ref())
    }
}

#[derive(Deref, DerefMut)]
pub struct LayoutNodeMut<'w>(NodeMut<'w>);

impl LayoutNodeMut<'_> {
    /// Checks if this layout is currently playing ANY animations
    pub fn is_playing_any(&self) -> bool {
        self.0
            .get::<LayoutAnimationPlaybackState>()
            .expect("LayoutNode should have playback state")
            .is_playing_any()
    }

    /// Gets the state of the provided animation, if it exists
    pub fn animation_state(&self, name: impl AsRef<str>) -> Option<PlaybackState> {
        self.0
            .get::<LayoutAnimationPlaybackState>()
            .expect("LayoutNode should have playback state")
            .playback_state(name.as_ref())
    }

    /// Plays the animation if it exists and is not already playing
    pub fn play_animation(&mut self, name: impl AsRef<str>) -> Result<(), LayoutAnimationError> {
        let name = name.as_ref();
        self.0
            .get_mut::<LayoutAnimationPlaybackState>()
            .expect("LayoutNode should have playback state")
            .play_animation(name)
            .then_some(())
            .ok_or_else(|| LayoutAnimationError::NoAnimation(name.to_string()))
    }

    pub fn stop_animation(&mut self, name: impl AsRef<str>) -> Result<(), LayoutAnimationError> {
        let name = name.as_ref();
        self.0
            .get_mut::<LayoutAnimationPlaybackState>()
            .expect("LayoutNode should have playback state")
            .stop_animation(name)
            .then_some(())
            .ok_or_else(|| LayoutAnimationError::NoAnimation(name.to_string()))
    }

    /// Pauses the animation if it is currently playing, does nothing if the animation is
    /// already paused or not playing
    pub fn pause_animation(&mut self, name: impl AsRef<str>) -> Result<(), LayoutAnimationError> {
        let name = name.as_ref();
        self.0
            .get_mut::<LayoutAnimationPlaybackState>()
            .expect("LayoutNode should have playback state")
            .pause_animation(name)
            .then_some(())
            .ok_or_else(|| LayoutAnimationError::NoAnimation(name.to_string()))
    }

    /// Pauses all animations that are currently playing
    pub fn pause_all_animations(&mut self) {
        self.0
            .get_mut::<LayoutAnimationPlaybackState>()
            .expect("LayoutNode should have playback state")
            .pause_all_animations()
    }

    /// Resumes an animation if it is currently playing, does nothing if the animation
    /// is not paused or is not playing
    pub fn resume_animation(&mut self, name: impl AsRef<str>) -> Result<(), LayoutAnimationError> {
        let name = name.as_ref();
        self.0
            .get_mut::<LayoutAnimationPlaybackState>()
            .expect("LayoutNode should have playback state")
            .resume_animation(name)
            .then_some(())
            .ok_or_else(|| LayoutAnimationError::NoAnimation(name.to_string()))
    }

    /// Resumes all paused animations
    pub fn resume_all_animations(&mut self) {
        self.0
            .get_mut::<LayoutAnimationPlaybackState>()
            .expect("LayoutNode should have playback state")
            .resume_all_animations()
    }

    /// Reverses an animation if it is currently playing, does nothing if the animation
    /// is not paused or is not playing
    ///
    /// This can be useful for animations that can be interrupted by another action,
    /// such as scrolling through options on a menu
    pub fn reverse_animation(&mut self, name: impl AsRef<str>) -> Result<(), LayoutAnimationError> {
        let name = name.as_ref();
        self.0
            .get_mut::<LayoutAnimationPlaybackState>()
            .expect("LayoutNode should have playback state")
            .reverse_animation(name)
            .then_some(())
            .ok_or_else(|| LayoutAnimationError::NoAnimation(name.to_string()))
    }

    /// Reverses an animation if it is currently playing, and if it is not playing
    /// then will play the animation in reverse
    pub fn play_or_reverse_animation(
        &mut self,
        name: impl AsRef<str>,
    ) -> Result<(), LayoutAnimationError> {
        let name = name.as_ref();
        self.0
            .get_mut::<LayoutAnimationPlaybackState>()
            .expect("LayoutNode should have playback state")
            .play_or_reverse_animation(name)
            .then_some(())
            .ok_or_else(|| LayoutAnimationError::NoAnimation(name.to_string()))
    }
}

pub struct GroupNodeMut<'w> {
    world: UnsafeWorldCell<'w>,
    id: Entity,
}

pub struct NodeArgs {}

pub struct ImageNodeArgs {
    pub node: NodeArgs,
    pub image: Handle<Image>,
    pub tint: Option<Color>,
}

pub struct TextNodeArgs {
    pub node: NodeArgs,
    pub text: String,
    pub font_size: f32,
    pub font: Handle<Font>,
    pub color: Color,
}

pub struct LayoutNodeArgs {
    pub node: NodeArgs,
    pub layout: Handle<Layout>,
}

impl<'w> GroupNodeMut<'w> {
    pub fn add_image_node<'a>(&'a mut self, args: ImageNodeArgs) -> NodeEntityMut<'a> {
        todo!()
    }

    pub fn add_text_node<'a>(&'a mut self, args: TextNodeArgs) -> NodeEntityMut<'a> {
        todo!()
    }

    pub fn add_layout_node<'a>(&'a mut self, args: LayoutNodeArgs) -> NodeEntityMut<'a> {
        todo!()
    }

    pub fn add_group_node<'a>(&'a mut self, args: NodeArgs) -> GroupNodeMut<'a> {
        todo!()
    }

    pub fn into_node(self) -> NodeEntityMut<'w> {
        NodeEntityMut {
            world: self.world,
            id: self.id,
        }
    }
}

unsafe impl<'a> ReadOnlyWorldQuery for NodeRef<'a> {}

unsafe impl<'a> WorldQuery for NodeRef<'a> {
    type Fetch<'w> = UnsafeWorldCell<'w>;
    type Item<'w> = NodeRef<'w>;
    type ReadOnly = Self;
    type State = ComponentId;

    fn shrink<'wlong: 'wshort, 'wshort>(item: Self::Item<'wlong>) -> Self::Item<'wshort> {
        item
    }

    const IS_DENSE: bool = true;
    const IS_ARCHETYPAL: bool = true;

    unsafe fn init_fetch<'w>(
        world: UnsafeWorldCell<'w>,
        _state: &Self::State,
        _last_run: Tick,
        _this_run: Tick,
    ) -> Self::Fetch<'w> {
        world
    }

    #[inline]
    unsafe fn set_archetype<'w>(
        _fetch: &mut Self::Fetch<'w>,
        _state: &Self::State,
        _archetype: &'w Archetype,
        _table: &'w Table,
    ) {
    }

    #[inline]
    unsafe fn set_table<'w>(_fetch: &mut Self::Fetch<'w>, _state: &Self::State, _table: &'w Table) {
    }

    #[inline(always)]
    unsafe fn fetch<'w>(
        fetch: &mut Self::Fetch<'w>,
        entity: Entity,
        _table_row: TableRow,
    ) -> Self::Item<'w> {
        NodeRef::try_new(*fetch, entity).unwrap()
    }

    fn update_component_access(_state: &Self::State, access: &mut FilteredAccess<ComponentId>) {
        assert!(
            !access.access().has_any_read(),
            "NodeMut conflicts with a previous access in this query. Exclusive access cannot coincide with any other accesses."
        );

        access.write_all()
    }

    fn update_archetype_component_access(
        _state: &Self::State,
        archetype: &Archetype,
        access: &mut Access<ArchetypeComponentId>,
    ) {
        for component_id in archetype.components() {
            access.add_write(archetype.get_archetype_component_id(component_id).unwrap());
        }
    }

    fn init_state(world: &mut World) -> Self::State {
        world.init_component::<Node>()
    }

    fn matches_component_set(
        state: &Self::State,
        set_contains_id: &impl Fn(ComponentId) -> bool,
    ) -> bool {
        set_contains_id(*state)
    }
}

unsafe impl<'a> WorldQuery for NodeMut<'a> {
    type Fetch<'w> = UnsafeWorldCell<'w>;
    type Item<'w> = NodeMut<'w>;
    type ReadOnly = NodeRef<'a>;
    type State = ComponentId; // The node component ID

    fn shrink<'wlong: 'wshort, 'wshort>(item: Self::Item<'wlong>) -> Self::Item<'wshort> {
        item
    }

    const IS_DENSE: bool = true;
    const IS_ARCHETYPAL: bool = true;

    unsafe fn init_fetch<'w>(
        world: UnsafeWorldCell<'w>,
        _state: &Self::State,
        _last_run: Tick,
        _this_run: Tick,
    ) -> Self::Fetch<'w> {
        world
    }

    #[inline]
    unsafe fn set_archetype<'w>(
        _fetch: &mut Self::Fetch<'w>,
        _state: &Self::State,
        _archetype: &'w Archetype,
        _table: &'w Table,
    ) {
    }

    #[inline]
    unsafe fn set_table<'w>(_fetch: &mut Self::Fetch<'w>, _state: &Self::State, _table: &'w Table) {
    }

    #[inline(always)]
    unsafe fn fetch<'w>(
        fetch: &mut Self::Fetch<'w>,
        entity: Entity,
        _table_row: TableRow,
    ) -> Self::Item<'w> {
        NodeMut::try_new(*fetch, entity).unwrap()
    }

    fn update_component_access(_state: &Self::State, access: &mut FilteredAccess<ComponentId>) {
        assert!(
            !access.access().has_any_read(),
            "NodeMut conflicts with a previous access in this query. Exclusive access cannot coincide with any other accesses."
        );

        access.write_all()
    }

    fn update_archetype_component_access(
        _state: &Self::State,
        archetype: &Archetype,
        access: &mut Access<ArchetypeComponentId>,
    ) {
        for component_id in archetype.components() {
            access.add_write(archetype.get_archetype_component_id(component_id).unwrap());
        }
    }

    fn init_state(world: &mut World) -> Self::State {
        world.init_component::<Node>()
    }

    fn matches_component_set(
        state: &Self::State,
        set_contains_id: &impl Fn(ComponentId) -> bool,
    ) -> bool {
        set_contains_id(*state)
    }
}

macro_rules! impl_node_kind_query {
    ($($mut_name:ident, $ro_name:ident, $kind:ident);*) => {
        $(
            unsafe impl<'a> WorldQuery for $mut_name<'a> {
                type Fetch<'w> = UnsafeWorldCell<'w>;
                type Item<'w> = $mut_name<'w>;
                type ReadOnly = $ro_name<'a>;
                type State = ComponentId; // The node component ID

                fn shrink<'wlong: 'wshort, 'wshort>(item: Self::Item<'wlong>) -> Self::Item<'wshort> {
                    item
                }

                const IS_DENSE: bool = true;
                const IS_ARCHETYPAL: bool = true;

                unsafe fn init_fetch<'w>(
                    world: UnsafeWorldCell<'w>,
                    _state: &Self::State,
                    _last_run: Tick,
                    _this_run: Tick,
                ) -> Self::Fetch<'w> {
                    world
                }

                #[inline]
                unsafe fn set_archetype<'w>(
                    _fetch: &mut Self::Fetch<'w>,
                    _state: &Self::State,
                    _archetype: &'w Archetype,
                    _table: &'w Table,
                ) {
                }

                #[inline]
                unsafe fn set_table<'w>(_fetch: &mut Self::Fetch<'w>, _state: &Self::State, _table: &'w Table) {
                }

                #[inline(always)]
                unsafe fn fetch<'w>(
                    fetch: &mut Self::Fetch<'w>,
                    entity: Entity,
                    _table_row: TableRow,
                ) -> Self::Item<'w> {
                    let node = NodeMut::try_new(*fetch, entity).unwrap();
                    assert_eq!(*node.get::<NodeKind>().unwrap(), NodeKind::$kind);
                    $mut_name(node)
                }

                fn update_component_access(_state: &Self::State, access: &mut FilteredAccess<ComponentId>) {
                    assert!(
                        !access.access().has_any_read(),
                        "NodeMut conflicts with a previous access in this query. Exclusive access cannot coincide with any other accesses."
                    );

                    access.write_all()
                }

                fn update_archetype_component_access(
                    _state: &Self::State,
                    archetype: &Archetype,
                    access: &mut Access<ArchetypeComponentId>,
                ) {
                    for component_id in archetype.components() {
                        access.add_write(archetype.get_archetype_component_id(component_id).unwrap());
                    }
                }

                fn init_state(world: &mut World) -> Self::State {
                    world.init_component::<Node>()
                }

                fn matches_component_set(
                    state: &Self::State,
                    set_contains_id: &impl Fn(ComponentId) -> bool,
                ) -> bool {
                    set_contains_id(*state)
                }

                unsafe fn filter_fetch(
                    fetch: &mut Self::Fetch<'_>,
                    entity: Entity,
                    _table_row: TableRow,
                ) -> bool {
                    let node = NodeMut::try_new(*fetch, entity).unwrap();
                    *node.get::<NodeKind>().unwrap() == NodeKind::$kind
                }

            }

            unsafe impl<'a> ReadOnlyWorldQuery for $ro_name<'a> {}

            unsafe impl<'a> WorldQuery for $ro_name<'a> {
                type Fetch<'w> = UnsafeWorldCell<'w>;
                type Item<'w> = $ro_name<'w>;
                type ReadOnly = Self;
                type State = ComponentId; // The node component ID

                fn shrink<'wlong: 'wshort, 'wshort>(item: Self::Item<'wlong>) -> Self::Item<'wshort> {
                    item
                }

                const IS_DENSE: bool = true;
                const IS_ARCHETYPAL: bool = true;

                unsafe fn init_fetch<'w>(
                    world: UnsafeWorldCell<'w>,
                    _state: &Self::State,
                    _last_run: Tick,
                    _this_run: Tick,
                ) -> Self::Fetch<'w> {
                    world
                }

                #[inline]
                unsafe fn set_archetype<'w>(
                    _fetch: &mut Self::Fetch<'w>,
                    _state: &Self::State,
                    _archetype: &'w Archetype,
                    _table: &'w Table,
                ) {
                }

                #[inline]
                unsafe fn set_table<'w>(_fetch: &mut Self::Fetch<'w>, _state: &Self::State, _table: &'w Table) {
                }

                #[inline(always)]
                unsafe fn fetch<'w>(
                    fetch: &mut Self::Fetch<'w>,
                    entity: Entity,
                    _table_row: TableRow,
                ) -> Self::Item<'w> {
                    let node = NodeRef::try_new(*fetch, entity).unwrap();
                    assert_eq!(*node.get::<NodeKind>().unwrap(), NodeKind::$kind);
                    $ro_name(node)
                }

                fn update_component_access(_state: &Self::State, access: &mut FilteredAccess<ComponentId>) {
                    assert!(
                        !access.access().has_any_read(),
                        "NodeMut conflicts with a previous access in this query. Exclusive access cannot coincide with any other accesses."
                    );

                    access.write_all()
                }

                fn update_archetype_component_access(
                    _state: &Self::State,
                    archetype: &Archetype,
                    access: &mut Access<ArchetypeComponentId>,
                ) {
                    for component_id in archetype.components() {
                        access.add_write(archetype.get_archetype_component_id(component_id).unwrap());
                    }
                }

                fn init_state(world: &mut World) -> Self::State {
                    world.init_component::<Node>()
                }

                fn matches_component_set(
                    state: &Self::State,
                    set_contains_id: &impl Fn(ComponentId) -> bool,
                ) -> bool {
                    set_contains_id(*state)
                }

                unsafe fn filter_fetch(
                    fetch: &mut Self::Fetch<'_>,
                    entity: Entity,
                    _table_row: TableRow,
                ) -> bool {
                    let node = NodeRef::try_new(*fetch, entity).unwrap();
                    *node.get::<NodeKind>().unwrap() == NodeKind::$kind
                }
            }
        )*
    }
}

impl_node_kind_query! {
    ImageNodeMut, ImageNodeRef, Image;
    TextNodeMut, TextNodeRef, Text;
    LayoutNodeMut, LayoutNodeRef, Layout
}
