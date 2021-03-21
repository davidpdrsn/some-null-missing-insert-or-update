use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::Deserialize;
use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum Patch<T> {
    Some(T),
    ExplicitNull,
    Missing,
}

impl<'de, T> Deserialize<'de> for Patch<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(de: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let inner: PatchInner<T> = Deserialize::deserialize(de)?;

        match inner.0 {
            Some(Some(value)) => Ok(Patch::Some(value)),
            Some(None) => Ok(Patch::ExplicitNull),
            None => todo!("none"),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(transparent)]
struct PatchInner<T>(
    #[serde(bound = "T: Deserialize<'de>", deserialize_with = "double_option")] Option<Option<T>>,
);

fn double_option<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Deserialize::deserialize(de).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Deserialize)]
    struct Payload {
        field: Patch<i32>,
    }

    #[test]
    fn some() {
        let payload = serde_json::from_value::<Payload>(json!({ "field": 1 })).unwrap();
        assert_eq!(payload.field, Patch::Some(1));
    }

    #[test]
    fn explicit_null() {
        let payload = serde_json::from_value::<Payload>(json!({ "field": null })).unwrap();
        assert_eq!(payload.field, Patch::ExplicitNull);
    }

    #[test]
    fn missing() {
        let payload = serde_json::from_value::<Payload>(json!({})).unwrap();
        assert_eq!(payload.field, Patch::Missing);
    }
}
