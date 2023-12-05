use std::{any::TypeId, path::PathBuf};

use bevy::{
    asset::{embedded_asset, AssetApp},
    math::vec2,
    prelude::*,
    render::view::RenderLayers,
    window::PrimaryWindow,
};
use bevy_egui::{egui::TextureId, EguiContext, EguiPlugin, EguiUserTextures};
use bevy_inspector_egui::inspector_egui_impls::InspectorEguiImpl;
use egui_dock::{DockArea, DockState, TabViewer};
use layout_view::LayoutViewResponse;
use svg::SvgLoader;
use yabuil::{
    asset::{Layout, UnregisteredData},
    LayoutPlugin,
};

mod animation_view;
mod layout_view;
mod node_view;
mod reflect;
mod svg;
mod visualization;

pub const LAYOUT_PREVIEW_RENDER_LAYER: RenderLayers = RenderLayers::layer(31);

#[derive(Debug, PartialEq, Eq, Reflect)]
pub enum EditorTab {
    Game,
    LayoutHierarchyView(Handle<Layout>),
    NodeView {
        node_path: PathBuf,
        layout: Handle<Layout>,
    },
    AnimationView {
        name: String,
        layout: Handle<Layout>,
    },
}

pub struct EditorTabViewer<'a> {
    world: &'a mut World,
    game_window: &'a mut Rect,
    pending_tabs: &'a mut Vec<EditorTab>,
    should_render_game: &'a mut bool,
}

impl<'a> TabViewer for EditorTabViewer<'a> {
    type Tab = EditorTab;

    fn title(&mut self, tab: &mut Self::Tab) -> bevy_egui::egui::WidgetText {
        match tab {
            EditorTab::Game => "Game".into(),
            EditorTab::LayoutHierarchyView(handle) => {
                let path = self.world.resource::<AssetServer>().get_path(handle.id());

                path.as_ref()
                    .and_then(|path| path.path().file_name().and_then(|s| s.to_str()))
                    .unwrap_or("Layout View")
                    .into()
            }
            EditorTab::NodeView { node_path, layout } => {
                let path = self.world.resource::<AssetServer>().get_path(layout.id());

                let name = path
                    .as_ref()
                    .and_then(|path| path.path().file_name().and_then(|s| s.to_str()))
                    .unwrap_or("Layout View");

                format!("{name}:{}", node_path.display()).into()
            }
            EditorTab::AnimationView { name, layout } => {
                let path = self.world.resource::<AssetServer>().get_path(layout.id());

                let layout_name = path
                    .as_ref()
                    .and_then(|path| path.path().file_name().and_then(|s| s.to_str()))
                    .unwrap_or("Layout View");

                format!("{layout_name}:{name}").into()
            }
        }
    }

    fn clear_background(&self, tab: &Self::Tab) -> bool {
        !matches!(tab, EditorTab::Game)
    }

    fn closeable(&mut self, tab: &mut Self::Tab) -> bool {
        match tab {
            EditorTab::Game => false,
            _ => true,
        }
    }

    fn ui(&mut self, ui: &mut bevy_egui::egui::Ui, tab: &mut Self::Tab) {
        let id = self.id(tab);

        self.world
            .resource_scope::<Assets<Layout>, _>(|world, mut layouts| match tab {
                EditorTab::Game => {
                    let rect = ui.clip_rect();
                    let tl = rect.left_top();
                    let br = rect.right_bottom();
                    *self.game_window = Rect::from_corners(vec2(tl.x, tl.y), vec2(br.x, br.y));

                    ui.checkbox(self.should_render_game, "Show Game");
                }
                EditorTab::LayoutHierarchyView(handle) => {
                    let Some(layout) = layouts.get_mut(handle.id()) else {
                        return;
                    };

                    match layout_view::layout_view_ui(layout, ui, id, world.resource::<EguiIcons>())
                    {
                        Some(LayoutViewResponse::OpenLayout(handle)) => {
                            self.pending_tabs
                                .push(EditorTab::LayoutHierarchyView(handle));
                        }
                        Some(LayoutViewResponse::OpenNode(path)) => {
                            self.pending_tabs.push(EditorTab::NodeView {
                                node_path: path,
                                layout: handle.clone(),
                            });
                        }
                        Some(LayoutViewResponse::OpenAnimation(name)) => {
                            self.pending_tabs.push(EditorTab::AnimationView {
                                name,
                                layout: handle.clone(),
                            });
                        }
                        _ => {}
                    }
                }
                EditorTab::NodeView {
                    node_path,
                    layout: layout_handle,
                } => {
                    let Some(layout) = layouts.get_mut(layout_handle.id()) else {
                        return;
                    };

                    let Some(node) = layout.child_by_id_mut(&node_path) else {
                        return;
                    };

                    let registry = world.resource::<AppTypeRegistry>().internal.clone();
                    let registry = registry.read().unwrap();

                    node_view::node_view_ui(
                        node,
                        ui,
                        id,
                        &mut world.resource_mut::<EguiUserTextures>(),
                        &registry,
                    );
                }
                EditorTab::AnimationView {
                    name,
                    layout: layout_handle,
                } => {
                    let Some(layout) = layouts.get_mut(layout_handle.id()) else {
                        return;
                    };

                    let mut animations = layout.animations.write().unwrap();

                    let Some(animation) = animations.get_mut(name.as_str()) else {
                        return;
                    };

                    let registry = world.resource::<AppTypeRegistry>().internal.clone();
                    let registry = registry.read().unwrap();

                    if let Some(path) =
                        animation_view::animation_view_ui(animation, ui, id, &registry)
                    {
                        self.pending_tabs.push(EditorTab::NodeView {
                            node_path: path,
                            layout: layout_handle.clone(),
                        });
                    }
                }
            });
    }
}

#[derive(Resource)]
pub struct UiState {
    dock_state: DockState<EditorTab>,
    pub game_window: Rect,
    pub should_render_game: bool,
}

#[derive(Resource)]
pub struct EguiIcons {
    pub image: TextureId,
    pub layout: TextureId,
    pub question: TextureId,
    pub text: TextureId,
}

impl FromWorld for EguiIcons {
    fn from_world(world: &mut World) -> Self {
        world.resource_scope::<EguiUserTextures, _>(|world, mut textures| {
            let server = world.resource::<AssetServer>();
            EguiIcons {
                image: textures.add_image(server.load("embedded://editor/resources/image.svg")),
                layout: textures.add_image(server.load("embedded://editor/resources/layout.svg")),
                question: textures
                    .add_image(server.load("embedded://editor/resources/question.svg")),
                text: textures.add_image(server.load("embedded://editor/resources/text.svg")),
            }
        })
    }
}

fn ui_system(world: &mut World) {
    let Ok(mut context) = world
        .query_filtered::<&EguiContext, With<PrimaryWindow>>()
        .get_single(world)
        .cloned()
    else {
        return;
    };

    world.resource_scope::<UiState, _>(|world, mut state| {
        let mut pending = vec![];
        let state = &mut *state;
        DockArea::new(&mut state.dock_state).show(
            context.get_mut(),
            &mut EditorTabViewer {
                world,
                game_window: &mut state.game_window,
                pending_tabs: &mut pending,
                should_render_game: &mut state.should_render_game,
            },
        );

        for tab in pending {
            state.dock_state.push_to_first_leaf(tab);
        }
    });
}

pub fn get_editor_app(asset_root: impl Into<PathBuf>, starting_asset: impl Into<PathBuf>) -> App {
    let asset_root = asset_root.into();

    let asset_root = if asset_root.is_absolute() {
        pathdiff::diff_paths(asset_root, std::env::current_dir().unwrap()).unwrap()
    } else {
        asset_root
    };

    let starting_asset = starting_asset.into();

    let mut app = App::new();

    app.add_plugins((
        DefaultPlugins.set(AssetPlugin {
            file_path: asset_root.display().to_string(),
            ..default()
        }),
        LayoutPlugin {
            ignore_unregistered_animations: true,
            ignore_unregistered_attributes: true,
        },
        EguiPlugin,
        bevy_inspector_egui::DefaultInspectorConfigPlugin,
    ))
    .register_asset_loader(SvgLoader::default());

    embedded_asset!(app, "src/", "resources/image.svg");
    embedded_asset!(app, "src/", "resources/layout.svg");
    embedded_asset!(app, "src/", "resources/question.svg");
    embedded_asset!(app, "src/", "resources/text.svg");

    app.init_resource::<EguiIcons>()
        .add_systems(Update, ui_system);

    app.register_type::<UnregisteredData>();
    app.register_type::<PathBuf>();

    {
        let mut registry = app.world.resource::<AppTypeRegistry>().write();

        registry
            .get_mut(TypeId::of::<UnregisteredData>())
            .unwrap()
            .insert(InspectorEguiImpl::new(
                reflect::inspector_ui_unregistered,
                reflect::inspector_ui_readonly_unregistered,
                reflect::inspector_ui_unregistered_many,
            ));

        registry
            .get_mut(TypeId::of::<PathBuf>())
            .unwrap()
            .insert(InspectorEguiImpl::new(
                reflect::inspector_ui_pathbuf,
                reflect::inspector_ui_pathbuf_readonly,
                reflect::inspector_ui_unregistered_many,
            ));
    }

    let asset = app.world.resource::<AssetServer>().load(starting_asset);

    let mut dock_state = DockState::new(vec![EditorTab::Game]);

    dock_state.main_surface_mut().split_left(
        egui_dock::NodeIndex::root(),
        0.3,
        vec![EditorTab::LayoutHierarchyView(asset)],
    );

    app.insert_resource(UiState {
        dock_state,
        game_window: Rect::default(),
        should_render_game: false,
    });

    app
}
