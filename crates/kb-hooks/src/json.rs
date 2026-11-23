//! JSON that keeps the order of an object's keys, for editing other
//! programs' settings files: a key the user wrote first stays first.
//!
//! serde_json keeps order only with its `preserve_order` feature, which
//! would change every map of every crate built with this one (the app
//! included). So the files are read into [`Json`] here instead: serde_json
//! still does the strict parsing (no comments, no trailing commas), and
//! hands each object's entries over in the order they were written.

use std::fmt;

use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeMap, SerializeSeq, Serializer};
use serde::{Deserialize, Serialize};
use serde_json::Number;

/// A JSON value whose objects keep their keys' order (and any duplicate
/// keys, as written).
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    /// Strict JSON, as serde_json reads it.
    pub fn parse(text: &str) -> Result<Json, String> {
        // A byte-order mark is not JSON, but Windows editors write one.
        let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
        serde_json::from_str(text).map_err(|e| e.to_string())
    }

    pub fn object() -> Json {
        Json::Object(Vec::new())
    }

    pub fn str(s: &str) -> Json {
        Json::String(s.to_string())
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<Json>> {
        match self {
            Json::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Json>> {
        match self {
            Json::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&Vec<(String, Json)>> {
        match self {
            Json::Object(o) => Some(o),
            _ => None,
        }
    }

    pub fn as_object_mut(&mut self) -> Option<&mut Vec<(String, Json)>> {
        match self {
            Json::Object(o) => Some(o),
            _ => None,
        }
    }

    /// The value of `key` in an object (the first, should it be there
    /// twice).
    pub fn get(&self, key: &str) -> Option<&Json> {
        self.as_object()?.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Json> {
        self.as_object_mut()?.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// Set `key` in an object: in its place when it is there, last
    /// otherwise. Nothing happens to anything but an object.
    pub fn set(&mut self, key: &str, value: Json) {
        if let Some(o) = self.as_object_mut() {
            match o.iter_mut().find(|(k, _)| k == key) {
                Some((_, v)) => *v = value,
                None => o.push((key.to_string(), value)),
            }
        }
    }

    /// Take `key` out of an object, keeping the others' order.
    pub fn remove(&mut self, key: &str) -> Option<Json> {
        let o = self.as_object_mut()?;
        let at = o.iter().position(|(k, _)| k == key)?;
        Some(o.remove(at).1)
    }

    /// `text`, pretty, indented like `like` (a file's own indentation: its
    /// first indented line), two spaces when it has none; with a newline at
    /// the end.
    pub fn to_pretty(&self, like: Option<&str>) -> String {
        let indent = like.and_then(indentation).unwrap_or_else(|| "  ".to_string());
        let mut out = Vec::new();
        let formatter = serde_json::ser::PrettyFormatter::with_indent(indent.as_bytes());
        let mut ser = serde_json::Serializer::with_formatter(&mut out, formatter);
        // Writing to memory cannot fail, and every value here is plain JSON.
        let _ = self.serialize(&mut ser);
        let mut text = String::from_utf8(out).unwrap_or_default();
        text.push('\n');
        text
    }
}

/// The indentation of `text`'s first indented line: spaces or tabs.
fn indentation(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let lead: String = line.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
        let rest = &line[lead.len()..];
        (!lead.is_empty() && !rest.is_empty() && lead.len() <= 8).then_some(lead)
    })
}

impl From<&serde_json::Value> for Json {
    fn from(v: &serde_json::Value) -> Self {
        match v {
            serde_json::Value::Null => Json::Null,
            serde_json::Value::Bool(b) => Json::Bool(*b),
            serde_json::Value::Number(n) => Json::Number(n.clone()),
            serde_json::Value::String(s) => Json::String(s.clone()),
            serde_json::Value::Array(a) => Json::Array(a.iter().map(Json::from).collect()),
            serde_json::Value::Object(o) => Json::Object(o.iter().map(|(k, v)| (k.clone(), Json::from(v))).collect()),
        }
    }
}

impl Serialize for Json {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Json::Null => s.serialize_unit(),
            Json::Bool(b) => s.serialize_bool(*b),
            Json::Number(n) => n.serialize(s),
            Json::String(t) => s.serialize_str(t),
            Json::Array(a) => {
                let mut seq = s.serialize_seq(Some(a.len()))?;
                for v in a {
                    seq.serialize_element(v)?;
                }
                seq.end()
            }
            Json::Object(o) => {
                let mut map = s.serialize_map(Some(o.len()))?;
                for (k, v) in o {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for Json {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(JsonVisitor)
    }
}

struct JsonVisitor;

impl<'de> Visitor<'de> for JsonVisitor {
    type Value = Json;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JSON")
    }

    fn visit_unit<E: de::Error>(self) -> Result<Json, E> {
        Ok(Json::Null)
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Json, E> {
        Ok(Json::Bool(v))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Json, E> {
        Ok(Json::Number(v.into()))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Json, E> {
        Ok(Json::Number(v.into()))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Json, E> {
        Number::from_f64(v).map(Json::Number).ok_or_else(|| E::custom("not a JSON number"))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Json, E> {
        Ok(Json::String(v.to_string()))
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<Json, E> {
        Ok(Json::String(v))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Json, A::Error> {
        let mut out = Vec::new();
        while let Some(v) = seq.next_element()? {
            out.push(v);
        }
        Ok(Json::Array(out))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Json, A::Error> {
        let mut out = Vec::new();
        while let Some((k, v)) = map.next_entry::<String, Json>()? {
            out.push((k, v));
        }
        Ok(Json::Object(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_keys_keep_their_order_and_text_comes_back_the_same() {
        let text = "{\n  \"zeta\": 1,\n  \"alpha\": {\n    \"b\": [\n      true,\n      null,\n      \"x\"\n    ],\n    \"a\": -2.5\n  },\n  \"mid\": \"\u{e7}ay\"\n}\n";
        let v = Json::parse(text).unwrap();
        let keys: Vec<&str> = v.as_object().unwrap().iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["zeta", "alpha", "mid"]);
        assert_eq!(v.to_pretty(Some(text)), text);
    }

    #[test]
    fn the_files_indentation_is_kept() {
        let four = "{\n    \"a\": 1\n}\n";
        assert_eq!(Json::parse(four).unwrap().to_pretty(Some(four)), four);
        let tabs = "{\n\t\"a\": [\n\t\t1\n\t]\n}\n";
        assert_eq!(Json::parse(tabs).unwrap().to_pretty(Some(tabs)), tabs);
        assert_eq!(Json::parse("{\"a\":1}").unwrap().to_pretty(Some("{\"a\":1}")), "{\n  \"a\": 1\n}\n");
    }

    #[test]
    fn only_strict_json_is_read() {
        for bad in ["{\"a\": 1,}", "{// x\n}", "{'a': 1}", "", "{\"a\": 1} x", "[1,]"] {
            assert!(Json::parse(bad).is_err(), "{bad:?}");
        }
        assert!(Json::parse("\u{FEFF}{}").is_ok(), "a byte-order mark is allowed");
        // Big numbers stay as they were.
        let big = "{\n  \"n\": 18446744073709551615,\n  \"m\": -9223372036854775808\n}\n";
        assert_eq!(Json::parse(big).unwrap().to_pretty(Some(big)), big);
    }

    #[test]
    fn editing_keeps_the_rest_in_place() {
        let mut v = Json::parse(r#"{"a":1,"b":2,"c":3}"#).unwrap();
        v.set("b", Json::str("two"));
        v.set("d", Json::Bool(false));
        assert_eq!(v.remove("a"), Some(Json::Number(1.into())));
        assert_eq!(serde_json::to_string(&v).unwrap(), r#"{"b":"two","c":3,"d":false}"#);
        assert!(v.get("a").is_none());
        assert_eq!(v.get("c"), Some(&Json::Number(3.into())));
    }
}
