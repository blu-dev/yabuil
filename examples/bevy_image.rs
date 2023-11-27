use std::path::PathBuf;

use bevy::{
    app::App,
    asset::{AssetServer, Handle},
    ecs::system::Commands,
    prelude::*,
    render::texture::Image,
    DefaultPlugins,
};
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use serde::{Deserialize, Serialize};
use yabuil::{
    asset::Layout, views::NodeWorldViewMut, ActiveLayout, LayoutApp, LayoutAttribute, LayoutBundle,
    LayoutPlugin,
};

#[derive(Serialize, Deserialize)]
pub struct CustomImage {
    path: PathBuf,
    #[serde(skip)]
    handle: Handle<Image>,
}

impl LayoutAttribute for CustomImage {
    fn apply(&self, world: &mut NodeWorldViewMut) {
        world.as_entity_world_mut().insert(self.handle.clone());
    }

    fn initialize_dependencies(&mut self, context: &mut yabuil::RestrictedLoadContext) {
        self.handle = context.load(self.path.clone());
    }

    fn visit_dependencies(&self, visit_fn: &mut dyn FnMut(bevy::asset::UntypedAssetId)) {
        visit_fn(self.handle.id().untyped());
    }
}

fn startup_system(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands
        .spawn((Camera2dBundle::default(), VisibilityBundle::default()))
        .with_children(|children| {
            children.spawn((
                LayoutBundle::new(asset_server.load::<Layout>("layouts/custom_image.layout.json")),
                ActiveLayout,
            ));
        });
}

pub fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            LayoutPlugin,
            WorldInspectorPlugin::default(),
        ))
        .register_layout_attribute::<CustomImage>("CustomImage")
        .add_systems(Startup, startup_system)
        .add_systems(Update, bevy::window::close_on_esc)
        .run();
}
