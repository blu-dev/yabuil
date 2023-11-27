use std::ops::DerefMut;

use bevy::prelude::*;

use crate::{
    animation::{AnimationPlayerState, Animations},
    components::{LayoutNodeMetadata, NodeKind},
    ComputedLayoutNodeMetadata,
};

#[derive(Copy, Clone)]
pub struct NodeView<'a>(EntityRef<'a>);

impl<'a> NodeView<'a> {
    pub fn new(entity: EntityRef<'a>) -> Option<Self> {
        entity
            .contains::<ComputedLayoutNodeMetadata>()
            .then_some(Self(entity))
    }

    pub fn as_entity(&self) -> EntityRef<'a> {
        self.0.clone()
    }

    pub fn metadata(&self) -> &LayoutNodeMetadata {
        self.0
            .get::<LayoutNodeMetadata>()
            .expect("NodeView should always have access to LayoutNodeMetadata")
    }

    pub fn computed_metadata(&self) -> &ComputedLayoutNodeMetadata {
        self.0
            .get::<ComputedLayoutNodeMetadata>()
            .expect("NodeView should always have access to ComputedLayoutNodeMetadata")
    }

    pub fn as_text_node(&self) -> Option<TextNodeView<'a>> {
        (self.computed_metadata().kind() == NodeKind::Text).then_some(TextNodeView(self.clone()))
    }

    pub fn as_image_node(&self) -> Option<ImageNodeView<'a>> {
        (self.computed_metadata().kind() == NodeKind::Image).then_some(ImageNodeView(self.clone()))
    }

    pub fn as_layout_node(&self) -> Option<LayoutNodeView<'a>> {
        (self.computed_metadata().kind() == NodeKind::Layout)
            .then_some(LayoutNodeView(self.clone()))
    }
}

pub struct NodeViewMut<'a>(EntityMut<'a>);

impl<'a> NodeViewMut<'a> {
    pub fn new(entity: EntityMut<'a>) -> Option<Self> {
        entity
            .contains::<ComputedLayoutNodeMetadata>()
            .then_some(Self(entity))
    }

    pub fn metadata(&self) -> &LayoutNodeMetadata {
        self.0
            .get::<LayoutNodeMetadata>()
            .expect("NodeViewMut should always have access to LayoutNodeMetadata")
    }

    pub fn metadata_mut(&mut self) -> Mut<'_, LayoutNodeMetadata> {
        self.0
            .get_mut::<LayoutNodeMetadata>()
            .expect("NodeViewMut should always have access to LayoutNodeMetadata")
    }

    pub fn computed_metadata(&self) -> &ComputedLayoutNodeMetadata {
        self.0
            .get::<ComputedLayoutNodeMetadata>()
            .expect("NodeViewMut should always have access to ComputedLayoutNodeMetadata")
    }

    pub fn as_readonly<'b>(&'b self) -> NodeView<'b> {
        NodeView(self.as_entity())
    }

    pub fn as_entity<'b>(&'b self) -> EntityRef<'b> {
        self.0.as_readonly()
    }

    pub fn as_entity_mut<'b>(&'b mut self) -> EntityMut<'b> {
        self.0.reborrow()
    }

    pub fn reborrow<'b>(&'b mut self) -> NodeViewMut<'b> {
        NodeViewMut(self.0.reborrow())
    }

    pub fn as_text_node<'b>(&'b self) -> Option<TextNodeView<'b>> {
        self.as_readonly().as_text_node()
    }

    pub fn as_image_node<'b>(&'b self) -> Option<ImageNodeView<'b>> {
        self.as_readonly().as_image_node()
    }

    pub fn as_layout_node<'b>(&'b self) -> Option<LayoutNodeView<'b>> {
        self.as_readonly().as_layout_node()
    }

    pub fn as_text_node_mut<'b>(&'b mut self) -> Option<TextNodeViewMut<'b>> {
        (self.computed_metadata().kind() == NodeKind::Text)
            .then(|| TextNodeViewMut(self.reborrow()))
    }

    pub fn as_image_node_mut<'b>(&'b mut self) -> Option<ImageNodeViewMut<'b>> {
        (self.computed_metadata().kind() == NodeKind::Image)
            .then(|| ImageNodeViewMut(self.reborrow()))
    }

    pub fn as_layout_node_mut<'b>(&'b mut self) -> Option<LayoutNodeViewMut<'b>> {
        (self.computed_metadata().kind() == NodeKind::Layout)
            .then(|| LayoutNodeViewMut(self.reborrow()))
    }
}

pub struct NodeWorldView<'a> {
    entity: EntityRef<'a>,
    world: &'a World,
}

impl<'a> NodeWorldView<'a> {
    pub fn new(entity: Entity, world: &'a World) -> Option<Self> {
        let entity = world.entity(entity);
        entity
            .contains::<ComputedLayoutNodeMetadata>()
            .then_some(Self { entity, world })
    }

    pub fn world(&self) -> &'a World {
        self.world
    }

    pub fn as_entity(&self) -> EntityRef<'a> {
        self.entity.clone()
    }

    pub fn as_node_view(&self) -> NodeView<'a> {
        NodeView(self.entity)
    }

    pub fn metadata(&self) -> &LayoutNodeMetadata {
        self.entity
            .get::<LayoutNodeMetadata>()
            .expect("NodeWorldView should always have access to LayoutNodeMetadata")
    }

    pub fn computed_metadata(&self) -> &ComputedLayoutNodeMetadata {
        self.entity
            .get::<ComputedLayoutNodeMetadata>()
            .expect("NodeWorldView should always have access to LayoutNodeMetadata")
    }

    pub fn as_text_node(&self) -> Option<TextNodeView<'a>> {
        (self.computed_metadata().kind() == NodeKind::Text)
            .then(|| TextNodeView(self.as_node_view()))
    }

    pub fn as_image_node(&self) -> Option<ImageNodeView<'a>> {
        (self.computed_metadata().kind() == NodeKind::Image)
            .then(|| ImageNodeView(self.as_node_view()))
    }

    pub fn as_layout_node(&self) -> Option<LayoutNodeView<'a>> {
        (self.computed_metadata().kind() == NodeKind::Layout)
            .then(|| LayoutNodeView(self.as_node_view()))
    }

    pub fn parent(&self) -> Option<NodeWorldView<'a>> {
        let parent = self.entity.get::<Parent>()?;
        NodeWorldView::new(parent.get(), self.world)
    }

    pub fn sibling(&self, id: impl AsRef<str>) -> Option<NodeWorldView<'a>> {
        let parent = self.parent()?;
        let children = parent.as_node_view().as_entity().get::<Children>()?;
        for child in children.iter() {
            let Some(child) = NodeWorldView::new(*child, self.world) else {
                continue;
            };

            if child.computed_metadata().id().name() == id.as_ref() {
                return Some(child);
            }
        }

        None
    }
}

pub struct NodeWorldViewMut<'a>(EntityWorldMut<'a>);

impl<'a> NodeWorldViewMut<'a> {
    pub fn new(entity: EntityWorldMut<'a>) -> Option<Self> {
        entity
            .contains::<ComputedLayoutNodeMetadata>()
            .then_some(Self(entity))
    }

    pub fn world<'b>(&'b self) -> &'b World {
        self.0.world()
    }

    pub fn as_entity<'b>(&'b self) -> EntityRef<'b> {
        EntityRef::from(&self.0)
    }

    pub fn as_entity_mut<'b>(&'b mut self) -> EntityMut<'b> {
        EntityMut::from(&mut self.0)
    }

    pub fn as_entity_world_mut(&mut self) -> &mut EntityWorldMut<'a> {
        &mut self.0
    }

    pub fn as_node_view<'b>(&'b self) -> NodeView<'b> {
        NodeView(self.as_entity())
    }

    pub fn as_node_view_mut<'b>(&'b mut self) -> NodeViewMut<'b> {
        NodeViewMut(self.as_entity_mut())
    }

    pub fn metadata(&self) -> &LayoutNodeMetadata {
        self.0
            .get::<LayoutNodeMetadata>()
            .expect("NodeWorldViewMut should always have access to LayoutNodeMetadata")
    }

    pub fn metadata_mut(&mut self) -> Mut<'_, LayoutNodeMetadata> {
        self.0
            .get_mut::<LayoutNodeMetadata>()
            .expect("NodeWorldViewMut should always have access to LayoutNodeMetadata")
    }

    pub fn computed_metadata(&self) -> &ComputedLayoutNodeMetadata {
        self.0
            .get::<ComputedLayoutNodeMetadata>()
            .expect("NodeView should always have access to LayoutNodeMetadata")
    }

    pub fn as_text_node<'b>(&'b self) -> Option<TextNodeView<'b>> {
        (self.computed_metadata().kind() == NodeKind::Text)
            .then(|| TextNodeView(self.as_node_view()))
    }

    pub fn as_image_node<'b>(&'b self) -> Option<ImageNodeView<'b>> {
        (self.computed_metadata().kind() == NodeKind::Image)
            .then(|| ImageNodeView(self.as_node_view()))
    }

    pub fn as_layout_node<'b>(&'b self) -> Option<LayoutNodeView<'b>> {
        (self.computed_metadata().kind() == NodeKind::Layout)
            .then(|| LayoutNodeView(self.as_node_view()))
    }

    pub fn as_text_node_mut<'b>(&'b mut self) -> Option<TextNodeViewMut<'b>> {
        (self.computed_metadata().kind() == NodeKind::Text)
            .then(|| TextNodeViewMut(self.as_node_view_mut()))
    }

    pub fn as_image_node_mut<'b>(&'b mut self) -> Option<ImageNodeViewMut<'b>> {
        (self.computed_metadata().kind() == NodeKind::Image)
            .then(|| ImageNodeViewMut(self.as_node_view_mut()))
    }

    pub fn as_layout_node_mut<'b>(&'b mut self) -> Option<LayoutNodeViewMut<'b>> {
        (self.computed_metadata().kind() == NodeKind::Layout)
            .then(|| LayoutNodeViewMut(self.as_node_view_mut()))
    }

    pub fn parent<'b>(&'b self) -> Option<NodeWorldView<'b>> {
        let parent = self.as_entity().get::<Parent>()?;
        NodeWorldView::new(parent.get(), self.0.world())
    }

    pub fn parent_scope<R>(&mut self, f: impl FnOnce(Option<NodeWorldViewMut<'_>>) -> R) -> R {
        let Some(parent) = self.as_entity().get::<Parent>().map(|p| p.get()) else {
            return f(None);
        };

        self.0
            .world_scope(|world| f(NodeWorldViewMut::new(world.entity_mut(parent))))
    }

    pub fn sibling<'b>(&'b self, id: impl AsRef<str>) -> Option<NodeWorldView<'b>> {
        let parent = self.parent()?;
        let children = parent.as_entity().get::<Children>()?;
        for child in children.iter() {
            let Some(child) = NodeWorldView::new(*child, self.0.world()) else {
                continue;
            };

            if child.computed_metadata().id().name() == id.as_ref() {
                return Some(child);
            }
        }

        None
    }

    pub fn sibling_scope<R>(
        &mut self,
        id: impl AsRef<str>,
        f: impl FnOnce(Option<NodeWorldViewMut<'_>>) -> R,
    ) -> R {
        let Some(parent) = self.as_entity().get::<Parent>().map(|p| p.get()) else {
            return f(None);
        };

        self.0.world_scope(|world| {
            let parent = world.entity(parent);

            let Some(children) = parent.get::<Children>() else {
                return f(None);
            };

            let id = id.as_ref();
            let sibling = children.iter().copied().find(|entity| {
                let Some(child) = NodeView::new(world.entity(*entity)) else {
                    return false;
                };

                child.computed_metadata().id().name() == id
            });

            if let Some(sibling) = sibling {
                f(NodeWorldViewMut::new(world.entity_mut(sibling)))
            } else {
                f(None)
            }
        })
    }

    pub fn child<'b>(&'b self, id: impl AsRef<str>) -> Option<NodeWorldView<'b>> {
        let children = self.as_entity().get::<Children>()?;

        let id = id.as_ref();

        for child in children.iter().copied() {
            let Some(child) = NodeWorldView::new(child, self.world()) else {
                continue;
            };

            if child.computed_metadata().id().name() == id {
                return Some(child);
            }
        }

        None
    }

    pub fn child_scope<R>(
        &mut self,
        id: impl AsRef<str>,
        f: impl FnOnce(Option<NodeWorldViewMut<'_>>) -> R,
    ) -> R {
        let Some(children) = self.as_entity().get::<Children>() else {
            return f(None);
        };

        let id = id.as_ref();

        let child = children.iter().copied().find(|entity| {
            let Some(child) = NodeView::new(self.world().entity(*entity)) else {
                return false;
            };

            child.computed_metadata().id().name() == id
        });

        if let Some(child) = child {
            self.as_entity_world_mut()
                .world_scope(|world| f(NodeWorldViewMut::new(world.entity_mut(child))))
        } else {
            f(None)
        }
    }
}

pub struct TextNodeView<'a>(NodeView<'a>);

impl<'a> TextNodeView<'a> {
    fn text_component(&self) -> &Text {
        self.0
            .as_entity()
            .get::<Text>()
            .expect("TextNode should have text")
    }

    pub fn text(&self) -> &str {
        self.text_component().sections[0].value.as_str()
    }

    pub fn color(&self) -> Color {
        self.text_component().sections[0].style.color
    }

    pub fn font(&self) -> AssetId<Font> {
        self.text_component().sections[0].style.font.id()
    }

    pub fn font_size(&self) -> f32 {
        self.text_component().sections[0].style.font_size
    }
}

pub struct TextNodeViewMut<'a>(NodeViewMut<'a>);

impl<'a> TextNodeViewMut<'a> {
    fn text_component(&self) -> &Text {
        self.0
            .as_entity()
            .get::<Text>()
            .expect("TextNode should have text")
    }

    fn update_text_component(&mut self, f: impl FnOnce(&mut Text)) {
        f(self
            .0
            .as_entity_mut()
            .get_mut::<Text>()
            .expect("TextNode should have text")
            .deref_mut())
    }

    pub fn text(&self) -> &str {
        self.text_component().sections[0].value.as_str()
    }

    pub fn set_text(&mut self, text: impl ToString) {
        self.update_text_component(move |comp| {
            comp.sections[0].value = text.to_string();
        });
    }

    pub fn color(&self) -> Color {
        self.text_component().sections[0].style.color
    }

    pub fn set_color(&mut self, color: Color) {
        self.update_text_component(move |comp| {
            comp.sections[0].style.color = color;
        });
    }

    pub fn font(&self) -> Handle<Font> {
        self.text_component().sections[0].style.font.clone()
    }

    pub fn set_font(&mut self, font: impl Into<Handle<Font>>) {
        self.update_text_component(move |comp| {
            comp.sections[0].style.font = font.into();
        });
    }

    pub fn font_size(&self) -> f32 {
        self.text_component().sections[0].style.font_size
    }

    pub fn set_font_size(&mut self, size: f32) {
        self.update_text_component(move |comp| comp.sections[0].style.font_size = size);
    }
}

pub struct ImageNodeView<'a>(NodeView<'a>);

impl<'a> ImageNodeView<'a> {
    pub fn image(&self) -> Handle<Image> {
        self.0
            .as_entity()
            .get::<Handle<Image>>()
            .expect("ImageNode should have image")
            .clone()
    }

    pub fn sprite(&self) -> &Sprite {
        self.0
            .as_entity()
            .get::<Sprite>()
            .expect("ImageNode should have sprite")
    }
}

pub struct ImageNodeViewMut<'a>(NodeViewMut<'a>);

impl<'a> ImageNodeViewMut<'a> {
    pub fn image(&self) -> Handle<Image> {
        self.0
            .as_entity()
            .get::<Handle<Image>>()
            .expect("ImageNode should have image")
            .clone()
    }

    pub fn sprite(&self) -> &Sprite {
        self.0
            .as_entity()
            .get::<Sprite>()
            .expect("ImageNode should have sprite")
    }

    pub fn set_image(&mut self, image: impl Into<Handle<Image>>) {
        *self
            .0
            .as_entity_mut()
            .get_mut::<Handle<Image>>()
            .expect("ImageNode should have image") = image.into();
    }

    pub fn update_sprite(&mut self, f: impl FnOnce(&mut Sprite)) {
        f(&mut *self
            .0
            .as_entity_mut()
            .get_mut::<Sprite>()
            .expect("ImageNode should have sprite"));
    }
}

pub struct LayoutNodeView<'a>(NodeView<'a>);

impl<'a> LayoutNodeView<'a> {
    pub fn has_animation(&self, animation: impl AsRef<str>) -> bool {
        self.0
            .as_entity()
            .get::<Animations>()
            .unwrap()
            .0
            .contains_key(animation.as_ref())
    }
}

pub struct LayoutNodeViewMut<'a>(NodeViewMut<'a>);

impl<'a> LayoutNodeViewMut<'a> {
    pub fn has_animation(&self, animation: impl AsRef<str>) -> bool {
        self.0
            .as_entity()
            .get::<Animations>()
            .unwrap()
            .0
            .contains_key(animation.as_ref())
    }

    pub fn play_animation(&mut self, animation: impl AsRef<str>) {
        *self
            .0
            .as_entity_mut()
            .get_mut::<AnimationPlayerState>()
            .unwrap() = AnimationPlayerState::Playing {
            animation: animation.as_ref().to_string(),
            time_elapsed_ms: 0.0,
        };
    }
}