use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
};

use bevy::{
    asset::{Asset, AssetLoader, AsyncReadExt, Handle, VisitAssetDependencies},
    math::{vec2, UVec2, Vec2},
    reflect::TypePath,
    render::texture::Image,
    sprite::Anchor,
    text::{Font, TextAlignment},
};
use serde::{Deserialize, Serialize};

use crate::{
    animation::Animations, AttributeRegistryInner, LayoutAttribute, RestrictedLoadContext,
};
use thiserror::Error;

mod deserialize_layout;

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
    pub animations: Animations,
}

impl Asset for Layout {}

impl VisitAssetDependencies for Layout {
    fn visit_dependencies(&self, visit: &mut impl FnMut(bevy::asset::UntypedAssetId)) {
        for node in self.nodes.iter() {
            match &node.inner {
                LayoutNodeData::Null => {}
                LayoutNodeData::Image { handle, .. } => visit(handle.id().untyped()),
                LayoutNodeData::Text { handle, .. } => visit(handle.id().untyped()),
                LayoutNodeData::Layout { handle, .. } => visit(handle.id().untyped()),
            }

            for attribute in node.attributes.iter() {
                attribute.visit_dependencies(visit);
            }
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

    /// Which part of this node to attach to the position
    pub anchor: NodeAnchor,

    /// Built-in supported node data for this node.
    ///
    /// These can be things like images, text, etc.
    pub inner: LayoutNodeData,

    /// User-space attributes for each node
    pub attributes: Vec<Box<dyn LayoutAttribute>>,
}

impl LayoutNode {
    pub fn get_anchored_origin(&self) -> Vec2 {
        self.anchor.calculate_origin(self.position, self.size)
    }

    pub fn get_anchored_center(&self) -> Vec2 {
        self.anchor.calculate_center(self.position, self.size)
    }
}

/// Which part of the node to attach to the position
#[derive(Deserialize, Serialize, Copy, Clone, PartialEq, Eq, Default)]
pub enum NodeAnchor {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    #[default]
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl From<NodeAnchor> for Anchor {
    fn from(value: NodeAnchor) -> Self {
        match value {
            NodeAnchor::TopLeft => Anchor::TopLeft,
            NodeAnchor::TopCenter => Anchor::TopCenter,
            NodeAnchor::TopRight => Anchor::TopRight,
            NodeAnchor::CenterLeft => Anchor::CenterLeft,
            NodeAnchor::Center => Anchor::Center,
            NodeAnchor::CenterRight => Anchor::CenterRight,
            NodeAnchor::BottomLeft => Anchor::BottomLeft,
            NodeAnchor::BottomCenter => Anchor::BottomCenter,
            NodeAnchor::BottomRight => Anchor::BottomRight,
        }
    }
}

impl NodeAnchor {
    pub fn calculate_origin(&self, position: Vec2, size: Vec2) -> Vec2 {
        let half_size = size / 2.0;

        match self {
            Self::TopLeft => position,
            Self::TopCenter => position - Vec2::X * half_size,
            Self::TopRight => position - Vec2::X * size,
            Self::CenterLeft => position - Vec2::Y * half_size,
            Self::Center => position - half_size,
            Self::CenterRight => position - vec2(size.x, half_size.y),
            Self::BottomLeft => position - Vec2::Y * size,
            Self::BottomCenter => position - vec2(half_size.x, size.y),
            Self::BottomRight => position - size,
        }
    }

    pub fn calculate_center(&self, position: Vec2, size: Vec2) -> Vec2 {
        let half_size = size / 2.0;

        match self {
            Self::TopLeft => position + half_size,
            Self::TopCenter => position + Vec2::Y * half_size,
            Self::TopRight => position + vec2(-half_size.x, half_size.y),
            Self::CenterLeft => position + Vec2::X * half_size,
            Self::Center => position,
            Self::CenterRight => position - Vec2::X * half_size,
            Self::BottomLeft => position + vec2(half_size.x, -half_size.y),
            Self::BottomCenter => position - Vec2::Y * half_size,
            Self::BottomRight => position - half_size,
        }
    }
}

/// First-class node data, guaranteed to be supported by yabuil
#[derive(Deserialize, Serialize)]
#[serde(tag = "node_kind", content = "node_data")]
pub enum LayoutNodeData {
    /// This node should be treated like a blank slate
    ///
    /// Entities for `Null` nodes are spawned with exclusively node metadata and a [`TransformBundle`](bevy::prelude::TransformBundle)
    /// and [`VisibilityBundle`](bevy::prelude::VisibilityBundle).
    Null,

    /// This node should be treated like an image
    ///
    /// Entities for `Image` nodes are spawned with a [`SpriteBundle`](bevy::prelude::SpriteBundle)
    Image {
        path: PathBuf,
        #[serde(skip)]
        handle: Handle<Image>,
    },

    /// This node contains a bounded text area
    ///
    /// The `size` field on this node is treated as a bounding area for a [`TextBundle`](bevy::prelude::TextBundle).
    Text {
        text: String,
        size: f32,
        color: [f32; 4],
        #[serde(default)]
        font: Option<PathBuf>,
        #[serde(skip)]
        handle: Handle<Font>,
        #[serde(default)]
        alignment: TextAlignment,
    },

    /// This node reuses another layout from another file
    Layout {
        path: PathBuf,

        #[serde(skip)]
        handle: Handle<Layout>,
    },
}

pub(crate) struct LayoutLoader(pub(crate) Arc<RwLock<AttributeRegistryInner>>);

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

            let mut layout: Layout =
                deserialize_layout::deserialize_layout(&bytes, &self.0.read().unwrap())?;

            let mut context = RestrictedLoadContext { load_context };

            for node in layout.nodes.iter_mut() {
                match &mut node.inner {
                    LayoutNodeData::Null => {}
                    LayoutNodeData::Image { handle, path } => *handle = context.load(path.clone()),
                    LayoutNodeData::Text { handle, font, .. } => {
                        if let Some(font) = font.as_ref() {
                            *handle = context.load(font.clone())
                        }
                    }
                    LayoutNodeData::Layout { handle, path } => *handle = context.load(path.clone()),
                }

                for attribute in node.attributes.iter_mut() {
                    attribute.initialize_dependencies(&mut context);
                }
            }

            Ok(layout)
        })
    }
}