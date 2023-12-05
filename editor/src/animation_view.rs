use std::path::PathBuf;

use bevy::{math::Vec2, reflect::TypeRegistry};
use bevy_inspector_egui::reflect_inspector::{Context, InspectorUi};
use egui::{
    epaint::{CubicBezierShape, QuadraticBezierShape},
    CollapsingHeader, Color32, ComboBox, DragValue, Grid, Id, Stroke, Ui,
};
use yabuil::animation::{NodeAnimation, TimeBezierCurve};

fn ui_for_bezier_curve(curve: &mut TimeBezierCurve, ui: &mut Ui, id: Id) {
    let variant = match curve {
        TimeBezierCurve::Linear => 0,
        TimeBezierCurve::Quadratic(_) => 1,
        TimeBezierCurve::Cubic(..) => 2,
    };

    let mut new_variant = variant;

    ComboBox::new(id.with("variant-selector"), "Curve Type").show_index(
        ui,
        &mut new_variant,
        3,
        |idx| match idx {
            0 => "Linear",
            1 => "Quadratic",
            2 => "Cubic",
            _ => unreachable!(),
        },
    );

    if new_variant != variant {
        match new_variant {
            0 => *curve = TimeBezierCurve::Linear,
            1 => *curve = TimeBezierCurve::Quadratic(Vec2::splat(0.5)),
            2 => *curve = TimeBezierCurve::Cubic(Vec2::splat(1.0 / 3.0), Vec2::splat(2.0 / 3.0)),
            _ => unreachable!(),
        }
    }

    let (rect, _response) =
        ui.allocate_exact_size(egui::Vec2::splat(100.0), egui::Sense::click_and_drag());

    ui.painter().rect(rect, 0.0, Color32::GRAY, Stroke::NONE);

    match curve {
        TimeBezierCurve::Linear => {
            ui.painter().line_segment(
                [rect.left_bottom(), rect.right_top()],
                Stroke::new(4.0, Color32::BLACK),
            );
        }
        TimeBezierCurve::Quadratic(point) => {
            ui.painter().add(QuadraticBezierShape {
                points: [
                    rect.left_bottom(),
                    rect.left_bottom() + egui::vec2(point.x * 100.0, -point.y * 100.0),
                    rect.right_top(),
                ],
                closed: false,
                fill: Color32::TRANSPARENT,
                stroke: Stroke::new(4.0, Color32::BLACK),
            });
            Grid::new(id.with("quadratic-points")).show(ui, |ui| {
                ui.label("Point");
                Grid::new(id.with("quadratic-point")).show(ui, |ui| {
                    ui.add(DragValue::new(&mut point.x).speed(0.001));
                    ui.add(DragValue::new(&mut point.y).speed(0.001));
                });
            });
        }
        TimeBezierCurve::Cubic(a, b) => {
            ui.painter().add(CubicBezierShape {
                points: [
                    rect.left_bottom(),
                    rect.left_bottom() + egui::vec2(a.x * 100.0, -a.y * 100.0),
                    rect.left_bottom() + egui::vec2(b.x * 100.0, -b.y * 100.0),
                    rect.right_top(),
                ],
                closed: false,
                fill: Color32::TRANSPARENT,
                stroke: Stroke::new(4.0, Color32::BLACK),
            });
            Grid::new(id.with("cubic-points")).show(ui, |ui| {
                ui.label("Point A");
                Grid::new(id.with("cubic-a")).show(ui, |ui| {
                    ui.add(DragValue::new(&mut a.x).speed(0.001));
                    ui.add(DragValue::new(&mut a.y).speed(0.001));
                });
                ui.end_row();
                ui.label("Point B");
                Grid::new(id.with("cubic-b")).show(ui, |ui| {
                    ui.add(DragValue::new(&mut b.x).speed(0.001));
                    ui.add(DragValue::new(&mut b.y).speed(0.001));
                });
                ui.end_row();
            });
        }
    }
}

pub fn animation_view_ui(
    animation: &mut Vec<NodeAnimation>,
    ui: &mut Ui,
    id: Id,
    type_registry: &TypeRegistry,
) -> Option<PathBuf> {
    let mut open = None;

    for (idx, node) in animation.iter_mut().enumerate() {
        CollapsingHeader::new(node.id.as_str())
            .id_source(id.with(idx))
            .show(ui, |ui| {
                Grid::new(id.with(idx).with("content")).show(ui, |ui| {
                    ui.label("Duration (ms)");
                    ui.add(DragValue::new(&mut node.time_ms).clamp_range(0.0..=std::f32::INFINITY));
                    ui.end_row();
                    ui.label("Time Scale");
                    ui_for_bezier_curve(&mut node.time_scale, ui, id.with("time-scale"));
                    ui.end_row();
                    ui.label("Target");

                    crate::reflect::inspector_ui_dynamic(
                        &mut node.target,
                        ui,
                        id.with(idx).with("content"),
                        InspectorUi::new_no_short_circuit(type_registry, &mut Context::default()),
                    );
                });
            })
            .header_response
            .context_menu(|ui| {
                if ui.button("Open Node").clicked() {
                    ui.close_menu();
                    open.replace(PathBuf::from(node.id.clone()));
                }
            });
    }

    open
}
