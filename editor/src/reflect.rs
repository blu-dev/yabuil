use std::{
    any::{Any, TypeId},
    path::PathBuf,
    ptr::NonNull,
};

use bevy::reflect::{Reflect, ReflectFromPtr};
use bevy_inspector_egui::{
    inspector_egui_impls::InspectorEguiImpl, reflect_inspector::InspectorUi,
};
use egui::{CollapsingHeader, ComboBox, DragValue, Grid, Id, Ui};
use yabuil::{asset::UnregisteredData, DynamicAnimationTarget, DynamicAttribute};

fn ui_for_value(value: &mut serde_json::Value, ui: &mut Ui, id: Id) -> bool {
    use serde_json::Value as V;

    let mut changed = false;

    let variant = match value {
        V::Null => 0,
        V::Bool(_) => 1,
        V::Number(_) => 2,
        V::String(_) => 3,
        V::Array(_) => 4,
        V::Object(_) => 5,
    };

    let mut new_variant = variant;

    ComboBox::new(id.with("kind-selector"), "Value Kind").show_index(
        ui,
        &mut new_variant,
        6,
        |idx| match idx {
            0 => "Null",
            1 => "Bool",
            2 => "Number",
            3 => "String",
            4 => "Array",
            5 => "Object",
            _ => unreachable!(),
        },
    );

    if new_variant != variant {
        changed = true;
        match new_variant {
            0 => *value = V::Null,
            1 => *value = V::Bool(Default::default()),
            2 => *value = V::Number(serde_json::Number::from_f64(0.0).unwrap()),
            3 => *value = V::String(Default::default()),
            4 => *value = V::Array(Default::default()),
            5 => *value = V::Object(Default::default()),
            _ => unreachable!(),
        }
    }

    match value {
        V::Null => {}
        V::Bool(v) => {
            changed |= ui.checkbox(v, "Value").clicked();
        }
        V::Number(number) => {
            ui.horizontal(|ui| {
                ui.label("Value");

                let mut value = number.as_f64().unwrap();

                changed |= ui.add(DragValue::new(&mut value)).changed();

                *number = serde_json::Number::from_f64(value).unwrap();
            });
        }
        V::String(string) => {
            ui.horizontal(|ui| {
                ui.label("Value");
                changed |= ui.text_edit_singleline(string).changed();
            });
        }
        V::Array(array) => {
            Grid::new(id.with("array-contents")).show(ui, |ui| {
                let mut to_remove = None;
                for (idx, value) in array.iter_mut().enumerate() {
                    if ui.small_button("-").clicked() {
                        to_remove = Some(idx);
                        changed = true;
                    }

                    ui.label(format!("{idx}"));
                    ui.vertical(|ui| {
                        changed |= ui_for_value(value, ui, id.with(idx));
                    });
                    ui.end_row();
                }

                if let Some(remove) = to_remove {
                    array.remove(remove);
                }
            });
        }
        V::Object(map) => {
            Grid::new(id.with("object-contents")).show(ui, |ui| {
                let mut to_remove = None;
                for (key, value) in map.iter_mut() {
                    if ui.small_button("-").clicked() {
                        to_remove = Some(key.clone());
                        changed = true
                    }

                    ui.label(key);
                    ui.vertical(|ui| {
                        changed |= ui_for_value(value, ui, id.with(key));
                    });
                    ui.end_row();
                }

                if let Some(remove) = to_remove {
                    let _ = map.remove(remove.as_str());
                }
            });
        }
    }

    changed
}

pub fn inspector_ui_pathbuf(
    value: &mut dyn Any,
    ui: &mut Ui,
    _options: &dyn Any,
    _id: Id,
    _inspector_ui: InspectorUi,
) -> bool {
    let path = value.downcast_mut::<PathBuf>().unwrap();
    let mut string = path.display().to_string();
    if ui.text_edit_singleline(&mut string).changed() {
        *path = PathBuf::from(string);
        true
    } else {
        false
    }
}

pub fn inspector_ui_pathbuf_readonly(
    value: &dyn Any,
    ui: &mut Ui,
    _options: &dyn Any,
    _id: Id,
    _inspector_ui: InspectorUi,
) {
    let path = value.downcast_ref::<PathBuf>().unwrap();
    let mut string = path.display().to_string();
    ui.text_edit_singleline(&mut string);
}

pub fn inspector_ui_unregistered(
    value: &mut dyn Any,
    ui: &mut Ui,
    _options: &dyn Any,
    id: Id,
    _inspector_ui: InspectorUi,
) -> bool {
    let value = value.downcast_mut::<UnregisteredData>().unwrap();
    ui_for_value(&mut value.value, ui, id)
}

pub fn inspector_ui_readonly_unregistered(
    _value: &dyn Any,
    _ui: &mut Ui,
    _options: &dyn Any,
    _id: Id,
    _inspector_ui: InspectorUi,
) {
}

pub fn inspector_ui_unregistered_many(
    _: &mut egui::Ui,
    _: &dyn Any,
    _: egui::Id,
    _: InspectorUi,
    _: &mut [&mut dyn Reflect],
    _: &dyn Fn(&mut dyn Reflect) -> &mut dyn Reflect,
) -> bool {
    false
}

pub fn inspector_ui_dynamic(
    value: &mut dyn Any,
    ui: &mut Ui,
    id: Id,
    mut inspector_ui: InspectorUi,
) -> bool {
    let show_ui = |name, value: &mut dyn Any, type_id: TypeId| {
        CollapsingHeader::new(name)
            .id_source(id)
            .show(ui, move |ui| {
                // First check if there is a registered override for this value
                if let Some(ui_impl) = inspector_ui
                    .type_registry
                    .get_type_data::<InspectorEguiImpl>(type_id)
                {
                    ui_impl.execute(value, ui, &(), id.with("contents"), inspector_ui)
                } else if let Some(reflect_from_ptr) = inspector_ui
                    .type_registry
                    .get_type_data::<ReflectFromPtr>(type_id)
                {
                    // SAFETY: Forcing an unsafe block here for this comment
                    //  We need to provide bevy a raw pointer for the reflect-from-ptr to work,
                    //  we guarantee above that reflect_from_ptr will be valid for this value
                    //  as we check the type id
                    unsafe {
                        let ptr = value as *mut dyn Any as *mut u8;
                        let reflect = reflect_from_ptr
                            .as_reflect_mut(bevy::ptr::PtrMut::new(NonNull::new(ptr).unwrap()));

                        inspector_ui.ui_for_reflect(reflect, ui)
                    }
                } else {
                    ui.label("Unable to get UI implementation for this item");
                    false
                }
            })
            .body_returned
            .unwrap_or_default()
    };

    if let Some(attribute) = value.downcast_mut::<DynamicAttribute>() {
        let type_id = attribute.as_any().type_id();
        show_ui(
            attribute.name().to_string(),
            attribute.as_any_mut(),
            type_id,
        )
    } else {
        let target = value.downcast_mut::<DynamicAnimationTarget>().unwrap();
        let type_id = target.as_any().type_id();
        show_ui(target.name().to_string(), target.as_any_mut(), type_id)
    }
}
