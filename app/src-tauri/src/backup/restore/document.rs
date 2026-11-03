use super::{DOCUMENT_LIMIT, PrepareError, Result};
use serde::{
    Deserializer,
    de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor},
};
use serde_json::{Map, Value};
use std::{fmt, sync::OnceLock};

// Bound allocation while parsing, not only after building a potentially huge tree.
const VALUE_LIMIT: usize = 1_000_000;
const JSON_DEPTH: usize = 64;
struct Seed<'a> {
    remaining: &'a mut usize,
    depth: usize,
}
impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = Value;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> std::result::Result<Value, D::Error> {
        if *self.remaining == 0 || self.depth > JSON_DEPTH {
            return Err(de::Error::custom("resource_limit"));
        }
        *self.remaining -= 1;
        d.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Seed<'_> {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("bounded JSON document")
    }
    fn visit_bool<E: de::Error>(self, v: bool) -> std::result::Result<Value, E> {
        Ok(v.into())
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> std::result::Result<Value, E> {
        Ok(v.into())
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<Value, E> {
        Ok(v.into())
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> std::result::Result<Value, E> {
        Ok(v.into())
    }
    fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Value, E> {
        Ok(v.into())
    }
    fn visit_string<E: de::Error>(self, v: String) -> std::result::Result<Value, E> {
        Ok(v.into())
    }
    fn visit_unit<E: de::Error>(self) -> std::result::Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> std::result::Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(v) = a.next_element_seed(Seed {
            remaining: self.remaining,
            depth: self.depth + 1,
        })? {
            values.push(v);
        }
        Ok(Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> std::result::Result<Value, A::Error> {
        let mut values = Map::new();
        while let Some(k) = a.next_key::<String>()? {
            if values.contains_key(&k) {
                return Err(de::Error::custom("duplicate_key"));
            }
            let v = a.next_value_seed(Seed {
                remaining: self.remaining,
                depth: self.depth + 1,
            })?;
            values.insert(k, v);
        }
        Ok(Value::Object(values))
    }
}
fn incompatible<T>() -> Result<T> {
    Err(PrepareError::IncompatibleDocument)
}
fn schema() -> &'static Value {
    static SCHEMA: OnceLock<Value> = OnceLock::new();
    SCHEMA.get_or_init(|| {
        serde_json::from_str(include_str!("../document-schema-v1.json"))
            .expect("checked editor schema fixture")
    })
}
pub(super) fn validate(json: &str) -> Result<()> {
    if json.len() as u64 > DOCUMENT_LIMIT {
        return Err(PrepareError::ResourceLimit);
    }
    let mut remaining = VALUE_LIMIT;
    let mut parser = serde_json::Deserializer::from_str(json);
    let value = Seed {
        remaining: &mut remaining,
        depth: 0,
    }
    .deserialize(&mut parser)
    .map_err(|e| {
        if e.to_string().contains("resource_limit") {
            PrepareError::ResourceLimit
        } else {
            PrepareError::IncompatibleDocument
        }
    })?;
    parser
        .end()
        .map_err(|_| PrepareError::IncompatibleDocument)?;
    if value["type"] != "doc" {
        return incompatible();
    }
    node(&value, &Value::Array(vec![]))
}
fn attributes(value: Option<&Value>, spec: &Value) -> Result<()> {
    let Some(value) = value else {
        return Ok(());
    };
    let Some(attrs) = value.as_object() else {
        return incompatible();
    };
    for (key, value) in attrs {
        if spec.get(key).is_none() {
            return incompatible();
        }
        let valid = match key.as_str() {
            "level" => value.as_i64().is_some_and(|v| (1..=6).contains(&v)),
            "start" => list_start(value),
            "language" | "type" | "href" | "target" | "rel" | "class" | "title" => {
                value.is_null() || value.is_string()
            }
            _ => false,
        };
        if !valid {
            return incompatible();
        }
    }
    Ok(())
}
// Tiptap uses JavaScript Number, not a 53-bit integer storage format.
// Accept finite integral doubles (including serialized exponential notation).
// Integer tokens must be the shortest round-trip spelling of the JavaScript double.
// A double's decimal spelling can differ from its exact mathematical integer.
fn list_start(value: &Value) -> bool {
    let Some(number) = value.as_f64() else {
        return false;
    };
    if !number.is_finite() || number.fract() != 0.0 {
        return false;
    }
    if let Some(integer) = value.as_i64() {
        return number.to_string() == integer.to_string();
    }
    if let Some(integer) = value.as_u64() {
        return number.to_string() == integer.to_string();
    }
    true
}
fn node(value: &Value, allowed_marks: &Value) -> Result<()> {
    let Some(object) = value.as_object() else {
        return incompatible();
    };
    if object
        .keys()
        .any(|k| !["type", "attrs", "content", "text", "marks"].contains(&k.as_str()))
    {
        return incompatible();
    }
    let Some(name) = value["type"].as_str() else {
        return incompatible();
    };
    let Some(spec) = schema()["nodes"].get(name) else {
        return incompatible();
    };
    attributes(object.get("attrs"), &spec["attrs"])?;
    if let Some(marks) = object.get("marks") {
        let Some(marks) = marks.as_array() else {
            return incompatible();
        };
        let mut previous = None;
        let mut seen = Vec::new();
        for mark in marks {
            let Some(fields) = mark.as_object() else {
                return incompatible();
            };
            if fields
                .keys()
                .any(|k| !["type", "attrs"].contains(&k.as_str()))
            {
                return incompatible();
            }
            let Some(name) = mark["type"].as_str() else {
                return incompatible();
            };
            let Some(spec) = schema()["marks"].get(name) else {
                return incompatible();
            };
            if !allowed_marks.is_null()
                && !allowed_marks
                    .as_array()
                    .is_some_and(|v| v.iter().any(|v| v == name))
            {
                return incompatible();
            }
            let rank = spec["rank"]
                .as_u64()
                .ok_or(PrepareError::IncompatibleDocument)?;
            if previous.is_some_and(|p| rank <= p) {
                return incompatible();
            }
            for other in &seen {
                if spec["excludes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v == other)
                    || schema()["marks"][*other]["excludes"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|v| v == name)
                {
                    return incompatible();
                }
            }
            previous = Some(rank);
            seen.push(name);
            attributes(fields.get("attrs"), &spec["attrs"])?;
        }
    }
    let children = match object.get("content") {
        None => &[][..],
        Some(Value::Array(v)) => v.as_slice(),
        _ => return incompatible(),
    };
    if name == "text" {
        if !children.is_empty() || !value["text"].as_str().is_some_and(|s| !s.is_empty()) {
            return incompatible();
        }
    } else if object.contains_key("text") {
        return incompatible();
    }
    let group = |v: &Value, group: &str| {
        schema()["nodes"]
            .get(v["type"].as_str().unwrap_or(""))
            .and_then(|v| v["groups"].as_array())
            .is_some_and(|v| v.iter().any(|v| v == group))
    };
    let valid = match spec["content"].as_str().unwrap_or("") {
        "block+" => !children.is_empty() && children.iter().all(|v| group(v, "block")),
        "inline*" => children.iter().all(|v| group(v, "inline")),
        "text*" => children.iter().all(|v| v["type"] == "text"),
        "listItem+" => !children.is_empty() && children.iter().all(|v| v["type"] == "listItem"),
        "paragraph block*" => {
            children.first().is_some_and(|v| v["type"] == "paragraph")
                && children.iter().skip(1).all(|v| group(v, "block"))
        }
        "" => children.is_empty(),
        _ => false,
    };
    if !valid {
        return incompatible();
    }
    for child in children {
        node(child, &spec["marks"])?;
    }
    Ok(())
}
