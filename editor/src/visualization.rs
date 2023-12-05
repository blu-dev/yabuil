use std::path::PathBuf;

use bevy::{
    asset::RecursiveDependencyLoadState, ecs::system::EntityCommand, prelude::*, utils::HashMap,
};
use yabuil::{asset::Layout, node::ComputedBoundingBox};

#[derive(Event)]
pub enum EditorLayoutEvent {
    NodeAdded { path: PathBuf },
    NodeEdited { path: PathBuf },
    NodeRemoved { path: PathBuf },
}

#[derive(Component)]
pub struct EditorLayout {
    pub layout: Handle<Layout>,
    pub children: HashMap<PathBuf, Entity>,
}

#[derive(Component)]
pub struct AwaitingLoad;

pub fn spawn_editor_camera(commands: &mut Commands, layout: Handle<Layout>) {
    commands
        .spawn((
            Camera2dBundle {
                camera: Camera {
                    is_active: false,
                    ..default()
                },
                ..default()
            },
            crate::LAYOUT_PREVIEW_RENDER_LAYER,
            VisibilityBundle::default(),
        ))
        .with_children(|children| {
            children.spawn((
                EditorLayout {
                    layout,
                    children: HashMap::new(),
                },
                TransformBundle::default(),
                VisibilityBundle::default(),
            ));
        });
}

struct SpawnEditorLayout {
    handle: Handle<Layout>,
}

impl EntityCommand for SpawnEditorLayout {
    fn apply(self, id: Entity, world: &mut World) {
        if let Err(e) =
            yabuil::components::spawning::spawn_layout(world, id, self.handle, |_, mut child| {
                child
                    .as_entity_world_mut()
                    .insert(ComputedBoundingBox::default());
            })
        {
            log::error!("Failed to load layout: {e}");
            world.entity_mut(id).despawn_recursive();
        }
    }
}

pub fn handle_load_editor_layout(
    mut commands: Commands,
    layouts: Query<(Entity, &EditorLayout), With<AwaitingLoad>>,
    assets: Res<AssetServer>,
) {
    for (entity, layout) in layouts.iter() {
        match assets.get_recursive_dependency_load_state(layout.layout.id()) {
            Some(RecursiveDependencyLoadState::Loaded) => {
                commands
                    .entity(entity)
                    .remove::<AwaitingLoad>()
                    .add(SpawnEditorLayout {
                        handle: layout.layout.clone(),
                    });
            }
            _ => {}
        }
    }
}
