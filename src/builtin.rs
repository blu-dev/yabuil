use bevy::prelude::*;
use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    animation::{LayoutAnimationTarget, ResourceRestrictedWorld},
    node::Node,
    views::NodeMut,
};

fn deserialize_color<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Color, D::Error> {
    let [r, g, b, a] = <[f32; 4]>::deserialize(deserializer)?;

    Ok(Color::rgba(r, g, b, a))
}

#[derive(Deserialize, Serialize, Reflect)]
pub struct ColorAnimation(#[serde(deserialize_with = "deserialize_color")] Color);

fn convert_color(color: Color) -> colorgrad::Color {
    colorgrad::Color::new(
        color.r() as f64,
        color.g() as f64,
        color.b() as f64,
        color.a() as f64,
    )
}

fn linear_and_bright(color: Color) -> (Vec4, f32) {
    let [r, g, b, a] = color.as_linear_rgba_f32();
    (Vec4::new(r, g, b, a), (r + g + b + a).powf(0.43))
}

impl LayoutAnimationTarget for ColorAnimation {
    const NAME: &'static str = "Color";

    fn interpolate(
        &self,
        previous: Option<&Self>,
        mut node: NodeMut,
        mut world: ResourceRestrictedWorld,
        progress: f32,
    ) {
        let color = match previous {
            Some(Self(prev_color)) => {
                let (linear_a, bright_a) = linear_and_bright(*prev_color);
                let (linear_b, bright_b) = linear_and_bright(self.0);
                let intensity =
                    (bright_a * (1.0 - progress) + bright_b * progress).powf(0.43f32.recip());
                let mut color = linear_a * (1.0 - progress) + linear_b * progress;
                let sum = color.x + color.y + color.z + color.w;
                if sum != 0.0 {
                    color = color * intensity / sum;
                }
                Color::rgba_linear(color.x, color.y, color.z, color.w)
            }
            None => self.0,
        };

        if let Some(mut image) = node.get_image() {
            image.sprite_data_mut().color = color;
        } else if let Some(mut text) = node.get_text() {
            text.style_mut().color = color;
        } else if let Some(handle) = node.get::<Handle<ColorMaterial>>() {
            world
                .resource_mut::<Assets<ColorMaterial>>()
                .get_mut(handle.id())
                .unwrap()
                .color = color;
        }
    }
}

#[derive(Deserialize, Serialize, Reflect)]
pub struct PositionAnimation(Vec2);

#[derive(Deserialize, Serialize, Reflect)]
pub struct SizeAnimation(Vec2);

impl LayoutAnimationTarget for PositionAnimation {
    const NAME: &'static str = "Position";

    fn interpolate(
        &self,
        previous: Option<&Self>,
        mut node: NodeMut,
        _: ResourceRestrictedWorld,
        progress: f32,
    ) {
        let pos = match previous {
            Some(Self(pos)) => *pos * (1.0 - progress) + self.0 * progress,
            None => self.0,
        };

        node.get_mut::<Node>().unwrap().position = pos;
    }
}

impl LayoutAnimationTarget for SizeAnimation {
    const NAME: &'static str = "Size";

    fn interpolate(
        &self,
        previous: Option<&Self>,
        mut node: NodeMut,
        _: ResourceRestrictedWorld,
        progress: f32,
    ) {
        let size = match previous {
            Some(Self(size)) => *size * (1.0 - progress) + self.0 * progress,
            None => self.0,
        };

        node.get_mut::<Node>().unwrap().size = size;
    }
}

#[derive(Deserialize, Serialize, Reflect)]
pub struct RotationAnimation(f32);

impl LayoutAnimationTarget for RotationAnimation {
    const NAME: &'static str = "Rotation";

    fn interpolate(
        &self,
        previous: Option<&Self>,
        mut node: NodeMut,
        _: ResourceRestrictedWorld,
        progress: f32,
    ) {
        let rotation = match previous {
            Some(Self(angle)) => *angle * (1.0 - progress) + self.0 * progress,
            None => self.0,
        };

        node.get_mut::<Node>().unwrap().rotation = rotation;
    }
}
