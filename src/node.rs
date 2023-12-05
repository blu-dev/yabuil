use bevy::{
    ecs::query::WorldQuery,
    math::vec2,
    prelude::*,
    render::camera::{ManualTextureViews, RenderTarget},
    utils::HashSet,
    window::{PrimaryWindow, WindowRef},
};
use serde::{Deserialize, Serialize};

use crate::{
    asset::{Layout, LayoutNode},
    components::{NodeKind, RootNode},
    LayoutId,
};

/// The Z Index of a node.
///
/// This gets translated into the transform system by multiplying the Z index by `0.001`.
///
/// It is not recommended to change the ZIndex frequently, as doing so will proc a full
/// re-propagation of ZIndex values throughout an entire layout. This is because the
/// display order of nodes is based on their order in the layout files. Nodes
/// that appear lower in a group/layout have a higher display order
#[derive(Debug, Copy, Clone, PartialEq, Eq, Reflect, Component, Default)]
pub enum ZIndex {
    Calculated(usize),
    #[default]
    NeedsRecalculation,
}

/// The location on a [`Node`] to treat as the position
///
/// For example, if a Node's anchor is [`Anchor::TopLeft`], the screen space
/// taken up by the node would be in the range from `[pos.x, pos.y] - [pos.x + size.x, pos.y + size.y]`
///
/// If it was [`Anchor::CenterRight`], then the screen space would be
/// `[pos.x - size.x, pos.y - size.y / 2.0] - [pos.x, pos.y + size.x / 2.0]`
#[derive(Deserialize, Serialize, Debug, Copy, Clone, PartialEq, Eq, Reflect, Default)]
pub enum Anchor {
    #[default]
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl From<bevy::sprite::Anchor> for Anchor {
    fn from(value: bevy::sprite::Anchor) -> Self {
        use bevy::sprite::Anchor as A;
        match value {
            A::TopLeft => Self::TopLeft,
            A::TopCenter => Self::TopCenter,
            A::TopRight => Self::TopRight,
            A::CenterLeft => Self::CenterLeft,
            A::Center => Self::Center,
            A::CenterRight => Self::CenterRight,
            A::BottomLeft => Self::BottomLeft,
            A::BottomCenter => Self::BottomCenter,
            A::BottomRight => Self::BottomRight,
            _ => Self::Center,
        }
    }
}

impl Anchor {
    /// Returns this anchor as normalized coordinates away from [`Anchor::Center`]
    ///
    /// I.e. [`Anchor::TopLeft`] will be `[-0.5, -0.5]` and [`Anchor::CenterRight`]
    /// will be `[0.5, 0.0]`
    pub const fn as_vec2(&self) -> Vec2 {
        match self {
            Self::TopLeft => vec2(-0.5, -0.5),
            Self::TopCenter => vec2(0.0, -0.5),
            Self::TopRight => vec2(0.5, -0.5),
            Self::CenterLeft => vec2(-0.5, 0.0),
            Self::Center => vec2(0.0, 0.0),
            Self::CenterRight => vec2(0.5, 0.0),
            Self::BottomLeft => vec2(-0.5, 0.5),
            Self::BottomCenter => vec2(0.0, 0.5),
            Self::BottomRight => vec2(0.5, 0.5),
        }
    }
}

/// Data about the position, size, and rotation of a node relative to its parent layout
#[derive(Debug, Copy, Clone, Reflect, Component)]
pub struct Node {
    /// Which part of this node the position represents
    pub anchor: Anchor,

    /// The XY pixel coordinates
    pub position: Vec2,

    /// The node dimensions in pixels
    pub size: Vec2,

    /// The rotation of the node in degrees
    ///
    /// The rotation pivot is always the center of the node
    pub rotation: f32,
}

impl Node {
    pub fn new_from_layout_node(node: &LayoutNode) -> Self {
        Self {
            anchor: node.anchor,
            position: node.position,
            size: node.size,
            rotation: node.rotation,
        }
    }

    pub fn calculate_position(&self, anchor: Anchor) -> Vec2 {
        self.position + self.size * (anchor.as_vec2() - self.anchor.as_vec2())
    }
}

/// A component that can be added to a node to calculate the bounding box
///
/// This is computed after transform propagation using the global camera coordinates/
/// rotation of the node
#[derive(Debug, Copy, Clone, Reflect, Component, Default)]
pub struct ComputedBoundingBox {
    top_left: Vec2,
    top_right: Vec2,
    bottom_left: Vec2,
    bottom_right: Vec2,
    center: Vec2,
    rotation: f32,
    size: Vec2,
}

impl ComputedBoundingBox {
    pub fn top_left(&self) -> Vec2 {
        self.top_left
    }

    pub fn top_right(&self) -> Vec2 {
        self.top_right
    }

    pub fn bottom_left(&self) -> Vec2 {
        self.bottom_left
    }

    pub fn bottom_right(&self) -> Vec2 {
        self.bottom_right
    }

    pub fn center(&self) -> Vec2 {
        self.center
    }

    pub fn contains(&self, point: Vec2) -> bool {
        let localized = point - self.center;
        let rotated = Vec2::from_angle(-self.rotation).rotate(localized);

        rotated.abs().cmple(self.size / 2.0).all()
    }

    pub fn calc_aabb(&self) -> Rect {
        let max = [
            self.top_left,
            self.top_right,
            self.bottom_left,
            self.bottom_right,
        ]
        .into_iter()
        .fold(Vec2::NEG_INFINITY, |a, b| a.max(b));
        let min = [
            self.top_left,
            self.top_right,
            self.bottom_left,
            self.bottom_right,
        ]
        .into_iter()
        .fold(Vec2::INFINITY, |a, b| a.min(b));

        Rect::from_corners(min, max)
    }
}

#[derive(Component, Clone, Reflect)]
pub(crate) struct LayoutHandle(pub Handle<Layout>);

/// Component that contains information about a layout
#[derive(Component, Copy, Clone, Reflect)]
pub struct LayoutInfo {
    /// The scale required to convert this layout's resolution to the parent layout's scale
    ///
    /// Exammple:
    /// Let's say we have two layouts, layout A and layout B.
    ///
    /// - Layout A's resolution, as defined in `a.layout.json` is `[1280, 720]`, aka 720p
    /// - Layout B's resolution, as defined in `b.layout.json` is `[1920, 1080]`, aka 1080p
    ///
    /// Layout B contains a layout node that points to Layout A. When generating the
    /// `LayoutInfo` for Layout A, the resolution scale would be calculated as:
    /// `resolution_scale = layout_b.resolution() / layout_a.resolution()`
    ///
    /// This information is only used to calculate the transform for this current node,
    /// which should propagate via bevy's transform logic into the child nodes and all of their
    /// children to ensure the layout's proportions are expected
    ///
    /// NOTE: For root layouts, this is `[1.0, 1.0]`
    pub(crate) resolution_scale: Vec2,

    /// The size of the layout, taken directly from the asset
    pub(crate) canvas_size: Vec2,
}

impl LayoutInfo {
    /// Calculates the child node's positions in world coordinates
    ///
    /// This method assumes that the camera being uses is a 2D camera as the units are in pixels,
    /// and that the root layout is parented to the camera.
    ///
    /// The result of this method is the vector `v` such that: `parent_layout.center() + v = child.center()`
    ///
    /// This method's return value should ONLY change at runtime when `Node` is changed, therefore propagation
    /// only occurs when a child's `Node` is changed.
    pub fn get_child_world_position(&self, child: &Node, anchor: Anchor) -> Vec2 {
        let position = child.calculate_position(anchor) - self.canvas_size / 2.0;
        position * Vec2::new(1.0, -1.0)
    }

    /// Calculates the scale of `node` based on this info
    ///
    /// This method assumes that `node` is a layout node that is the layout
    /// for which this info was derived from
    pub fn calculate_self_node_scale(&self, node: &Node) -> Vec2 {
        self.resolution_scale * node.size / self.canvas_size
    }
}

#[derive(WorldQuery)]
#[world_query(mutable)]
pub(crate) struct TransformPropagationQuery {
    entity: Entity,
    parent: &'static Parent,
    node: &'static Node,
    transform: &'static mut Transform,
    z_index: &'static ZIndex,
    anchor: Option<&'static bevy::sprite::Anchor>,
    layout_info: Option<&'static LayoutInfo>,
    is_root_node: Has<RootNode>,
}

pub(crate) fn propagate_to_transforms(
    mut nodes: Query<TransformPropagationQuery, Changed<Node>>,
    layout_info: Query<&LayoutInfo>,
) {
    nodes.par_iter_mut().for_each(|mut node| {
        let mut transform = Transform::default();
        if let Some(layout_info) = node.layout_info {
            transform.scale = layout_info.calculate_self_node_scale(node.node).extend(1.0);
        }

        let world_pos = if let Ok(parent_layout) = layout_info.get(node.parent.get()) {
            parent_layout.get_child_world_position(
                node.node,
                node.anchor
                    .map(|anchor| Anchor::from(*anchor))
                    .unwrap_or(Anchor::Center),
            )
        } else {
            if !node.is_root_node {
                log::warn!("A LayoutNode's parent does not have cached LayoutInfo");
            }
            node.node.position
        };

        match node.z_index {
            ZIndex::Calculated(value) => {
                transform.translation = world_pos.extend(*value as f32 * 0.001);
            }
            _ => {}
        }

        transform.rotation = Quat::from_axis_angle(Vec3::Z, node.node.rotation.to_radians());

        *node.transform = transform;
    });
}

#[derive(WorldQuery)]
#[world_query(mutable)]
pub(crate) struct BoundingBoxPropagationQuery {
    node: &'static Node,
    transform: &'static GlobalTransform,
    layout: &'static LayoutId,
    bounding_box: &'static mut ComputedBoundingBox,
}

pub(crate) fn propagate_to_bounding_box(
    mut nodes: Query<BoundingBoxPropagationQuery, Changed<Node>>,
    parents: Query<&Parent>,
    cameras: Query<&Camera>,
    images: Res<Assets<Image>>,
    manual_texture_views: Res<ManualTextureViews>,
    windows: Query<&Window>,
    primary_window: Query<&Window, With<PrimaryWindow>>,
) {
    nodes.par_iter_mut().for_each(|mut node| {
        let mut bounding_box = ComputedBoundingBox::default();

        let layout_id = node.layout.0;

        let Ok(parent) = parents.get(layout_id) else {
            log::warn!("Failed to get layout with id {layout_id:?}");
            return;
        };

        let Ok(camera) = cameras.get(parent.get()) else {
            log::warn!("Layout {layout_id:?} is not the direct child of a camera");
            return;
        };

        let size = match &camera.target {
            RenderTarget::Window(WindowRef::Primary) => {
                let Ok(window) = primary_window.get_single() else {
                    log::warn!("Failed to get primary window");
                    return;
                };

                Vec2::new(window.width(), window.height())
            }
            RenderTarget::Window(WindowRef::Entity(entity)) => {
                let Ok(window) = windows.get(*entity) else {
                    log::warn!("Failed to get window {entity:?}");
                    return;
                };

                Vec2::new(window.width(), window.height())
            }
            RenderTarget::Image(image) => {
                let Some(image) = images.get(image.id()) else {
                    log::warn!("Failed to render target image");
                    return;
                };

                image.size_f32()
            }
            RenderTarget::TextureView(view) => {
                let Some(target) = manual_texture_views.get(view) else {
                    log::warn!("Failed to render target view");
                    return;
                };

                target.size.as_vec2()
            }
        };

        let mut screen_coords = node.transform.translation().xy() + size / 2.0;
        screen_coords.y = size.y - screen_coords.y;

        let half_extent = node.node.size / 2.0;

        bounding_box.top_left = node
            .transform
            .transform_point((half_extent * Vec2::NEG_ONE).extend(0.0))
            .xy()
            - node.transform.translation().xy()
            + screen_coords;

        bounding_box.top_right = node
            .transform
            .transform_point((half_extent * vec2(1.0, -1.0)).extend(0.0))
            .xy()
            - node.transform.translation().xy()
            + screen_coords;

        bounding_box.bottom_left = node
            .transform
            .transform_point((half_extent * vec2(-1.0, 1.0)).extend(0.0))
            .xy()
            - node.transform.translation().xy()
            + screen_coords;

        bounding_box.bottom_right = node.transform.transform_point(half_extent.extend(0.0)).xy()
            - node.transform.translation().xy()
            + screen_coords;

        bounding_box.rotation = node
            .transform
            .to_scale_rotation_translation()
            .1
            .to_axis_angle()
            .1;
        bounding_box.size = node.node.size * node.transform.to_scale_rotation_translation().0.xy();
        bounding_box.center = screen_coords;

        *node.bounding_box = bounding_box;
    });
}

#[derive(WorldQuery)]
#[world_query(mutable)]
pub(crate) struct RefreshQuery {
    z_index: &'static mut ZIndex,
    kind: &'static NodeKind,
    children: Option<&'static Children>,
}

pub(crate) fn refresh_z_index(
    mut set: ParamSet<(Query<&LayoutId, Changed<ZIndex>>, Query<RefreshQuery>)>,
    mut needs_processed: Local<HashSet<Entity>>,
) {
    fn handle_node(query: &Query<RefreshQuery>, entity: Entity, z_value: &mut usize) {
        let mut node = unsafe { query.get_unchecked(entity).unwrap() };

        if matches!(node.kind, NodeKind::Layout | NodeKind::Group) {
            *node.z_index = ZIndex::Calculated(0);
        } else {
            *node.z_index = ZIndex::Calculated(*z_value);
            *z_value += 1;
        }

        if let Some(children) = node.children {
            for child in children.iter().copied() {
                handle_node(query, child, z_value);
            }
        }
    }

    needs_processed.clear();
    needs_processed.extend(set.p0().iter().map(|node| node.0));

    for node in needs_processed.iter().copied() {
        handle_node(&set.p1(), node, &mut 0);
    }
}
