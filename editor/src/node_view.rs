use bevy::{math::Vec2, reflect::TypeRegistry, render::color::Color, text::TextAlignment};
use bevy_egui::EguiUserTextures;
use bevy_inspector_egui::reflect_inspector::{Context, InspectorUi};
use egui::{CollapsingHeader, ComboBox, DragValue, Grid, Id, TextEdit, Ui};
use yabuil::{
    asset::{LayoutNode, LayoutNodeInner},
    node::Anchor,
};

fn show_vec(id: impl Into<Id>, vec: &mut Vec2, ui: &mut Ui, min: Vec2, max: Vec2) -> bool {
    let mut changed = false;
    Grid::new(id.into()).show(ui, |ui| {
        changed |= ui
            .add(DragValue::new(&mut vec.x).clamp_range(min.x..=max.x))
            .changed();
        changed |= ui
            .add(DragValue::new(&mut vec.y).clamp_range(min.y..=max.y))
            .changed();
    });
    changed
}

fn show_anchor(id: impl Into<Id>, anchor: &mut Anchor, ui: &mut Ui) -> bool {
    let mut changed = false;

    let list = [
        Anchor::TopLeft,
        Anchor::TopCenter,
        Anchor::TopRight,
        Anchor::CenterLeft,
        Anchor::Center,
        Anchor::CenterRight,
        Anchor::BottomLeft,
        Anchor::BottomCenter,
        Anchor::BottomRight,
    ];

    Grid::new(id.into()).show(ui, |ui| {
        for (idx, value) in list.into_iter().enumerate() {
            if idx != 0 && idx % 3 == 0 {
                ui.end_row();
            }

            changed |= ui.radio_value(anchor, value, "").changed();
        }
    });

    changed
}

fn show_node_contents(
    node: &mut LayoutNodeInner,
    ui: &mut Ui,
    id: Id,
    textures: &mut EguiUserTextures,
    mut size: Vec2,
) -> bool {
    use LayoutNodeInner as L;

    let mut changed = false;

    match node {
        L::Null => {}
        L::Image(data) => {
            CollapsingHeader::new("Image Data")
                .id_source(id)
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label("Image Path");

                            let mut path = data
                                .path
                                .as_ref()
                                .map(|p| p.display().to_string())
                                .unwrap_or_default();
                            ui.add_enabled(false, TextEdit::singleline(&mut path));
                        });

                        ui.horizontal(|ui| {
                            ui.label("Image Preview");
                            let id = textures.add_image(data.handle.clone_weak());

                            let local_size = Vec2::splat(300.0);

                            if local_size.y < size.y {
                                size.x = size.x * local_size.y / size.y;
                                size.y = local_size.y;
                            }

                            if local_size.x < size.x {
                                size.y = size.y * local_size.x / size.x;
                                size.x = local_size.x;
                            }

                            ui.image(egui::load::SizedTexture {
                                id,
                                size: egui::Vec2::new(size.x, size.y),
                            });
                        });

                        ui.horizontal(|ui| {
                            ui.label("Image Tint");

                            if let Some(tint) = data.tint.as_mut() {
                                let mut color = tint.as_rgba_f32();
                                if ui
                                    .color_edit_button_rgba_premultiplied(&mut color)
                                    .changed()
                                {
                                    changed = true;
                                    let [r, g, b, a] = color;
                                    *tint = Color::rgba(r, g, b, a);
                                }
                            } else {
                                let mut color = [1.0; 4];
                                if ui
                                    .color_edit_button_rgba_premultiplied(&mut color)
                                    .changed()
                                {
                                    changed = true;
                                    let [r, g, b, a] = color;
                                    data.tint = Some(Color::rgba(r, g, b, a));
                                }
                            }
                        });
                    });
                });
        }
        L::Text(data) => {
            CollapsingHeader::new("Text Data")
                .id_source(id)
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label("Text");
                            changed |= ui.add(TextEdit::multiline(&mut data.text)).changed();
                        });

                        ui.horizontal(|ui| {
                            ui.label("Font Size");
                            changed |= ui
                                .add(
                                    DragValue::new(&mut data.size)
                                        .clamp_range(1.0..=std::f32::INFINITY),
                                )
                                .changed();
                        });

                        ui.horizontal(|ui| {
                            ui.label("Font Path");

                            let mut path = data
                                .font
                                .as_ref()
                                .map(|p| p.display().to_string())
                                .unwrap_or_default();

                            ui.add_enabled(false, TextEdit::singleline(&mut path));
                        });

                        ui.horizontal(|ui| {
                            ui.label("Color");

                            let mut rgba = data.color.as_rgba_f32();

                            if ui.color_edit_button_rgba_premultiplied(&mut rgba).changed() {
                                changed = true;
                                let [r, g, b, a] = rgba;
                                data.color = Color::rgba(r, g, b, a);
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Alignment");
                            Grid::new(id.with("alignment")).show(ui, |ui| {
                                for value in [
                                    TextAlignment::Left,
                                    TextAlignment::Center,
                                    TextAlignment::Right,
                                ] {
                                    changed |=
                                        ui.radio_value(&mut data.alignment, value, "").changed();
                                }
                                ui.end_row();
                            });
                        });
                    });
                });
        }
        L::Layout(_) => {}
        L::Group(_) => {}
    }
    changed
}

pub fn node_view_ui(
    node: &mut LayoutNode,
    ui: &mut Ui,
    id: Id,
    textures: &mut EguiUserTextures,
    type_registry: &TypeRegistry,
) -> bool {
    let mut changed = false;
    let variant_id = match &mut node.inner {
        LayoutNodeInner::Null => 0,
        LayoutNodeInner::Image(_) => 1,
        LayoutNodeInner::Text(_) => 2,
        LayoutNodeInner::Layout(_) => 3,
        LayoutNodeInner::Group(_) => 4,
    };

    let mut new_id = variant_id;

    ComboBox::new(id.with("id-selector"), "Node Kind").show_index(ui, &mut new_id, 5, |index| {
        match index {
            0 => "Null",
            1 => "Image",
            2 => "Text",
            3 => "Layout",
            4 => "Group",
            _ => unreachable!(),
        }
    });

    if new_id != variant_id {
        changed = true;
        match new_id {
            0 => node.inner = LayoutNodeInner::Null,
            1 => node.inner = LayoutNodeInner::Image(Default::default()),
            2 => node.inner = LayoutNodeInner::Text(Default::default()),
            3 => node.inner = LayoutNodeInner::Layout(Default::default()),
            4 => node.inner = LayoutNodeInner::Group(Default::default()),
            _ => unreachable!(),
        }
    }

    Grid::new(id.with("node-metadata")).show(ui, |ui| {
        ui.label("Position");
        changed |= show_vec(
            id.with("position"),
            &mut node.position,
            ui,
            Vec2::NEG_INFINITY,
            Vec2::INFINITY,
        );
        ui.end_row();
        ui.label("Size");
        changed |= show_vec(
            id.with("size"),
            &mut node.size,
            ui,
            Vec2::ZERO,
            Vec2::INFINITY,
        );
        ui.end_row();
        ui.label("Anchor");
        changed |= show_anchor(id.with("anchor"), &mut node.anchor, ui);
        ui.end_row();
    });
    changed |= show_node_contents(
        &mut node.inner,
        ui,
        id.with("node-contents"),
        textures,
        node.size,
    );
    CollapsingHeader::new("Attributes")
        .id_source(id.with("attributes"))
        .show(ui, |ui| {
            let id = id.with("attributes");

            for (idx, node) in node.attributes.iter_mut().enumerate() {
                changed |= crate::reflect::inspector_ui_dynamic(
                    node,
                    ui,
                    id.with(idx),
                    InspectorUi::new_no_short_circuit(type_registry, &mut Context::default()),
                );
            }
        });
    changed
}
