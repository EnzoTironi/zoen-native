//! Packet byte, depth and node budgets for strict JSON before Loro sees input.

use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::Serialize;
use serde_json::Value;
use std::io::{self, Write};

use super::super::{error, PlanError, Result};
use super::MAX_PLAN_PACKET_BYTES;

pub(super) const MAX_JSON_DEPTH: usize = 32;
pub(super) const MAX_JSON_NODES: usize = 262_144;

pub(in crate::plan) fn strict_value(bytes: &[u8]) -> Result<Value> {
    strict_json(bytes)
}

pub(in crate::plan) fn bounded_json(value: &impl Serialize) -> Result<Vec<u8>> {
    let mut writer = BoundedWriter(Vec::new());
    serde_json::to_writer(&mut writer, value).map_err(|_| PlanError::Limit("plan packet bytes"))?;
    Ok(writer.0)
}

struct BoundedWriter(Vec<u8>);

impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_PLAN_PACKET_BYTES.saturating_sub(self.0.len()) {
            return Err(io::Error::other("plan packet bytes"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct JsonBudget {
    remaining: usize,
    exhausted: Option<&'static str>,
}

struct JsonNode<'a> {
    budget: &'a mut JsonBudget,
    depth: usize,
}

impl<'de> DeserializeSeed<'de> for JsonNode<'_> {
    type Value = Value;

    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        de: D,
    ) -> std::result::Result<Value, D::Error> {
        use serde::de::Error;
        if self.depth > MAX_JSON_DEPTH {
            self.budget.exhausted = Some("plan JSON depth");
            return Err(D::Error::custom("plan JSON depth"));
        }
        if self.budget.remaining == 0 {
            self.budget.exhausted = Some("plan JSON nodes");
            return Err(D::Error::custom("plan JSON nodes"));
        }
        self.budget.remaining -= 1;
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for JsonNode<'_> {
    type Value = Value;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("bounded JSON without duplicate object keys")
    }

    fn visit_bool<E: serde::de::Error>(self, value: bool) -> std::result::Result<Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E: serde::de::Error>(self, value: i64) -> std::result::Result<Value, E> {
        Ok(Value::from(value))
    }

    fn visit_u64<E: serde::de::Error>(self, value: u64) -> std::result::Result<Value, E> {
        Ok(Value::from(value))
    }

    fn visit_f64<E: serde::de::Error>(self, value: f64) -> std::result::Result<Value, E> {
        serde_json::Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("nonfinite JSON number"))
    }

    fn visit_str<E: serde::de::Error>(self, value: &str) -> std::result::Result<Value, E> {
        Ok(Value::String(value.into()))
    }

    fn visit_string<E: serde::de::Error>(self, value: String) -> std::result::Result<Value, E> {
        Ok(Value::String(value))
    }

    fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> std::result::Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = seq.next_element_seed(JsonNode {
            budget: self.budget,
            depth: self.depth + 1,
        })? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<Value, A::Error> {
        use serde::de::Error;
        let mut values = serde_json::Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(A::Error::custom("duplicate plan JSON key"));
            }
            let value = map.next_value_seed(JsonNode {
                budget: self.budget,
                depth: self.depth + 1,
            })?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}

pub(super) fn strict_json(bytes: &[u8]) -> Result<Value> {
    let mut de = serde_json::Deserializer::from_slice(bytes);
    let mut budget = JsonBudget {
        remaining: MAX_JSON_NODES,
        exhausted: None,
    };
    let value = JsonNode {
        budget: &mut budget,
        depth: 0,
    }
    .deserialize(&mut de);
    if let Some(limit) = budget.exhausted {
        return Err(PlanError::Limit(limit));
    }
    let value = value.map_err(error)?;
    de.end().map_err(error)?;
    Ok(value)
}

pub(super) fn json_depth(value: &Value, depth: usize) -> Result<()> {
    if depth > MAX_JSON_DEPTH {
        return Err(PlanError::Limit("plan JSON depth"));
    }
    match value {
        Value::Array(values) => {
            for value in values {
                json_depth(value, depth + 1)?;
            }
        }
        Value::Object(values) => {
            for value in values.values() {
                json_depth(value, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}
