use bevy::prelude::*;
use serde::{Deserialize, Deserializer, Serialize};

use crate::{animation::LayoutAnimationTarget, components::NodeKind, node::Node};

fn deserialize_color<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Color, D::Error> {
    let [r, g, b, a] = <[f32; 4]>::deserialize(deserializer)?;

    Ok(Color::rgba(r, g, b, a))
}

#[derive(Deserialize, Serialize, Reflect)]
pub struct ColorAnimation(#[serde(deserialize_with = "deserialize_color")] Color);

impl LayoutAnimationTarget for ColorAnimation {
    const NAME: &'static str = "Color";

    fn interpolate(&self, previous: Option<&Self>, mut node: EntityMut, progress: f32) {
        let color = match previous {
            Some(Self(prev_color)) => {
                let a = (1.0 - progress) * prev_color.a() + progress * self.0.a();
                let new_hsl = prev_color.as_hsla() * (1.0 - progress) + self.0.as_hsla() * progress;
                new_hsl.with_a(a)
            }
            None => self.0,
        };

        match *node.get::<NodeKind>().unwrap() {
            NodeKind::Text => {
                node.get_mut::<Text>().unwrap().sections[0].style.color = color;
            }
            NodeKind::Image => {
                node.get_mut::<Sprite>().unwrap().color = color;
            }
            _other => {}
        }
    }
}

#[derive(Deserialize, Serialize, Reflect)]
pub struct PositionAnimation(Vec2);

#[derive(Deserialize, Serialize, Reflect)]
pub struct SizeAnimation(Vec2);

impl LayoutAnimationTarget for PositionAnimation {
    const NAME: &'static str = "Position";

    fn interpolate(&self, previous: Option<&Self>, mut node: EntityMut, progress: f32) {
        let pos = match previous {
            Some(Self(pos)) => *pos * (1.0 - progress) + self.0 * progress,
            None => self.0,
        };

        node.get_mut::<Node>().unwrap().position = pos;
    }
}

impl LayoutAnimationTarget for SizeAnimation {
    const NAME: &'static str = "Size";

    fn interpolate(&self, previous: Option<&Self>, mut node: EntityMut, progress: f32) {
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

    fn interpolate(&self, previous: Option<&Self>, mut node: EntityMut, progress: f32) {
        let rotation = match previous {
            Some(Self(angle)) => *angle * (1.0 - progress) + self.0 * progress,
            None => self.0,
        };

        node.get_mut::<Node>().unwrap().rotation = rotation;
    }
}
