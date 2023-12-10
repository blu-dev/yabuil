use serde::{
    de::{DeserializeSeed, Visitor},
    Deserialize, Deserializer,
};

use std::marker::PhantomData;

use crate::{
    animation::{DynamicAnimationTarget, RawKeyframe, RawLayoutAnimations, TimeBezierCurve},
    LayoutRegistryInner,
};

use super::helpers::{
    decl_ident_parse, decl_struct_parse, HashMapSeedPassthrough, VecSeedPassthrough,
};

decl_ident_parse!(
    field RawKeyframe(TimestampMs, TimeScale, Targets)
);

pub(crate) struct RawLayoutAnimationsSeed<'de>(pub(crate) &'de LayoutRegistryInner);

impl<'de> DeserializeSeed<'de> for RawLayoutAnimationsSeed<'de> {
    type Value = RawLayoutAnimations;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        let inner = deserializer.deserialize_map(HashMapSeedPassthrough::new(
            HashMapSeedPassthrough::new(VecSeedPassthrough::new(RawKeyframeSeed(self.0))),
        ))?;

        Ok(RawLayoutAnimations(inner))
    }
}

#[derive(Copy, Clone)]
struct RawKeyframeSeed<'de>(&'de LayoutRegistryInner);

impl<'de> Visitor<'de> for RawKeyframeSeed<'de> {
    type Value = RawKeyframe;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("struct RawKeyframe")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        decl_struct_parse!(
            self, RawKeyframeFieldId, map;
            (timestamp_ms => usize),
            (time_scale => TimeBezierCurve),
            (passthrough targets => TargetListSeed);
            require(timestamp_ms, targets);
            default(time_scale)
        );

        Ok(Self::Value {
            timestamp_ms,
            time_scale,
            targets,
        })
    }
}

impl<'de> DeserializeSeed<'de> for RawKeyframeSeed<'de> {
    type Value = RawKeyframe;
    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(self)
    }
}

struct TargetListSeed<'de>(&'de LayoutRegistryInner);

impl<'de> Visitor<'de> for TargetListSeed<'de> {
    type Value = Vec<DynamicAnimationTarget>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("map of LayoutAnimationTarget")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut list = Vec::with_capacity(map.size_hint().unwrap_or_default());
        while let Some(key) = map.next_key::<String>()? {
            match self.0.animations.get(key.as_str()) {
                Some(data) => {
                    let content = map.next_value::<serde_value::Value>()?;
                    list.push(
                        (data.deserialize)(content)
                            .map_err(<A::Error as serde::de::Error>::custom)?,
                    );
                }
                None if self.0.ignore_unknown_registry_data => {
                    log::trace!("Ignoring unregistered LayoutAnimationTarget {key}");
                    let _ = map.next_value::<serde_value::Value>()?;
                }
                None => {
                    return Err(<A::Error as serde::de::Error>::custom(format!(
                        "LayoutAnimationTarget {key} is not registered"
                    )));
                }
            }
        }

        Ok(list)
    }
}

impl<'de> DeserializeSeed<'de> for TargetListSeed<'de> {
    type Value = Vec<DynamicAnimationTarget>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(self)
    }
}
