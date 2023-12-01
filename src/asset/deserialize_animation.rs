use std::{marker::PhantomData, sync::Arc};

use bevy::utils::HashMap;
use serde::{
    de::{DeserializeSeed, Visitor},
    Deserialize,
};

use crate::{
    animation::{Animations, NodeAnimation, TimeBezierCurve},
    LayoutAnimationTarget, LayoutRegistryInner,
};

enum AnimationDataFieldId {
    Id,
    TimeMs,
    TimeScale,
    Target,
}

struct AnimationDataFieldIdVisitor;

impl<'de> Visitor<'de> for AnimationDataFieldIdVisitor {
    type Value = AnimationDataFieldId;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("field identifier")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        match v {
            "id" => Ok(AnimationDataFieldId::Id),
            "time_ms" => Ok(AnimationDataFieldId::TimeMs),
            "time_scale" => Ok(AnimationDataFieldId::TimeScale),
            "target" => Ok(AnimationDataFieldId::Target),
            _ => Err(<E as serde::de::Error>::unknown_field(
                v,
                &["id", "time_ms", "time_scale", "target"],
            )),
        }
    }
}

impl<'de> Deserialize<'de> for AnimationDataFieldId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(AnimationDataFieldIdVisitor)
    }
}

struct TargetDeserializer<'de>(&'de LayoutRegistryInner);

impl<'de> Visitor<'de> for TargetDeserializer<'de> {
    type Value = Box<dyn LayoutAnimationTarget>;
    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("registered layout animation")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let key = map.next_key::<String>()?.ok_or_else(|| {
            <A::Error as serde::de::Error>::custom("expected 1 key-value pair in the target map")
        })?;
        let deserializer = self.0.animations.get(key.as_str()).ok_or_else(|| {
            <A::Error as serde::de::Error>::custom(
                format!("Layout animation '{key}' was not registered")
            )
        })?;

        let content = map.next_value::<serde_value::Value>()?;

        (deserializer.deserialize)(content).map_err(|e| <A::Error as serde::de::Error>::custom(e))
    }
}

impl<'de> DeserializeSeed<'de> for TargetDeserializer<'de> {
    type Value = Box<dyn LayoutAnimationTarget>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(self)
    }
}

struct NodeAnimationVisitor<'de>(&'de LayoutRegistryInner);

impl<'de> Visitor<'de> for NodeAnimationVisitor<'de> {
    type Value = NodeAnimation;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("struct NodeAnimation")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut id = None;
        let mut time_ms = None;
        let mut time_scale = None;
        let mut target = None;

        while let Some(key) = map.next_key::<AnimationDataFieldId>()? {
            match key {
                AnimationDataFieldId::Id => {
                    if id.is_some() {
                        return Err(<A::Error as serde::de::Error>::duplicate_field("id"));
                    }

                    id = Some(map.next_value::<String>()?);
                }
                AnimationDataFieldId::TimeMs => {
                    if time_ms.is_some() {
                        return Err(<A::Error as serde::de::Error>::duplicate_field("time_ms"));
                    }

                    time_ms = Some(map.next_value::<f32>()?);
                }
                AnimationDataFieldId::TimeScale => {
                    if time_scale.is_some() {
                        return Err(<A::Error as serde::de::Error>::duplicate_field("time_scale"));
                    }

                    time_scale = Some(map.next_value::<TimeBezierCurve>()?);
                }
                AnimationDataFieldId::Target => {
                    if target.is_some() {
                        return Err(<A::Error as serde::de::Error>::duplicate_field("target"));
                    }

                    target = Some(map.next_value_seed(TargetDeserializer(self.0))?);
                }
            }
        }

        Ok(Self::Value {
            id: id.ok_or_else(|| <A::Error as serde::de::Error>::missing_field("id"))?,
            time_ms: time_ms
                .ok_or_else(|| <A::Error as serde::de::Error>::missing_field("time_ms"))?,
            time_scale: time_scale.unwrap_or_default(),
            target: target.ok_or_else(|| <A::Error as serde::de::Error>::missing_field("target"))?,
        })
    }
}

pub struct NodeAnimationDeserializer<'de>(&'de LayoutRegistryInner);

impl<'de> DeserializeSeed<'de> for NodeAnimationDeserializer<'de> {
    type Value = NodeAnimation;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(NodeAnimationVisitor(self.0))
    }
}

pub struct AnimationListDeserializer<'de>(&'de LayoutRegistryInner);

impl<'de> Visitor<'de> for AnimationListDeserializer<'de> {
    type Value = Vec<NodeAnimation>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("list of NodeAnimation")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let mut values =
            if let Some(hint) = seq.size_hint() {
                Vec::with_capacity(hint)
            } else {
                vec![]
            };

        while let Some(next) = seq.next_element_seed(NodeAnimationDeserializer(self.0))? {
            values.push(next);
        }

        Ok(values)
    }
}

impl<'de> DeserializeSeed<'de> for AnimationListDeserializer<'de> {
    type Value = Vec<NodeAnimation>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(self)
    }
}

pub struct AnimationsDeserializer<'de>(pub(crate) &'de LayoutRegistryInner);

impl<'de> Visitor<'de> for AnimationsDeserializer<'de> {
    type Value = Animations;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("map of animations")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut values = if let Some(hint) = map.size_hint() {
            HashMap::with_capacity(hint)
        } else {
            HashMap::new()
        };

        while let Some((key, value)) =
            map.next_entry_seed(PhantomData::<String>, AnimationListDeserializer(self.0))?
        {
            values.insert(key, value);
        }

        Ok(Animations(Arc::new(values)))
    }
}

impl<'de> DeserializeSeed<'de> for AnimationsDeserializer<'de> {
    type Value = Animations;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(self)
    }
}
