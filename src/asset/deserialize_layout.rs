use std::marker::PhantomData;

use bevy::{
    asset::LoadContext,
    math::{UVec2, Vec2},
};
use camino::Utf8PathBuf;
use serde::{
    de::{DeserializeSeed, Visitor},
    Deserialize,
};
use serde_value::ValueDeserializer;

use crate::{
    animation::{Keyframes, LayoutAnimation},
    node::Anchor,
    DynamicAttribute, LayoutRegistryInner,
};

use super::{deserialize_animation::RawLayoutAnimationsSeed, Layout, LayoutNode, LayoutNodeInner};

use super::helpers::{decl_ident_parse, decl_struct_parse};

decl_ident_parse!(variant LayoutNode(Null, Image, Text, Layout, Group));
decl_ident_parse!(field Layout(Resolution, CanvasSize, Nodes, Animations));
decl_ident_parse!(field Node(Id, Position, Size, Rotation, Anchor, Attributes, NodeKind, NodeData));

struct AttributeMapVisitor<'de>(&'de LayoutRegistryInner);

impl<'de> Visitor<'de> for AttributeMapVisitor<'de> {
    type Value = Vec<DynamicAttribute>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("map of LayoutNode attributes")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut list = if let Some(hint) = map.size_hint() {
            Vec::with_capacity(hint)
        } else {
            vec![]
        };

        while let Some(key) = map.next_key::<String>()? {
            match self.0.attributes.get(key.as_str()) {
                Some(data) => {
                    let value = map.next_value::<serde_value::Value>()?;
                    let value = (data.deserialize)(value)
                        .map_err(<A::Error as serde::de::Error>::custom)?;
                    list.push(value);
                }
                None => {
                    return Err(<A::Error as serde::de::Error>::custom(format!(
                        "LayoutNode attribute '{key}' was not registered"
                    )));
                }
            }
        }

        Ok(list)
    }
}

struct AttributeDeserializer<'de>(&'de LayoutRegistryInner);

impl<'de> DeserializeSeed<'de> for AttributeDeserializer<'de> {
    type Value = Vec<DynamicAttribute>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(AttributeMapVisitor(self.0))
    }
}

struct NodeSeed<'de>(&'de LayoutRegistryInner);

impl<'de> Visitor<'de> for NodeSeed<'de> {
    type Value = LayoutNode;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("struct LayoutNode")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        decl_struct_parse!(
            self, NodeFieldId, map;
            (id => String),
            (position => Vec2),
            (size => Vec2),
            (rotation => f32),
            (anchor => Anchor),
            (passthrough attributes => AttributeDeserializer),
            (node_kind => LayoutNodeVariantId),
            (node_data => serde_value::Value);
            require(id, position, size, anchor, node_kind);
            default(rotation, attributes)
        );

        let inner = if node_kind == LayoutNodeVariantId::Null {
            if node_data.is_some() {
                return Err(<A::Error as serde::de::Error>::custom(
                    "Null nodes do not have associated node_data",
                ));
            }

            LayoutNodeInner::Null
        } else {
            let Some(node_data) = node_data else {
                return Err(<A::Error as serde::de::Error>::missing_field("node_data"));
            };

            match node_kind {
                LayoutNodeVariantId::Image => LayoutNodeInner::Image(
                    Deserialize::deserialize(ValueDeserializer::<A::Error>::new(node_data))
                        .map_err(<A::Error as serde::de::Error>::custom)?,
                ),
                LayoutNodeVariantId::Text => LayoutNodeInner::Text(
                    Deserialize::deserialize(ValueDeserializer::<A::Error>::new(node_data))
                        .map_err(<A::Error as serde::de::Error>::custom)?,
                ),
                LayoutNodeVariantId::Layout => LayoutNodeInner::Layout(
                    Deserialize::deserialize(ValueDeserializer::<A::Error>::new(node_data))
                        .map_err(<A::Error as serde::de::Error>::custom)?,
                ),
                LayoutNodeVariantId::Group => LayoutNodeInner::Group(
                    NodeListSeed(self.0)
                        .deserialize(ValueDeserializer::<A::Error>::new(node_data))
                        .map_err(<A::Error as serde::de::Error>::custom)?,
                ),
                LayoutNodeVariantId::Null => unreachable!(),
            }
        };

        Ok(Self::Value {
            id,
            position,
            size,
            rotation,
            anchor,
            inner,
            attributes,
        })
    }
}

impl<'de> DeserializeSeed<'de> for NodeSeed<'de> {
    type Value = LayoutNode;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(self)
    }
}

struct NodeListSeed<'de>(&'de LayoutRegistryInner);

impl<'de> DeserializeSeed<'de> for NodeListSeed<'de> {
    type Value = Vec<LayoutNode>;
    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for NodeListSeed<'de> {
    type Value = Vec<LayoutNode>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("sequence of LayoutNode")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let mut list = if let Some(size) = seq.size_hint() {
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

struct LayoutDeserializer<'de, 'a>(&'de LayoutRegistryInner, &'de mut LoadContext<'a>);

impl<'de> Visitor<'de> for LayoutDeserializer<'de, '_> {
    type Value = Layout;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("struct Layout")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        decl_struct_parse!(
            self, LayoutFieldId, map;
            (canvas_size => UVec2),
            (resolution => Option<UVec2>),
            (passthrough nodes => NodeListSeed),
            (passthrough animations => RawLayoutAnimationsSeed);
            require(canvas_size, nodes);
            default(resolution, animations)
        );

        let mut handles = Vec::with_capacity(animations.0.len());

        for (name, node_set) in animations.0 {
            handles.push(self.1.labeled_asset_scope(name, move |_context| {
                LayoutAnimation(
                    node_set
                        .into_iter()
                        .map(|(node_id, keyframes)| {
                            (
                                Utf8PathBuf::from(node_id),
                                Keyframes::flatten_raw_keyframes(keyframes),
                            )
                        })
                        .collect(),
                )
            }));
        }

        Ok(Self::Value {
            resolution,
            canvas_size,
            nodes,
            animations: handles,
        })
    }
}

impl<'de> DeserializeSeed<'de> for LayoutDeserializer<'de, '_> {
    type Value = Layout;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(self)
    }
}

pub(super) fn deserialize_layout<'a>(
    data: &'a [u8],
    registry: &'a LayoutRegistryInner,
    context: &'a mut LoadContext,
) -> Result<Layout, serde_json::Error> {
    let mut deserializer = serde_json::Deserializer::from_slice(data);

    LayoutDeserializer(registry, context).deserialize(&mut deserializer)
}
