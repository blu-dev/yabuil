use bevy::utils::HashMap;
use serde::de::{DeserializeSeed, Visitor};
use serde::Deserialize;
use std::marker::PhantomData;

pub(crate) struct PhantomVisitor<T>(pub PhantomData<T>);

pub(crate) struct HashMapSeedPassthrough<'de, K, T>(T, PhantomData<&'de K>);

impl<'de, K, T: Clone> Clone for HashMapSeedPassthrough<'de, K, T> {
    fn clone(&self) -> Self {
        Self(self.0.clone(), PhantomData)
    }
}

impl<'de, K, T: Copy> Copy for HashMapSeedPassthrough<'de, K, T> {}

impl<'de, K, T> HashMapSeedPassthrough<'de, K, T> {
    pub fn new(seed: T) -> Self {
        Self(seed, PhantomData)
    }
}

impl<'de, K, T> Visitor<'de> for HashMapSeedPassthrough<'de, K, T>
where
    K: Deserialize<'de> + PartialEq + Eq + std::hash::Hash,
    T: DeserializeSeed<'de> + Copy,
{
    type Value = HashMap<K, <T as DeserializeSeed<'de>>::Value>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("map of values")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut output = HashMap::with_capacity(map.size_hint().unwrap_or_default());
        while let Some(next) = map.next_key::<K>()? {
            output.insert(next, map.next_value_seed(self.0)?);
        }

        Ok(output)
    }
}

impl<'de, K, T> DeserializeSeed<'de> for HashMapSeedPassthrough<'de, K, T>
where
    K: Deserialize<'de> + PartialEq + Eq + std::hash::Hash,
    T: DeserializeSeed<'de> + Copy,
{
    type Value = HashMap<K, <T as DeserializeSeed<'de>>::Value>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(self)
    }
}

#[derive(Copy, Clone)]
pub(crate) struct VecSeedPassthrough<'de, T: DeserializeSeed<'de> + Copy + 'de>(
    T,
    PhantomData<&'de ()>,
);

impl<'de, T: DeserializeSeed<'de> + Copy + 'de> VecSeedPassthrough<'de, T> {
    pub fn new(seed: T) -> Self {
        Self(seed, PhantomData)
    }
}

impl<'de, T: DeserializeSeed<'de> + Copy + 'de> Visitor<'de> for VecSeedPassthrough<'de, T> {
    type Value = Vec<T::Value>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("sequence of values")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let mut list = Vec::with_capacity(seq.size_hint().unwrap_or_default());
        while let Some(next) = seq.next_element_seed(self.0)? {
            list.push(next);
        }

        Ok(list)
    }
}

impl<'de, T: DeserializeSeed<'de> + Copy + 'de> DeserializeSeed<'de>
    for VecSeedPassthrough<'de, T>
{
    type Value = Vec<T::Value>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(self)
    }
}

macro_rules! decl_ident_parse {
    (variant $ty:ident($($name:ident),*)) => {
        paste::paste! {
            #[derive(PartialEq, Eq)]
            enum [<$ty VariantId>] {
                $($name),*
            }

            impl<'de> Visitor<'de> for super::helpers::PhantomVisitor<[<$ty VariantId>]> {
                type Value = [<$ty VariantId>];

                fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                    formatter.write_str("variant identifier")
                }

                fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
                where
                    E: serde::de::Error
                {
                    match v {
                        $(
                            stringify!($name) => Ok([<$ty VariantId>]::$name),
                        )*
                        _ => Err(<E as serde::de::Error>::unknown_variant(
                            v,
                            &[$(stringify!($name)),*]
                        ))
                    }
                }
            }

            impl<'de> Deserialize<'de> for [<$ty VariantId>] {
                fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                where
                    D: serde::Deserializer<'de>,
                {
                    deserializer.deserialize_str(crate::asset::helpers::PhantomVisitor(PhantomData::<Self>))
                }
            }
        }
    };
    (field $ty:ident($($name:ident),*)) => {
        paste::paste! {
            #[derive(PartialEq, Eq)]
            enum [<$ty FieldId>] {
                $($name),*
            }

            impl<'de> Visitor<'de> for super::helpers::PhantomVisitor<[<$ty FieldId>]> {
                type Value = [<$ty FieldId>];

                fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                    formatter.write_str("variant identifier")
                }

                fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
                where
                    E: serde::de::Error
                {
                    match v {
                        $(
                            stringify!([<$name:snake>]) => Ok([<$ty FieldId>]::$name),
                        )*
                        _ => Err(<E as serde::de::Error>::unknown_field(
                            v,
                            &[$(stringify!([<$name:snake>])),*]
                        ))
                    }
                }
            }

            impl<'de> Deserialize<'de> for [<$ty FieldId>] {
                fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                where
                    D: serde::Deserializer<'de>,
                {
                    deserializer.deserialize_str(crate::asset::helpers::PhantomVisitor(PhantomData::<Self>))
                }
            }
        }
    };
}

macro_rules! decl_struct_parse {
    ($this:ident, $field_ty:ident, $map:ident; $(($($t:tt)*)),*; require($($required_field:ident),*); default($($default_field:ident),*)) => {
        paste::paste! {
            $(
                decl_struct_parse!(@decl_field $($t)*);
            )*

            while let Some(key) = $map.next_key::<$field_ty>()? {
                match key {
                    $(
                        decl_struct_parse!{@decl_variant $field_ty; $($t)*} => {
                            decl_struct_parse!(@munch $this, $map; $($t)*);
                        }
                    )*
                }
            }

            $(
                let Some($required_field) = $required_field else {
                    return Err(<A::Error as serde::de::Error>::missing_field(stringify!($required_field)));
                };
            )*

            $(
                let $default_field = $default_field.unwrap_or_default();
            )*
        }
    };
    (@munch $this:ident, $map:ident; passthrough $field:ident => $t:path) => {
        if $field.is_some() {
            return Err(<A::Error as serde::de::Error>::duplicate_field(stringify!($name)));
        }

        $field = Some($map.next_value_seed($t($this.0))?);
    };
    (@decl_field passthrough $field:ident => $t:path) => {
        let mut $field: Option<<$t as DeserializeSeed<'_>>::Value> = None;
    };
    (@decl_variant $field_ty:ident; passthrough $field:ident => $t:path) => {
        paste::paste! {
            $field_ty::[<$field:camel>]
        }
    };
    (@munch $this:ident, $map:ident; $field:ident => $t:path) => {
        if $field.is_some() {
            return Err(<A::Error as serde::de::Error>::duplicate_field(stringify!($name)));
        }

        $field = Some($map.next_value::<$t>()?);
    };
    (@decl_field $field:ident => $t:path) => {
        let mut $field: Option<$t> = None;
    };
    (@decl_variant $field_ty:ident; $field:ident => $t:path) => {
        paste::paste! {
            $field_ty::[<$field:camel>]
        }
    };
}

pub(crate) use decl_ident_parse;
pub(crate) use decl_struct_parse;
