use bevy::{
    prelude::*,
    sprite::Anchor,
    text::{Text2dBounds, TextLayoutInfo},
};

use crate::{
    animation::AnimationPlayerState,
    asset::{ImageNodeData, Layout, LayoutNode, TextNodeData},
    node::{LayoutHandle, LayoutInfo, ZIndex},
    views::NodeWorldViewMut,
    LayoutId, LayoutNodeId,
};
use crate::{
    asset::{LayoutNodeData, LayoutNodeInner},
    node::Node,
};

use super::{NodeKind, SpawnLayoutError};

struct SpawnNodeContext<'a> {
    world: &'a mut World,
    assets: &'a Assets<Layout>,
    visitor: &'a mut dyn FnMut(&LayoutNode, NodeWorldViewMut),
    root: LayoutId,
    parent: LayoutNodeId,

    parent_layout: &'a Layout,
}

impl<'a> SpawnNodeContext<'a> {
    fn reborrow(&mut self, id: &str) -> SpawnNodeContext<'_> {
        SpawnNodeContext {
            world: self.world,
            assets: self.assets,
            visitor: self.visitor,
            root: self.root,
            parent: self.parent.join(id),
            parent_layout: self.parent_layout,
        }
    }

    fn reborrow_with_layout(&mut self, id: &str, layout: &'a Layout) -> SpawnNodeContext<'_> {
        SpawnNodeContext {
            world: self.world,
            assets: self.assets,
            visitor: self.visitor,
            root: self.root,
            parent: self.parent.join(id),
            parent_layout: layout,
        }
    }
}

fn spawn_null_node(context: SpawnNodeContext<'_>, node: &LayoutNode) -> Entity {
    context
        .world
        .spawn((
            TransformBundle::default(),
            VisibilityBundle::default(),
            Node::new_from_layout_node(node),
            NodeKind::Null,
            context.root,
            context.parent.join(node.id.as_str()),
            ZIndex::default(),
        ))
        .id()
}

fn spawn_image_node(
    context: SpawnNodeContext<'_>,
    node: &LayoutNode,
    image: &ImageNodeData,
) -> Entity {
    context
        .world
        .spawn((
            TransformBundle::default(),
            VisibilityBundle::default(),
            Node::new_from_layout_node(node),
            NodeKind::Image,
            context.root,
            context.parent.join(node.id.as_str()),
            ZIndex::default(),
            Sprite {
                color: image.tint.unwrap_or(Color::WHITE),
                custom_size: Some(node.size),
                ..default()
            },
            image.handle.clone(),
        ))
        .id()
}

fn spawn_text_node(
    context: SpawnNodeContext<'_>,
    node: &LayoutNode,
    text: &TextNodeData,
) -> Entity {
    let text_anchor = match text.alignment {
        TextAlignment::Left => Anchor::CenterLeft,
        TextAlignment::Center => Anchor::Center,
        TextAlignment::Right => Anchor::CenterRight,
    };

    context
        .world
        .spawn((
            TransformBundle::default(),
            VisibilityBundle::default(),
            Node::new_from_layout_node(node),
            NodeKind::Text,
            context.root,
            context.parent.join(node.id.as_str()),
            ZIndex::default(),
            Text::from_section(
                text.text.clone(),
                TextStyle {
                    font: text.handle.clone(),
                    font_size: text.size,
                    color: text.color,
                },
            ),
            text_anchor,
            Text2dBounds { size: node.size },
            TextLayoutInfo::default(),
        ))
        .id()
}

fn spawn_layout_node(
    mut context: SpawnNodeContext<'_>,
    node: &LayoutNode,
    layout: &LayoutNodeData,
) -> Result<Entity, SpawnLayoutError> {
    let asset = context
        .assets
        .get(layout.handle.id())
        .ok_or(SpawnLayoutError::NotLoaded)?;

    let parent = context
        .world
        .spawn((
            TransformBundle::default(),
            VisibilityBundle::default(),
            Node::new_from_layout_node(node),
            NodeKind::Layout,
            context.root,
            context.parent.join(node.id.as_str()),
            ZIndex::default(),
            LayoutInfo {
                resolution_scale: context.parent_layout.get_resolution().as_vec2()
                    / asset.get_resolution().as_vec2(),
                canvas_size: asset.canvas_size.as_vec2(),
            },
            LayoutHandle(layout.handle.clone()),
            AnimationPlayerState::NotPlaying,
            asset.animations.clone(),
        ))
        .id();

    let mut children = vec![];

    let parent_id = node.id.as_str();

    for node in asset.nodes.iter() {
        let child = spawn_node(context.reborrow_with_layout(parent_id, asset), node)?;
        context.world.entity_mut(parent).add_child(child);
        children.push(child);
    }

    for (node, child) in asset.nodes.iter().zip(children.into_iter()) {
        let child = NodeWorldViewMut::new(context.world.entity_mut(child))
            .expect("freshly spawned node should be viewable");
        (context.visitor)(node, child);
    }

    Ok(parent)
}

fn spawn_group_node(
    mut context: SpawnNodeContext<'_>,
    node: &LayoutNode,
    group: &[LayoutNode],
) -> Result<Entity, SpawnLayoutError> {
    let parent = context
        .world
        .spawn((
            TransformBundle::default(),
            VisibilityBundle::default(),
            Node::new_from_layout_node(node),
            NodeKind::Group,
            context.root,
            context.parent.join(node.id.as_str()),
            ZIndex::default(),
            LayoutInfo {
                resolution_scale: Vec2::ONE,
                canvas_size: node.size,
            },
        ))
        .id();

    let mut children = vec![];

    let parent_id = node.id.as_str();

    for node in group.iter() {
        let child = spawn_node(context.reborrow(parent_id), node)?;
        context.world.entity_mut(parent).add_child(child);
        children.push(child);
    }

    for (node, child) in group.iter().zip(children.into_iter()) {
        let child = NodeWorldViewMut::new(context.world.entity_mut(child))
            .expect("freshly spawned node should be viewable");
        (context.visitor)(node, child);
    }

    Ok(parent)
}

fn spawn_node(
    context: SpawnNodeContext<'_>,
    node: &LayoutNode,
) -> Result<Entity, SpawnLayoutError> {
    let entity = match &node.inner {
        LayoutNodeInner::Null => spawn_null_node(context, node),
        LayoutNodeInner::Image(image) => spawn_image_node(context, node, image),
        LayoutNodeInner::Text(text) => spawn_text_node(context, node, text),
        LayoutNodeInner::Layout(layout) => spawn_layout_node(context, node, layout)?,
        LayoutNodeInner::Group(group) => spawn_group_node(context, node, group)?,
    };

    Ok(entity)
}

pub fn spawn_layout(
    world: &mut World,
    root: Entity,
    handle: Handle<Layout>,
    mut visitor: impl FnMut(&LayoutNode, NodeWorldViewMut),
) -> Result<(), SpawnLayoutError> {
    world.resource_scope::<Assets<Layout>, _>(|world, assets| {
        let asset = assets.get(handle.id()).ok_or(SpawnLayoutError::NotLoaded)?;

        world.entity_mut(root).insert((
            Node {
                anchor: crate::node::Anchor::TopLeft,
                position: Vec2::ZERO,
                size: asset.canvas_size.as_vec2(),
                rotation: 0.0,
            },
            NodeKind::Layout,
            LayoutId(root),
            LayoutNodeId::root(),
            ZIndex::default(),
            LayoutInfo {
                resolution_scale: Vec2::ONE,
                canvas_size: asset.canvas_size.as_vec2(),
            },
            LayoutHandle(handle.clone()),
            AnimationPlayerState::NotPlaying,
        ));

        let mut children = vec![];
        for node in asset.nodes.iter() {
            let child = spawn_node(
                SpawnNodeContext {
                    world,
                    assets: &assets,
                    visitor: &mut visitor,
                    root: LayoutId(root),
                    parent: LayoutNodeId::root(),
                    parent_layout: asset,
                },
                node,
            )?;

            world.entity_mut(root).add_child(child);
            children.push(child);
        }

        for (node, child) in asset.nodes.iter().zip(children.into_iter()) {
            let child = NodeWorldViewMut::new(world.entity_mut(child))
                .expect("freshly spawned node should be viewable");
            (visitor)(node, child);
        }

        Ok(())
    })
}
