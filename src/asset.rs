use std::{
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use bevy::{
    asset::{Asset, AssetLoader, AsyncReadExt, Handle, VisitAssetDependencies},
    math::{UVec2, Vec2},
    reflect::TypePath,
    render::{color::Color, texture::Image},
    text::{Font, TextAlignment},
};
use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    animation::LayoutAnimation, components::NodeKind, node::Anchor, DynamicAttribute,
    LayoutRegistryInner, RestrictedLoadContext,
};
use thiserror::Error;

mod deserialize_animation;
mod deserialize_layout;
mod helpers;

pub(crate) fn deserialize_color<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Color, D::Error> {
    let [r, g, b, a] = <[f32; 4]>::deserialize(deserializer)?;
    Ok(Color::rgba(r, g, b, a))
}

pub(crate) fn deserialize_color_opt<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Color>, D::Error> {
    if let Some([r, g, b, a]) = <Option<[f32; 4]>>::deserialize(deserializer)? {
        Ok(Some(Color::rgba(r, g, b, a)))
    } else {
        Ok(None)
    }
}

/// A collection of nodes with an associated coordinate system and resolution
#[derive(TypePath)]
pub struct Layout {
    /// The resolution to interpret this layout's coodinate system as
    ///
    /// If [`None`], default to `canvas_size`
    pub resolution: Option<UVec2>,

    /// The size of this layout.
    ///
    /// If this is the root node of the layout, then this layout's coordinate system
    /// gets scaled to the size of the render target.
    ///
    /// If this is a sublayout, then this layout's coordinate system gets scaled to the coordinate
    /// system of the root node
    pub canvas_size: UVec2,

    /// The nodes of the layout
    pub nodes: Vec<LayoutNode>,

    /// Animations associated with this layout
    pub animations: Vec<Handle<LayoutAnimation>>,
}

impl Layout {
    pub fn child_by_id(&self, id: impl AsRef<Path>) -> Option<&LayoutNode> {
        let mut nodes = &self.nodes;
        let path = id.as_ref();
        let count = path.components().count();
        'search: for (idx, id) in path.components().enumerate() {
            let id = id.as_os_str().to_str().unwrap();
            for node in nodes.iter() {
                if node.id != id {
                    continue;
                }

                if idx + 1 == count {
                    return Some(node);
                } else {
                    match &node.inner {
                        LayoutNodeInner::Group(group_data) => nodes = &group_data.nodes,
                        _ => return None,
                    }
                }

                continue 'search;
            }

            return None;
        }

        unreachable!()
    }

    pub fn child_by_id_mut(&mut self, id: impl AsRef<Path>) -> Option<&mut LayoutNode> {
        let mut nodes = &mut self.nodes;
        let path = id.as_ref();
        let count = path.components().count();
        'search: for (idx, id) in path.components().enumerate() {
            let id = id.as_os_str().to_str().unwrap();
            for node in nodes.iter_mut() {
                if node.id != id {
                    continue;
                }

                if idx + 1 == count {
                    return Some(node);
                } else {
                    match &mut node.inner {
                        LayoutNodeInner::Group(group_data) => nodes = &mut group_data.nodes,
                        _ => return None,
                    }
                }

                continue 'search;
            }

            return None;
        }

        unreachable!()
    }
}

impl Asset for Layout {}

fn visit_node_dependencies(node: &LayoutNode, visit: &mut impl FnMut(bevy::asset::UntypedAssetId)) {
    match &node.inner {
        LayoutNodeInner::Null => {}
        LayoutNodeInner::Image(data) => visit(data.handle.id().untyped()),
        LayoutNodeInner::Text(data) => visit(data.handle.id().untyped()),
        LayoutNodeInner::Layout(data) => visit(data.handle.id().untyped()),
        LayoutNodeInner::Group(data) => {
            for node in data.nodes.iter() {
                visit_node_dependencies(node, visit)
            }
        }
    }

    for attribute in node.attributes.iter() {
        attribute.visit_dependencies(visit);
    }
}

impl VisitAssetDependencies for Layout {
    fn visit_dependencies(&self, visit: &mut impl FnMut(bevy::asset::UntypedAssetId)) {
        for node in self.nodes.iter() {
            visit_node_dependencies(node, visit);
        }
    }
}

impl Layout {
    /// Gets the resolution of the layout, prioritizing the resolution (if it is explicitly defined)
    /// and falling back to the canvas size
    pub fn get_resolution(&self) -> UVec2 {
        self.resolution.unwrap_or(self.canvas_size)
    }
}

/// A single node in a layout
#[derive(Default)]
pub struct LayoutNode {
    /// The unique id of a node
    ///
    /// There can be multiple nodes throughout a layout tree with the same ID (for composability/
    /// reusability) but there cannot be multiple nodes in the same set of sibilings with the same ID.
    pub id: String,

    /// The position of this node
    ///
    /// This position is relative to the parent in the layout's resolution
    pub position: Vec2,

    /// The size of this node
    ///
    /// This size is in the layout's resolution
    pub size: Vec2,

    /// The rotation of this node
    ///
    /// This is in degrees, and will default to 0.0 when not present
    pub rotation: f32,

    /// Which part of this node to attach to the position
    pub anchor: Anchor,

    /// Built-in supported node data for this node.
    ///
    /// These can be things like images, text, etc.
    pub inner: LayoutNodeInner,

    /// User-space attributes for each node
    pub attributes: Vec<DynamicAttribute>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ImageNodeData {
    pub path: Option<PathBuf>,
    #[serde(default, deserialize_with = "deserialize_color_opt")]
    pub tint: Option<Color>,
    #[serde(skip)]
    pub handle: Handle<Image>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct TextNodeData {
    pub text: String,
    pub size: f32,
    #[serde(deserialize_with = "deserialize_color")]
    pub color: Color,
    #[serde(default)]
    pub font: Option<PathBuf>,
    #[serde(skip)]
    pub handle: Handle<Font>,
    #[serde(default)]
    pub alignment: TextAlignment,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct LayoutNodeData {
    pub path: PathBuf,
    #[serde(skip)]
    pub handle: Handle<Layout>,
}

#[derive(Default)]
pub struct GroupNodeData {
    pub child_anchor: Anchor,
    pub nodes: Vec<LayoutNode>,
}

/// First-class node data, guaranteed to be supported by yabuil
#[derive(Default)]
pub enum LayoutNodeInner {
    /// This node should be treated like a blank slate
    ///
    /// Entities for `Null` nodes are spawned with exclusively node metadata and a [`TransformBundle`](bevy::prelude::TransformBundle)
    /// and [`VisibilityBundle`](bevy::prelude::VisibilityBundle).
    #[default]
    Null,

    /// This node should be treated like an image
    ///
    /// Entities for `Image` nodes are spawned with a [`SpriteBundle`](bevy::prelude::SpriteBundle)
    Image(ImageNodeData),

    /// This node contains a bounded text area
    ///
    /// The `size` field on this node is treated as a bounding area for a [`TextBundle`](bevy::prelude::TextBundle).
    Text(TextNodeData),

    /// This node reuses another layout from another file
    Layout(LayoutNodeData),

    /// This is an inlined group of other nodes
    ///
    /// This should primarily be used to make animation easier
    Group(GroupNodeData),
}

impl LayoutNodeInner {
    pub fn node_kind(&self) -> NodeKind {
        match self {
            Self::Null => NodeKind::Null,
            Self::Image(_) => NodeKind::Image,
            Self::Text(_) => NodeKind::Text,
            Self::Layout(_) => NodeKind::Layout,
            Self::Group(_) => NodeKind::Group,
        }
    }
}

pub(crate) struct LayoutLoader(pub(crate) Arc<RwLock<LayoutRegistryInner>>);

#[derive(Error, Debug)]
pub enum LayoutError {
    #[error(transparent)]
    IO(#[from] std::io::Error),

    #[error(transparent)]
    JSON(#[from] serde_json::Error),
}

impl AssetLoader for LayoutLoader {
    type Asset = Layout;
    type Error = LayoutError;
    type Settings = ();

    fn extensions(&self) -> &[&str] {
        &["layout.json"]
    }

    fn load<'a>(
        &'a self,
        reader: &'a mut bevy::asset::io::Reader,
        _settings: &'a Self::Settings,
        load_context: &'a mut bevy::asset::LoadContext,
    ) -> bevy::utils::BoxedFuture<'a, Result<Self::Asset, Self::Error>> {
        Box::pin(async move {
            let mut bytes = vec![];
            reader.read_to_end(&mut bytes).await?;

            let mut layout: Layout = deserialize_layout::deserialize_layout(
                &bytes,
                &self.0.read().unwrap(),
                load_context,
            )?;

            let mut context = RestrictedLoadContext { load_context };

            for node in layout.nodes.iter_mut() {
                initialize_node(node, &mut context);
            }

            Ok(layout)
        })
    }
}

fn initialize_node(node: &mut LayoutNode, context: &mut RestrictedLoadContext<'_, '_>) {
    match &mut node.inner {
        LayoutNodeInner::Null => {}
        LayoutNodeInner::Image(data) => {
            if let Some(path) = data.path.as_ref() {
                data.handle = context.load(path.clone());
            }
        }
        LayoutNodeInner::Text(data) => {
            if let Some(font) = data.font.as_ref() {
                data.handle = context.load(font.clone())
            }
        }
        LayoutNodeInner::Layout(data) => data.handle = context.load(data.path.clone()),
        LayoutNodeInner::Group(group) => {
            for node in group.nodes.iter_mut() {
                initialize_node(node, context);
            }
        }
    }

    for attribute in node.attributes.iter_mut() {
        attribute.initialize_dependencies(context);
    }
}
