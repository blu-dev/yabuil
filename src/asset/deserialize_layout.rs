use bevy::math::{UVec2, Vec2};
use serde::{
    de::{DeserializeSeed, Visitor},
    Deserialize,
};

use crate::{node::Anchor, LayoutAttribute, LayoutRegistryInner};

use super::{deserialize_animation::AnimationsDeserializer, Layout, LayoutNode};

enum LayoutFieldId {
    Resolution,
    CanvasSize,
    Nodes,
    Animations,
}

impl<'de> Deserialize<'de> for LayoutFieldId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(LayoutFieldVisitor)
    }
}

struct LayoutFieldVisitor;

impl<'de> Visitor<'de> for LayoutFieldVisitor {
    type Value = LayoutFieldId;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("field identifier")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        match v {
            "resolution" => Ok(LayoutFieldId::Resolution),
            "canvas_size" => Ok(LayoutFieldId::CanvasSize),
            "nodes" => Ok(LayoutFieldId::Nodes),
            "animations" => Ok(LayoutFieldId::Animations),
            _ => Err(<E as serde::de::Error>::unknown_field(
                v,
                &["resolution", "canvas_size", "nodes", "animations"],
            )),
        }
    }
}

enum NodeFieldId {
    Id,
    Position,
    Size,
    Rotation,
    Anchor,
    Attributes,
    Other(serde_value::Value),
}

struct NodeFieldVisitor;

impl<'de> Visitor<'de> for NodeFieldVisitor {
    type Value = NodeFieldId;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("field identifier")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        match v {
            "id" => Ok(NodeFieldId::Id),
            "position" => Ok(NodeFieldId::Position),
            "size" => Ok(NodeFieldId::Size),
            "rotation" => Ok(NodeFieldId::Rotation),
            "anchor" => Ok(NodeFieldId::Anchor),
            "attributes" => Ok(NodeFieldId::Attributes),
            other => Ok(NodeFieldId::Other(serde_value::Value::String(other.to_string()))),
        }
    }
}

impl<'de> Deserialize<'de> for NodeFieldId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(NodeFieldVisitor)
    }
}

struct NodeListSeed<'de>(&'de LayoutRegistryInner);

impl<'de> DeserializeSeed<'de> for NodeListSeed<'de> {
    type Value = Vec<LayoutNode>;
    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(NodeListVisitor(self.0))
    }
}

struct AttributeMapVisitor<'de>(&'de LayoutRegistryInner);

impl<'de> Visitor<'de> for AttributeMapVisitor<'de> {
    type Value = Vec<Box<dyn LayoutAttribute>>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("map of LayoutNode attributes")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut list =
            if let Some(hint) = map.size_hint() {
                Vec::with_capacity(hint)
            } else {
                vec![]
            };

        while let Some(key) = map.next_key::<String>()? {
            let Some(data) = self.0.attributes.get(key.as_str()) else {
                return Err(<A::Error as serde::de::Error>::custom(
                    format!("LayoutNode attribute '{key}' was not registered")
                ));
            };

            let value = map.next_value::<serde_value::Value>()?;
            let value =
                (data.deserialize)(value).map_err(<A::Error as serde::de::Error>::custom)?;
            list.push(value);
        }

        Ok(list)
    }
}

struct AttributeDeserializer<'de>(&'de LayoutRegistryInner);

impl<'de> DeserializeSeed<'de> for AttributeDeserializer<'de> {
    type Value = Vec<Box<dyn LayoutAttribute>>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(AttributeMapVisitor(self.0))
    }
}

struct NodeVisitor<'de>(&'de LayoutRegistryInner);

impl<'de> Visitor<'de> for NodeVisitor<'de> {
    type Value = LayoutNode;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("struct LayoutNode")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let dupe = |ident: &'static str| -> Result<Self::Value, A::Error> {
            Err(<A::Error as serde::de::Error>::duplicate_field(ident))
        };

        let miss = |ident: &'static str| -> Result<Self::Value, A::Error> {
            Err(<A::Error as serde::de::Error>::missing_field(ident))
        };

        let mut id = None;
        let mut position = None;
        let mut size = None;
        let mut rotation = None;
        let mut anchor = None;
        let mut attributes = None;
        let mut extra_content = Vec::new();

        while let Some(key) = map.next_key::<NodeFieldId>()? {
            match key {
                NodeFieldId::Id => {
                    if id.is_some() {
                        return dupe("id");
                    }

                    id = Some(map.next_value::<String>()?);
                }
                NodeFieldId::Position => {
                    if position.is_some() {
                        return dupe("position");
                    }

                    position = Some(map.next_value::<Vec2>()?);
                }
                NodeFieldId::Size => {
                    if size.is_some() {
                        return dupe("size");
                    }

                    size = Some(map.next_value::<Vec2>()?);
                }
                NodeFieldId::Rotation => {
                    if rotation.is_some() {
                        return dupe("rotation");
                    }

                    rotation = Some(map.next_value::<f32>()?);
                }
                NodeFieldId::Anchor => {
                    if anchor.is_some() {
                        return dupe("anchor");
                    }

                    anchor = Some(map.next_value::<Anchor>()?);
                }
                NodeFieldId::Attributes => {
                    if attributes.is_some() {
                        return dupe("attributes");
                    }

                    attributes = Some(map.next_value_seed(AttributeDeserializer(self.0))?);
                }
                NodeFieldId::Other(value) => {
                    extra_content.push((value, map.next_value::<serde_value::Value>()?));
                }
            }
        }

        if id.is_none() {
            return miss("id");
        }

        if position.is_none() {
            return miss("position");
        }

        if size.is_none() {
            return miss("size");
        }

        if anchor.is_none() {
            return miss("anchor");
        }

        let inner = Deserialize::deserialize(
            serde::de::value::MapDeserializer::new(extra_content.into_iter())
        )
        .map_err(<A::Error as serde::de::Error>::custom)?;

        Ok(Self::Value {
            id: unsafe { id.unwrap_unchecked() },
            position: unsafe { position.unwrap_unchecked() },
            size: unsafe { size.unwrap_unchecked() },
            rotation: rotation.unwrap_or_default(),
            anchor: unsafe { anchor.unwrap_unchecked() },
            inner,
            attributes: attributes.unwrap_or_default(),
        })
    }
}

struct NodeSeed<'de>(&'de LayoutRegistryInner);

impl<'de> DeserializeSeed<'de> for NodeSeed<'de> {
    type Value = LayoutNode;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(NodeVisitor(self.0))
    }
}

struct NodeListVisitor<'de>(&'de LayoutRegistryInner);

impl<'de> Visitor<'de> for NodeListVisitor<'de> {
    type Value = Vec<LayoutNode>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("sequence of LayoutNode")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let mut list =
            if let Some(size) = seq.size_hint() {
                Vec::with_capacity(size)
            } else {
                Vec::new()
            };

        while let Some(next) = seq.next_element_seed(NodeSeed(self.0))? {
            list.push(next);
        }

        Ok(list)
    }
}

struct LayoutVisitor<'de>(&'de LayoutRegistryInner);

impl<'de> Visitor<'de> for LayoutVisitor<'de> {
    type Value = Layout;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("struct Layout")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut resolution = None;
        let mut canvas_size = None;
        let mut nodes = None;
        let mut animations = None;

        while let Some(key) = map.next_key::<LayoutFieldId>()? {
            match key {
                LayoutFieldId::CanvasSize => {
                    if canvas_size.is_some() {
                        return Err(<A::Error as serde::de::Error>::duplicate_field("canvas_size"));
                    }

                    canvas_size = Some(map.next_value::<UVec2>()?);
                }
                LayoutFieldId::Resolution => {
                    if resolution.is_some() {
                        return Err(<A::Error as serde::de::Error>::duplicate_field("resolution"));
                    }

                    resolution = Some(map.next_value::<Option<UVec2>>()?);
                }
                LayoutFieldId::Nodes => {
                    if nodes.is_some() {
                        return Err(<A::Error as serde::de::Error>::duplicate_field("nodes"));
                    }

                    nodes = Some(map.next_value_seed(NodeListSeed(self.0))?);
                }
                LayoutFieldId::Animations => {
                    if animations.is_some() {
                        return Err(<A::Error as serde::de::Error>::duplicate_field("animations"));
                    }

                    animations = Some(map.next_value_seed(AnimationsDeserializer(self.0))?);
                }
            }
        }

        if canvas_size.is_none() {
            return Err(<A::Error as serde::de::Error>::missing_field("canvas_size"));
        }

        if nodes.is_none() {
            return Err(<A::Error as serde::de::Error>::missing_field("nodes"));
        }

        Ok(Self::Value {
            resolution: resolution.unwrap_or_default(),
            canvas_size: unsafe { canvas_size.unwrap_unchecked() },
            nodes: unsafe { nodes.unwrap_unchecked() },
            animations: animations.unwrap_or_default(),
        })
    }
}

struct LayoutDeserializer<'de>(&'de LayoutRegistryInner);

impl<'de> DeserializeSeed<'de> for LayoutDeserializer<'de> {
    type Value = Layout;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(LayoutVisitor(self.0))
    }
}

pub(super) fn deserialize_layout(
    data: &[u8],
    registry: &LayoutRegistryInner,
) -> Result<Layout, serde_json::Error> {
    let mut deserializer = serde_json::Deserializer::from_slice(data);

    LayoutDeserializer(registry).deserialize(&mut deserializer)
}
