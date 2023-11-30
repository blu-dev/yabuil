use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
};

use bevy::{
    asset::{Asset, AssetLoader, AsyncReadExt, Handle, VisitAssetDependencies},
    math::{UVec2, Vec2},
    reflect::TypePath,
    render::texture::Image,
    text::{Font, TextAlignment},
};
use serde::{Deserialize, Serialize};

use crate::{
    animation::Animations, node::Anchor, LayoutAttribute, LayoutRegistryInner,
    RestrictedLoadContext,
};
use thiserror::Error;

mod deserialize_animation;
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
                LayoutNodeData::Image(data) => visit(data.handle.id().untyped()),
                LayoutNodeData::Text(data) => visit(data.handle.id().untyped()),
                LayoutNodeData::Layout { handle, .. } => visit(handle.id().untyped()),
            }

            for attribute in node.attributes.iter() {
                attribute.visit_dependencies(visit);
            }
        }

        for node_anim in self.animations.0.values().flat_map(|anim| anim.iter()) {
            node_anim.target.visit_dependencies(visit);
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
    pub inner: LayoutNodeData,

    /// User-space attributes for each node
    pub(crate) attributes: Vec<Box<dyn LayoutAttribute>>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct ImageNodeData {
    pub path: Option<PathBuf>,
    #[serde(default)]
    pub tint: Option<[f32; 4]>,
    #[serde(skip)]
    pub handle: Handle<Image>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct TextNodeData {
    pub text: String,
    pub size: f32,
    pub color: [f32; 4],
    #[serde(default)]
    pub font: Option<PathBuf>,
    #[serde(skip)]
    pub handle: Handle<Font>,
    #[serde(default)]
    pub alignment: TextAlignment,
}

/// First-class node data, guaranteed to be supported by yabuil
#[derive(Deserialize, Serialize, Default)]
#[serde(tag = "node_kind", content = "node_data")]
pub enum LayoutNodeData {
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
    Layout {
        path: PathBuf,

        #[serde(skip)]
        handle: Handle<Layout>,
    },
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

            let mut layout: Layout =
                deserialize_layout::deserialize_layout(&bytes, &self.0.read().unwrap())?;

            let mut context = RestrictedLoadContext { load_context };

            for node in layout.nodes.iter_mut() {
                match &mut node.inner {
                    LayoutNodeData::Null => {}
                    LayoutNodeData::Image(data) => {
                        if let Some(path) = data.path.as_ref() {
                            data.handle = context.load(path.clone());
                        }
                    }
                    LayoutNodeData::Text(data) => {
                        if let Some(font) = data.font.as_ref() {
                            data.handle = context.load(font.clone())
                        }
                    }
                    LayoutNodeData::Layout { handle, path } => *handle = context.load(path.clone()),
                }

                for attribute in node.attributes.iter_mut() {
                    attribute.initialize_dependencies(&mut context);
                }
            }

            let animations = Arc::get_mut(&mut layout.animations.0)
                .expect("There should only be one reference to the animation map during loading");

            for animation in animations.values_mut() {
                for node_anim in animation.iter_mut() {
                    node_anim.target.initialize_dependencies(&mut context);
                }
            }

            Ok(layout)
        })
    }
}
