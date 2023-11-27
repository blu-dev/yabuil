use std::path::{Path, PathBuf};

use bevy::{
    asset::{LoadState, RecursiveDependencyLoadState},
    ecs::query::WorldQuery,
    prelude::*,
    render::{
        camera::{ManualTextureViews, RenderTarget},
        view::RenderLayers,
    },
    sprite::Anchor,
    text::Text2dBounds,
    utils::{HashMap, HashSet},
    window::{PrimaryWindow, WindowRef},
};
use thiserror::Error;

use crate::{
    animation::AnimationPlayerState,
    asset::{Layout, LayoutNode, LayoutNodeData, NodeAnchor},
    views::NodeWorldViewMut,
};

#[derive(Copy, Clone)]
pub struct LayoutId(pub(crate) Entity);

#[derive(Clone)]
pub struct LayoutNodeId(PathBuf);

impl LayoutNodeId {
    pub fn qualified(&self) -> &Path {
        self.0.as_path()
    }

    pub fn name(&self) -> &str {
        self.0.file_name().unwrap().to_str().unwrap()
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum NodeKind {
    Null,
    Image,
    Text,
    Layout,
}

#[derive(Component)]
pub struct ActiveLayout;

#[derive(Component)]
pub struct LayoutNodeMetadata {
    kind: NodeKind,
    layout: LayoutId,
    id: LayoutNodeId,
    anchor: NodeAnchor,
    pub position: Vec2,
    pub size: Vec2,
}

impl LayoutNodeMetadata {
    pub fn new(layout_id: LayoutId, parent_id: Option<PathBuf>, node: &LayoutNode) -> Self {
        Self {
            kind: match &node.inner {
                LayoutNodeData::Null => NodeKind::Null,
                LayoutNodeData::Text { .. } => NodeKind::Text,
                LayoutNodeData::Image { .. } => NodeKind::Image,
                LayoutNodeData::Layout { .. } => NodeKind::Layout,
            },
            layout: layout_id,
            id: LayoutNodeId(
                parent_id
                    .map(|parent| parent.join(node.id.as_str()))
                    .unwrap_or_else(|| PathBuf::from(node.id.clone())),
            ),
            anchor: node.anchor,
            position: node.position,
            size: node.size,
        }
    }

    pub fn kind(&self) -> NodeKind {
        self.kind
    }

    pub fn layout_id(&self) -> LayoutId {
        self.layout
    }

    pub fn id(&self) -> &LayoutNodeId {
        &self.id
    }
}

#[derive(Component)]
pub struct ComputedLayoutNodeMetadata {
    kind: NodeKind,
    layout: LayoutId,
    bounding_box: Rect,
    id: LayoutNodeId,
    resolution_scale: Vec2,
    camera_center: Vec2,

    layout_resolution: Vec2,
    layout_canvas_size: Vec2,
}

impl ComputedLayoutNodeMetadata {
    pub fn kind(&self) -> NodeKind {
        self.kind
    }

    pub fn layout_id(&self) -> LayoutId {
        self.layout
    }

    pub fn bounding_box(&self) -> Rect {
        self.bounding_box
    }

    pub fn id(&self) -> &LayoutNodeId {
        &self.id
    }

    pub fn resolution_scale(&self) -> Vec2 {
        self.resolution_scale
    }

    pub fn camera_center(&self) -> Vec2 {
        self.camera_center
    }
}

#[derive(Component, PartialEq, Eq)]
pub(crate) enum PendingStatus {
    AwaitingCreation,
    Failed,
}

#[derive(Component)]
pub struct RootNode {
    handle: Handle<Layout>,
    nodes: HashMap<String, Entity>,
}

#[derive(Bundle)]
pub struct LayoutBundle {
    root: RootNode,
    awaiting_creation: PendingStatus,
    visibility: VisibilityBundle,
    transform: TransformBundle,
}

impl LayoutBundle {
    pub fn new(handle: Handle<Layout>) -> Self {
        Self {
            root: RootNode {
                handle,
                nodes: HashMap::new(),
            },
            awaiting_creation: PendingStatus::AwaitingCreation,
            visibility: VisibilityBundle {
                visibility: Visibility::Hidden,
                ..default()
            },
            transform: TransformBundle::default(),
        }
    }
}

#[derive(WorldQuery)]
#[world_query(mutable)]
pub(crate) struct PendingRootQuery {
    pub entity: Entity,
    pub root: &'static RootNode,
    pub status: &'static mut PendingStatus,
    pub layers: Option<&'static RenderLayers>,
}

#[derive(Error, Debug)]
enum SpawnLayoutError {
    #[error("Failed to spawn layout because the asset data does not exist/isn't loaded")]
    NotLoaded,
}

struct LayoutBuilder<'a> {
    pub root_node: Entity,
    pub root_resolution: UVec2,
    pub allocated_size: Vec2,
    pub parent_origin: Vec2,
    pub parent_node_path: Option<PathBuf>,
    pub z_offset: &'a mut f32,
}

impl LayoutBuilder<'_> {
    pub fn create_metadata(
        &self,
        layout: &Layout,
        node: &LayoutNodeMetadata,
    ) -> ComputedLayoutNodeMetadata {
        let resolution_scale = (self.root_resolution.as_vec2() / layout.get_resolution().as_vec2())
            * (self.allocated_size / layout.canvas_size.as_vec2());

        let tl = node.anchor.calculate_origin(node.position, node.size) * resolution_scale
            + self.parent_origin;
        let br = tl + node.size * resolution_scale;

        let mut center = node.anchor.calculate_center(node.position, node.size) * resolution_scale;
        center.y = self.allocated_size.y as f32 - center.y;

        ComputedLayoutNodeMetadata {
            kind: node.kind(),
            layout: node.layout_id(),
            bounding_box: Rect::from_corners(tl, br),
            id: node.id().clone(),
            resolution_scale,
            camera_center: center,
            layout_resolution: layout.get_resolution().as_vec2(),
            layout_canvas_size: layout.canvas_size.as_vec2(),
        }
    }
}

fn spawn_layout_recursive(
    world: &mut World,
    assets: &Assets<Layout>,
    layout_id: AssetId<Layout>,
    current_node: Entity,
    builder: LayoutBuilder,
) -> Result<(), SpawnLayoutError> {
    let layout = assets.get(layout_id).ok_or(SpawnLayoutError::NotLoaded)?;

    for node in layout.nodes.iter() {
        let local_metadata = LayoutNodeMetadata::new(
            LayoutId(builder.root_node),
            builder.parent_node_path.clone(),
            node,
        );
        let metadata = builder.create_metadata(layout, &local_metadata);

        let bounding_box = metadata.bounding_box;
        let scaling = metadata.resolution_scale;
        let node_id = metadata.id.0.clone();
        let mut transform =
            Transform::from_translation(metadata.camera_center.extend(*builder.z_offset));
        *builder.z_offset += 0.01;
        let child = world.spawn((metadata, local_metadata));
        let child = child.id();
        world.entity_mut(current_node).add_child(child);
        let mut child = world.entity_mut(child);

        match &node.inner {
            LayoutNodeData::Null => {
                child.insert((
                    TransformBundle::from_transform(transform),
                    VisibilityBundle::default(),
                ));
            }
            LayoutNodeData::Image { handle, .. } => {
                child.insert(SpriteBundle {
                    sprite: Sprite {
                        custom_size: Some(bounding_box.size()),
                        ..default()
                    },
                    texture: handle.clone(),
                    transform,
                    ..default()
                });
            }
            LayoutNodeData::Text {
                text,
                handle,
                size,
                color,
                alignment,
                ..
            } => {
                let anchor = match alignment {
                    TextAlignment::Left => {
                        transform.translation.x -= bounding_box.half_size().x * scaling.x;
                        Anchor::CenterLeft
                    }
                    TextAlignment::Center => Anchor::Center,
                    TextAlignment::Right => {
                        transform.translation.x += bounding_box.half_size().x * scaling.x;
                        Anchor::CenterRight
                    }
                };
                child.insert(Text2dBundle {
                    text: Text::from_section(
                        text.clone(),
                        TextStyle {
                            font: handle.clone(),
                            font_size: *size,
                            color: Color::rgba(color[0], color[1], color[2], color[3]),
                        },
                    ),
                    text_2d_bounds: Text2dBounds {
                        size: bounding_box.size(),
                    },
                    text_anchor: anchor,
                    transform,
                    ..default()
                });
            }
            LayoutNodeData::Layout { handle, .. } => {
                let Some(layout) = assets.get(handle.id()) else {
                    return Err(SpawnLayoutError::NotLoaded);
                };

                // Move the transform for this to the bottom right so that the transform math
                // of child nodes adds up
                transform.translation -= bounding_box.half_size().extend(0.0);
                child.insert((
                    TransformBundle::from_transform(transform),
                    VisibilityBundle::default(),
                    layout.animations.clone(),
                    AnimationPlayerState::NotPlaying,
                ));

                let builder = LayoutBuilder {
                    root_node: builder.root_node,
                    root_resolution: builder.root_resolution,
                    allocated_size: node.size,
                    parent_origin: bounding_box.min,
                    parent_node_path: Some(node_id),
                    z_offset: builder.z_offset,
                };

                let current_node = child.id();

                spawn_layout_recursive(world, assets, handle.id(), current_node, builder)?;

                child = world.entity_mut(current_node);
            }
        }

        let mut child = NodeWorldViewMut::new(child).unwrap();

        for attribute in node.attributes.iter() {
            attribute.apply(&mut child);
        }
    }

    Ok(())
}

pub(crate) fn spawn_layout_system(
    mut commands: Commands,
    mut pending: Query<PendingRootQuery>,
    assets: Res<AssetServer>,
) {
    for mut root in pending.iter_mut() {
        if *root.status == PendingStatus::Failed {
            continue;
        }

        let handle_id = root.root.handle.id();

        match assets.get_load_state(handle_id) {
            None => {
                log::error!("Failed to load layout because the handle state is gone");
                *root.status = PendingStatus::Failed;
                continue;
            }
            Some(LoadState::Failed) => {
                log::error!("Failed to load layout, check asset loader logs");
                *root.status = PendingStatus::Failed;
                continue;
            }
            _ => {}
        }

        match assets.get_recursive_dependency_load_state(handle_id) {
            None => {
                log::error!("Failed to load layout because the handle state is gone");
                *root.status = PendingStatus::Failed;
                continue;
            }
            Some(RecursiveDependencyLoadState::Failed) => {
                log::error!("Failed to load layout because one or more dependencies failed to load, check asset loader logs");
                *root.status = PendingStatus::Failed;
                continue;
            }
            Some(RecursiveDependencyLoadState::Loaded) => {}
            _ => continue,
        }

        let entity = root.entity;

        commands.add(move |world: &mut World| {
            let result =
                world.resource_scope::<Assets<Layout>, _>(|world, layouts| {
                    let root = layouts.get(handle_id).ok_or(SpawnLayoutError::NotLoaded)?;
                    let resolution = root.get_resolution();

                    spawn_layout_recursive(
                        world,
                        &layouts,
                        handle_id,
                        entity,
                        LayoutBuilder {
                            root_node: entity,
                            root_resolution: resolution,
                            allocated_size: root.canvas_size.as_vec2(),
                            parent_origin: Vec2::default(),
                            parent_node_path: None,
                            z_offset: &mut 0.0,
                        },
                    )
                });

            let mut root = world.entity_mut(entity);

            if let Err(e) = result {
                log::error!("Failed to load layout: {e}");
                root.despawn_descendants();
                *root.get_mut::<PendingStatus>().unwrap() = PendingStatus::Failed;
            } else {
                root.remove::<PendingStatus>();
            }
        });
    }
}

pub(crate) fn update_ui_layout_visibility(
    mut layouts: Query<(&mut Visibility, Has<ActiveLayout>), With<RootNode>>,
) {
    for (mut vis, is_active) in layouts.iter_mut() {
        if is_active {
            *vis = Visibility::Inherited;
        } else {
            *vis = Visibility::Hidden;
        }
    }
}

pub(crate) fn update_ui_layout_transform(
    cameras: Query<&Camera>,
    windows: Query<&Window>,
    primary_window: Query<&Window, With<PrimaryWindow>>,
    mut layouts: Query<
        (&Parent, &RootNode, &mut Transform),
        (With<ActiveLayout>, Without<PendingStatus>),
    >,
    layout_assets: Res<Assets<Layout>>,
    images: Res<Assets<Image>>,
    texture_views: Res<ManualTextureViews>,
) {
    for (parent, root, mut transform) in layouts.iter_mut() {
        let Some(node) = layout_assets.get(root.handle.id()) else {
            log::warn!("Could not get layout asset");
            continue;
        };

        let Ok(parent) = cameras.get(parent.get()) else {
            log::warn!("Layout is not parented to camera");
            continue;
        };

        let render_target_size =
            if let Some(viewport) = parent.viewport.as_ref() {
                viewport.physical_size
            } else {
                match &parent.target {
                    RenderTarget::Window(win_ref) => {
                        let window = match win_ref {
                            WindowRef::Primary => {
                                let Ok(window) = primary_window.get_single() else {
                                    log::warn!("Failed to get primary window");
                                    continue;
                                };
                                window
                            }
                            WindowRef::Entity(entity) => {
                                let Ok(window) = windows.get(*entity) else {
                                    log::warn!("Failed to get window {entity:?}");
                                    continue;
                                };
                                window
                            }
                        };

                        Vec2::new(window.width(), window.height()).as_uvec2()
                    }
                    RenderTarget::Image(image) => {
                        if let Some(image) = images.get(image.id()) {
                            image.size()
                        } else {
                            log::warn!("Failed to get render target image");
                            continue;
                        }
                    }
                    RenderTarget::TextureView(handle) => {
                        if let Some(view) = texture_views.get(handle) {
                            view.size
                        } else {
                            log::warn!("Failed to get manual texture view");
                            continue;
                        }
                    }
                }
            };

        let scale = render_target_size.as_vec2() / node.canvas_size.as_vec2();
        transform.scale = scale.extend(1.0);
        transform.translation = {
            let he = render_target_size.as_vec2() / -2.0;
            he.extend(0.0)
        };
    }
}

pub(crate) fn node_metadata_propagate(
    children: Query<&Children>,
    roots: Query<(Entity, &RootNode)>,
    nodes: Query<
        (
            Entity,
            &Parent,
            Ref<LayoutNodeMetadata>,
            &mut ComputedLayoutNodeMetadata,
            &mut Transform,
        ),
        Without<RootNode>,
    >,
    anchors: Query<&Anchor>,
    mut updated_cache: Local<HashSet<Entity>>,
    layouts: Res<Assets<Layout>>,
) {
    updated_cache.clear();

    for (entity, root) in roots.iter() {
        let Some(layout) = layouts.get(root.handle.id()) else {
            continue;
        };

        for descendant in children.iter_descendants(entity) {
            // SAFETY: we are not running in parallel, at max this loop iteration will only access
            // a single node and it's parent at a time, which are distinct entities.
            let (this_node, parent, metadata, mut computed, mut transform) = unsafe {
                nodes
                    .get_unchecked(descendant)
                    .expect("Descendant of layout root should be a node")
            };
            let should_update = if parent.get() == entity {
                metadata.is_changed()
            } else {
                // iter_descendants is breadth-first so this works for propagating changes
                updated_cache.contains(&parent.get()) || metadata.is_changed()
            };

            if !should_update {
                continue;
            }

            updated_cache.insert(this_node);

            // If the parent is the root node, we are the root of propagation
            let (resolution_scale, parent_origin, parent_height) = if parent.get() == entity {
                (Vec2::ONE, Vec2::ZERO, layout.canvas_size.y as f32)
            } else {
                let (_, _, parent_meta, parent_computed, _) = nodes
                    .get(parent.get())
                    .expect("Descendant of layout root should be a node");

                let resolution_scale = (layout.get_resolution().as_vec2()
                    / computed.layout_resolution)
                    * (parent_meta.size / computed.layout_canvas_size);

                (
                    resolution_scale,
                    parent_computed.bounding_box.min,
                    parent_meta.size.y,
                )
            };

            let tl = metadata
                .anchor
                .calculate_origin(metadata.position, metadata.size)
                * resolution_scale
                + parent_origin;

            let br = tl + metadata.size * resolution_scale;

            let mut center = metadata
                .anchor
                .calculate_center(metadata.position, metadata.size)
                * resolution_scale;

            center.y = parent_height as f32 - center.y;

            computed.bounding_box = Rect::from_corners(tl, br);
            computed.resolution_scale = resolution_scale;
            transform.translation.x = center.x;
            transform.translation.y = center.y;

            match metadata.kind() {
                NodeKind::Layout => {
                    transform.translation -= computed.bounding_box.half_size().extend(0.0);
                }
                NodeKind::Text => match anchors.get(this_node).unwrap() {
                    Anchor::CenterLeft => {
                        transform.translation.x -= computed.bounding_box.half_size().x
                    }
                    Anchor::CenterRight => {
                        transform.translation.x += computed.bounding_box.half_size().x
                    }
                    _ => {}
                },
                _ => {}
            }

            if metadata.kind() == NodeKind::Layout {}
        }
    }
}
