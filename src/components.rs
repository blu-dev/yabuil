use std::path::{Path, PathBuf};

use bevy::{
    asset::{LoadState, RecursiveDependencyLoadState},
    ecs::query::WorldQuery,
    prelude::*,
    render::{
        camera::{ManualTextureViews, RenderTarget},
        view::RenderLayers,
    },
    window::{PrimaryWindow, WindowRef},
};
use thiserror::Error;

use crate::{
    animation::AnimationPlayerState,
    asset::{Layout, LayoutNodeInner},
    node::{Anchor, LayoutInfo, ZIndex},
    views::NodeWorldViewMut,
};

#[derive(Copy, Clone, Component, Reflect)]
pub struct LayoutId(pub(crate) Entity);

#[derive(Clone, Component, Reflect)]
pub struct LayoutNodeId(PathBuf);

impl LayoutNodeId {
    pub fn qualified(&self) -> &Path {
        self.0.as_path()
    }

    pub fn name(&self) -> &str {
        self.0.file_name().unwrap().to_str().unwrap()
    }

    pub fn join(&self, id: &str) -> Self {
        Self(self.0.clone().join(id))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Component, Reflect)]
pub enum NodeKind {
    Null,
    Image,
    Text,
    Layout,
    Group,
}

#[derive(Component)]
pub struct ActiveLayout;

#[derive(Component, PartialEq, Eq)]
pub(crate) enum PendingStatus {
    AwaitingCreation,
    Failed,
}

#[derive(Component)]
pub struct RootNode {
    handle: Handle<Layout>,
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
            root: RootNode { handle },
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

fn spawn_layout_recursive(
    world: &mut World,
    assets: &Assets<Layout>,
    layout_id: AssetId<Layout>,
    current_node: Entity,
    root_id: LayoutId,
    parent_id: LayoutNodeId,
    z_index: &mut ZIndex,
) -> Result<(), SpawnLayoutError> {
    let layout = assets.get(layout_id).ok_or(SpawnLayoutError::NotLoaded)?;

    let mut nodes_to_init = vec![];

    for node in layout.nodes.iter() {
        let child = spawn_node(
            node,
            world,
            &parent_id,
            root_id,
            z_index,
            current_node,
            assets,
            layout,
        )?;

        nodes_to_init.push(child);
    }

    for (node, entity) in layout.nodes.iter().zip(nodes_to_init.into_iter()) {
        let mut child = NodeWorldViewMut::new(world.entity_mut(entity)).unwrap();

        for attribute in node.attributes.iter() {
            attribute.apply(&mut child);
        }
    }

    Ok(())
}

fn spawn_node(
    node: &crate::asset::LayoutNode,
    world: &mut World,
    parent_id: &LayoutNodeId,
    root_id: LayoutId,
    z_index: &mut ZIndex,
    current_node: Entity,
    assets: &Assets<Layout>,
    layout: &Layout,
) -> Result<Entity, SpawnLayoutError> {
    let layout_node = crate::node::Node {
        anchor: node.anchor,
        position: node.position,
        size: node.size,
        rotation: node.rotation,
    };
    let child = world.spawn((
        layout_node,
        parent_id.join(node.id.as_str()),
        root_id,
        z_index.fetch_inc(),
    ));
    let child = child.id();
    world.entity_mut(current_node).add_child(child);
    let mut child = world.entity_mut(child);
    match &node.inner {
        LayoutNodeInner::Null => {
            child.insert((
                TransformBundle::default(),
                VisibilityBundle::default(),
                NodeKind::Null,
            ));
        }
        LayoutNodeInner::Image(data) => {
            let color = data
                .tint
                .map(|[r, g, b, a]| Color::rgba(r, g, b, a))
                .unwrap_or(Color::WHITE);

            child.insert((
                SpriteBundle {
                    sprite: Sprite {
                        color,
                        custom_size: Some(node.size),
                        ..default()
                    },
                    texture: data.handle.clone(),
                    ..default()
                },
                NodeKind::Image,
            ));
        }
        LayoutNodeInner::Text(data) => {
            let anchor = match data.alignment {
                TextAlignment::Left => bevy::sprite::Anchor::CenterLeft,
                TextAlignment::Center => bevy::sprite::Anchor::Center,
                TextAlignment::Right => bevy::sprite::Anchor::CenterRight,
            };

            child.insert((
                Text2dBundle {
                    text: Text::from_section(
                        data.text.clone(),
                        TextStyle {
                            font: data.handle.clone(),
                            font_size: data.size,
                            color: Color::rgba(
                                data.color[0],
                                data.color[1],
                                data.color[2],
                                data.color[3],
                            ),
                        },
                    ),
                    text_anchor: anchor,
                    ..default()
                },
                NodeKind::Text,
            ));
        }
        LayoutNodeInner::Layout(data) => {
            let Some(child_layout) = assets.get(data.handle.id()) else {
                return Err(SpawnLayoutError::NotLoaded);
            };

            child.insert((
                TransformBundle::default(),
                VisibilityBundle::default(),
                child_layout.animations.clone(),
                AnimationPlayerState::NotPlaying,
                NodeKind::Layout,
                LayoutInfo {
                    resolution_scale: layout.get_resolution().as_vec2()
                        / child_layout.get_resolution().as_vec2(),
                    canvas_size: child_layout.canvas_size.as_vec2(),
                },
            ));

            let current_node = child.id();

            spawn_layout_recursive(
                world,
                assets,
                data.handle.id(),
                current_node,
                root_id,
                parent_id.join(node.id.as_str()),
                z_index,
            )?;

            child = world.entity_mut(current_node);
        }
        LayoutNodeInner::Group(group) => {
            child.insert((
                TransformBundle::default(),
                VisibilityBundle::default(),
                NodeKind::Group,
                LayoutInfo {
                    resolution_scale: Vec2::ONE,
                    canvas_size: node.size,
                },
            ));

            let current_node = child.id();

            let parent_id = parent_id.join(node.id.as_str());

            for node in group.iter() {
                spawn_node(
                    node,
                    world,
                    &parent_id,
                    root_id,
                    z_index,
                    current_node,
                    assets,
                    layout,
                )?;
            }

            child = world.entity_mut(current_node);
        }
    }
    Ok(child.id())
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
            let result: Result<(), SpawnLayoutError> =
                world.resource_scope::<Assets<Layout>, _>(|world, layouts| {
                    let root = layouts.get(handle_id).ok_or(SpawnLayoutError::NotLoaded)?;
                    let id = LayoutNodeId(PathBuf::from("__root"));
                    spawn_layout_recursive(
                        world,
                        &layouts,
                        handle_id,
                        entity,
                        LayoutId(entity),
                        id.clone(),
                        &mut ZIndex::default(),
                    )?;

                    world.entity_mut(entity).insert((
                        root.animations.clone(),
                        AnimationPlayerState::NotPlaying,
                        LayoutInfo {
                            resolution_scale: Vec2::ONE,
                            canvas_size: root.canvas_size.as_vec2(),
                        },
                        crate::node::Node {
                            anchor: Anchor::TopLeft,
                            position: Vec2::ZERO,
                            size: root.canvas_size.as_vec2(),
                            rotation: 0.0,
                        },
                        LayoutId(entity),
                        id,
                        NodeKind::Layout,
                    ));

                    Ok(())
                });

            let mut root = world.entity_mut(entity);

            if let Err(e) = result {
                log::error!("Failed to load layout: {e}");
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

        let render_target_size = if let Some(viewport) = parent.viewport.as_ref() {
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
        // transform.translation = {
        //     let he = render_target_size.as_vec2() / -2.0;
        //     he.extend(0.0)
        // };

        transform.translation = Vec3::ZERO;
    }
}
