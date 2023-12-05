use std::path::PathBuf;

use bevy::asset::Handle;
use egui::{CollapsingHeader, Id, RichText, Ui};
use yabuil::asset::{Layout, LayoutNode, LayoutNodeData, LayoutNodeInner};

use crate::EguiIcons;

pub enum LayoutViewResponse {
    OpenLayout(Handle<Layout>),
    OpenNode(PathBuf),
    OpenAnimation(String),
}

fn show_node(
    ui: &mut Ui,
    node: &LayoutNode,
    icons: &EguiIcons,
    id: Id,
) -> Option<LayoutViewResponse> {
    let icon = match &node.inner {
        LayoutNodeInner::Null => icons.question,
        LayoutNodeInner::Image(_) => icons.image,
        LayoutNodeInner::Text(_) => icons.text,
        LayoutNodeInner::Layout(_) => icons.layout,
        LayoutNodeInner::Group(nodes) => {
            return CollapsingHeader::new(RichText::new(node.id.as_str()).monospace())
                .id_source(id.with("node-content"))
                .show(ui, |ui| {
                    let mut output = None;
                    for node in nodes.iter() {
                        output = output.or(show_node(ui, node, icons, id.with("node-content")));
                    }
                    output
                })
                .body_returned
                .flatten();
        }
    };

    ui.horizontal(|ui| {
        ui.image(egui::load::SizedTexture {
            id: icon,
            size: egui::Vec2::splat(20.0),
        });

        let mut response = ui.selectable_label(false, RichText::new(node.id.as_str()).monospace());

        let mut layout_response = None;

        if let LayoutNodeInner::Layout(LayoutNodeData { handle, .. }) = &node.inner {
            response = response.context_menu(|ui| {
                if ui.button("Open Layout").clicked() {
                    layout_response = Some(LayoutViewResponse::OpenLayout(handle.clone()));
                    ui.close_menu();
                }
            });
        }

        if layout_response.is_some() {
            return layout_response;
        }

        if response.clicked() {
            Some(LayoutViewResponse::OpenNode(PathBuf::from(node.id.clone())))
        } else {
            None
        }
    })
    .inner
}

pub fn layout_view_ui(
    layout: &mut Layout,
    ui: &mut Ui,
    id: Id,
    icons: &EguiIcons,
) -> Option<LayoutViewResponse> {
    let mut response = CollapsingHeader::new("Nodes")
        .id_source(id.with("nodes"))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x /= 2.0;
            let mut response = None;
            for node in layout.nodes.iter() {
                response = response.or(show_node(ui, node, icons, id.with("nodes")));
            }
            response
        })
        .body_returned
        .flatten();

    response = response.or(CollapsingHeader::new("Animations")
        .id_source(id.with("animations"))
        .show(ui, |ui| {
            let mut response = None;
            for animation in layout.animations.read().unwrap().keys() {
                response = response.or(ui
                    .selectable_label(false, RichText::new(animation).monospace())
                    .clicked()
                    .then(|| animation.clone()));
            }
            response
        })
        .body_returned
        .flatten()
        .map(LayoutViewResponse::OpenAnimation));

    response
}
