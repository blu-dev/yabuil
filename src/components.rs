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

use crate::{asset::Layout, views::NodeEntityMut};

use self::spawning::spawn_layout;

pub mod spawning;

#[derive(Event)]
pub struct LoadedLayout {
    pub id: LayoutId,
    pub handle: Handle<Layout>,
}

#[derive(Copy, Clone, Component, Reflect)]
pub struct LayoutId(pub Entity);

#[derive(Clone, Component, Reflect)]
pub struct LayoutNodeId(PathBuf);

impl LayoutNodeId {
    pub fn root() -> Self {
        Self(PathBuf::from("__root"))
    }

    pub fn qualified(&self) -> &Path {
        let path = self.0.as_path();
        path.strip_prefix("__root").unwrap_or(path)
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

#[derive(Component)]
struct OnLoadCallback(Option<Box<dyn FnOnce(NodeEntityMut) + Send + Sync + 'static>>);

#[derive(Bundle)]
pub struct LayoutBundle {
    root: RootNode,
    awaiting_creation: PendingStatus,
    visibility: VisibilityBundle,
    transform: TransformBundle,
    on_load: OnLoadCallback,
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
            on_load: OnLoadCallback(None),
        }
    }

    pub fn with_on_load_callback(
        mut self,
        f: impl FnOnce(NodeEntityMut) + Send + Sync + 'static,
    ) -> Self {
        self.on_load.0 = Some(Box::new(f));
        self
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
pub enum SpawnLayoutError {
    #[error("Failed to spawn layout because the asset data does not exist/isn't loaded")]
    NotLoaded,
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

        let root_handle = root.root.handle.clone();
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
            let result = spawn_layout(world, entity, root_handle.clone(), |node, mut child| {
                for attribute in node.attributes.iter() {
                    attribute.apply(child.reborrow());
                }
            });

            let mut root = world.entity_mut(entity);

            if let Err(e) = result {
                log::error!("Failed to load layout: {e}");
                *root.get_mut::<PendingStatus>().unwrap() = PendingStatus::Failed;
            } else {
                root.remove::<PendingStatus>();
                let callback = root
                    .get_mut::<OnLoadCallback>()
                    .and_then(|mut cb| cb.0.take());
                root.remove::<OnLoadCallback>();
                if let Some(cb) = callback {
                    cb(NodeEntityMut::from_entity_world_mut(root));
                }
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
            let scale = match &parent.target {
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

                    Vec2::splat(window.scale_factor() as f32)
                }
                _ => Vec2::ONE,
            };

            viewport.physical_size.as_vec2() / scale
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

                    Vec2::new(window.width(), window.height())
                }
                RenderTarget::Image(image) => {
                    if let Some(image) = images.get(image.id()) {
                        image.size().as_vec2()
                    } else {
                        log::warn!("Failed to get render target image");
                        continue;
                    }
                }
                RenderTarget::TextureView(handle) => {
                    if let Some(view) = texture_views.get(handle) {
                        view.size.as_vec2()
                    } else {
                        log::warn!("Failed to get manual texture view");
                        continue;
                    }
                }
            }
        };

        let scale = render_target_size / node.canvas_size.as_vec2();
        transform.scale = scale.extend(1.0);
        transform.translation = Vec3::ZERO;
    }
}
