//! Lenient serde deserializers for MCP tool parameters.
//!
//! Some MCP clients serialize values as strings (e.g. `"30"` instead of `30`,
//! `"true"` instead of `true`). These helpers accept both representations so
//! Julie doesn't reject valid tool calls.

use std::fmt;

use serde::de;

/// Deserializes a `u32` that may arrive as a JSON number or a string.
///
/// Use with `#[serde(deserialize_with = "deserialize_u32_lenient")]`.
pub fn deserialize_u32_lenient<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: de::Deserializer<'de>,
{
    struct U32OrString;

    impl<'de> de::Visitor<'de> for U32OrString {
        type Value = u32;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("u32 or string-encoded u32")
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<u32, E> {
            u32::try_from(v).map_err(|_| E::custom(format!("u32 overflow: {v}")))
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<u32, E> {
            u32::try_from(v).map_err(|_| E::custom(format!("invalid u32: {v}")))
        }

        fn visit_f64<E: de::Error>(self, v: f64) -> Result<u32, E> {
            if v.fract() == 0.0 && v >= 0.0 && v <= u32::MAX as f64 {
                Ok(v as u32)
            } else {
                Err(E::custom(format!("invalid u32: {v}")))
            }
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<u32, E> {
            v.trim()
                .parse()
                .map_err(|_| E::custom(format!("invalid u32 string: \"{v}\"")))
        }
    }

    deserializer.deserialize_any(U32OrString)
}

/// Deserializes an `Option<u32>` that may arrive as a JSON number, string, or null.
///
/// Use with `#[serde(default, deserialize_with = "deserialize_option_u32_lenient")]`.
pub fn deserialize_option_u32_lenient<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: de::Deserializer<'de>,
{
    struct OptionU32OrString;

    impl<'de> de::Visitor<'de> for OptionU32OrString {
        type Value = Option<u32>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("u32, string-encoded u32, or null")
        }

        fn visit_none<E: de::Error>(self) -> Result<Option<u32>, E> {
            Ok(None)
        }

        fn visit_unit<E: de::Error>(self) -> Result<Option<u32>, E> {
            Ok(None)
        }

        fn visit_some<D2: de::Deserializer<'de>>(
            self,
            deserializer: D2,
        ) -> Result<Option<u32>, D2::Error> {
            deserialize_u32_lenient(deserializer).map(Some)
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<Option<u32>, E> {
            u32::try_from(v)
                .map(Some)
                .map_err(|_| E::custom(format!("u32 overflow: {v}")))
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<Option<u32>, E> {
            u32::try_from(v)
                .map(Some)
                .map_err(|_| E::custom(format!("invalid u32: {v}")))
        }

        fn visit_f64<E: de::Error>(self, v: f64) -> Result<Option<u32>, E> {
            if v.fract() == 0.0 && v >= 0.0 && v <= u32::MAX as f64 {
                Ok(Some(v as u32))
            } else {
                Err(E::custom(format!("invalid u32: {v}")))
            }
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Option<u32>, E> {
            v.trim()
                .parse()
                .map(Some)
                .map_err(|_| E::custom(format!("invalid u32 string: \"{v}\"")))
        }
    }

    deserializer.deserialize_any(OptionU32OrString)
}

/// Deserializes a `bool` that may arrive as a JSON boolean or a string.
///
/// Accepts: `true`, `false`, `"true"`, `"false"`, `"1"`, `"0"`.
/// Use with `#[serde(deserialize_with = "deserialize_bool_lenient")]`.
pub fn deserialize_bool_lenient<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: de::Deserializer<'de>,
{
    struct BoolOrString;

    impl<'de> de::Visitor<'de> for BoolOrString {
        type Value = bool;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("bool or string-encoded bool")
        }

        fn visit_bool<E: de::Error>(self, v: bool) -> Result<bool, E> {
            Ok(v)
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<bool, E> {
            match v.trim().to_lowercase().as_str() {
                "true" | "1" | "yes" => Ok(true),
                "false" | "0" | "no" => Ok(false),
                _ => Err(E::custom(format!("invalid bool string: \"{v}\""))),
            }
        }
    }

    deserializer.deserialize_any(BoolOrString)
}

/// Deserializes an `Option<i64>` that may arrive as a JSON number, string,
/// empty string, or null. Empty strings and null deserialize to `None` so
/// clients can clear a field by sending `""`.
///
/// Use with `#[serde(default, deserialize_with = "deserialize_option_i64_lenient")]`.
pub fn deserialize_option_i64_lenient<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: de::Deserializer<'de>,
{
    struct OptionI64OrString;

    impl<'de> de::Visitor<'de> for OptionI64OrString {
        type Value = Option<i64>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("i64, string-encoded i64, or null")
        }

        fn visit_none<E: de::Error>(self) -> Result<Option<i64>, E> {
            Ok(None)
        }

        fn visit_unit<E: de::Error>(self) -> Result<Option<i64>, E> {
            Ok(None)
        }

        fn visit_some<D2: de::Deserializer<'de>>(
            self,
            deserializer: D2,
        ) -> Result<Option<i64>, D2::Error> {
            deserialize_option_i64_lenient(deserializer)
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<Option<i64>, E> {
            Ok(Some(v))
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<Option<i64>, E> {
            i64::try_from(v)
                .map(Some)
                .map_err(|_| E::custom(format!("i64 overflow: {v}")))
        }

        fn visit_f64<E: de::Error>(self, v: f64) -> Result<Option<i64>, E> {
            if v.fract() == 0.0 && v >= i64::MIN as f64 && v <= i64::MAX as f64 {
                Ok(Some(v as i64))
            } else {
                Err(E::custom(format!("invalid i64: {v}")))
            }
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Option<i64>, E> {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            trimmed
                .parse::<i64>()
                .map(Some)
                .map_err(|_| E::custom(format!("invalid i64 string: \"{v}\"")))
        }
    }

    deserializer.deserialize_any(OptionI64OrString)
}

/// Deserializes a `Vec<String>` that may arrive as a JSON array or a
/// stringified JSON array.
///
/// Some MCP clients serialize all tool-call values through a `Record<string,
/// string>` intermediate, so an argument like `["a", "b"]` gets delivered as
/// `"[\"a\", \"b\"]"`. This helper accepts both shapes. An empty string is
/// treated as an empty vector; `null`/missing fall through to `#[serde(default)]`.
pub fn deserialize_vec_string_lenient<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: de::Deserializer<'de>,
{
    struct VecStringOrString;

    impl<'de> de::Visitor<'de> for VecStringOrString {
        type Value = Vec<String>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("array of strings or stringified JSON array")
        }

        fn visit_unit<E: de::Error>(self) -> Result<Vec<String>, E> {
            Ok(Vec::new())
        }

        fn visit_none<E: de::Error>(self) -> Result<Vec<String>, E> {
            Ok(Vec::new())
        }

        fn visit_some<D2: de::Deserializer<'de>>(
            self,
            deserializer: D2,
        ) -> Result<Vec<String>, D2::Error> {
            deserialize_vec_string_lenient(deserializer)
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Vec<String>, E> {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                return Ok(Vec::new());
            }
            serde_json::from_str::<Vec<String>>(trimmed)
                .map_err(|err| E::custom(format!("invalid stringified Vec<String>: {err}")))
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Vec<String>, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            let mut out = Vec::new();
            while let Some(item) = seq.next_element::<String>()? {
                out.push(item);
            }
            Ok(out)
        }
    }

    deserializer.deserialize_any(VecStringOrString)
}

/// Deserializes an `Option<Vec<String>>` that may arrive as an array, a
/// stringified array, an empty string, or null.
///
/// Use with `#[serde(default, deserialize_with = "deserialize_option_vec_string_lenient")]`.
pub fn deserialize_option_vec_string_lenient<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<String>>, D::Error>
where
    D: de::Deserializer<'de>,
{
    struct OptionVecStringOrString;

    impl<'de> de::Visitor<'de> for OptionVecStringOrString {
        type Value = Option<Vec<String>>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("array of strings, stringified JSON array, or null")
        }

        fn visit_unit<E: de::Error>(self) -> Result<Option<Vec<String>>, E> {
            Ok(None)
        }

        fn visit_none<E: de::Error>(self) -> Result<Option<Vec<String>>, E> {
            Ok(None)
        }

        fn visit_some<D2: de::Deserializer<'de>>(
            self,
            deserializer: D2,
        ) -> Result<Option<Vec<String>>, D2::Error> {
            deserialize_option_vec_string_lenient(deserializer)
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Option<Vec<String>>, E> {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            serde_json::from_str::<Vec<String>>(trimmed)
                .map(Some)
                .map_err(|err| E::custom(format!("invalid stringified Vec<String>: {err}")))
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Option<Vec<String>>, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            let mut out = Vec::new();
            while let Some(item) = seq.next_element::<String>()? {
                out.push(item);
            }
            Ok(Some(out))
        }
    }

    deserializer.deserialize_any(OptionVecStringOrString)
}

/// Deserializes an `Option<bool>` that may arrive as a JSON boolean, string, or null.
///
/// Use with `#[serde(default, deserialize_with = "deserialize_option_bool_lenient")]`.
pub fn deserialize_option_bool_lenient<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: de::Deserializer<'de>,
{
    struct OptionBoolOrString;

    impl<'de> de::Visitor<'de> for OptionBoolOrString {
        type Value = Option<bool>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("bool, string-encoded bool, or null")
        }

        fn visit_none<E: de::Error>(self) -> Result<Option<bool>, E> {
            Ok(None)
        }

        fn visit_unit<E: de::Error>(self) -> Result<Option<bool>, E> {
            Ok(None)
        }

        fn visit_some<D2: de::Deserializer<'de>>(
            self,
            deserializer: D2,
        ) -> Result<Option<bool>, D2::Error> {
            deserialize_bool_lenient(deserializer).map(Some)
        }

        fn visit_bool<E: de::Error>(self, v: bool) -> Result<Option<bool>, E> {
            Ok(Some(v))
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Option<bool>, E> {
            match v.trim().to_lowercase().as_str() {
                "true" | "1" | "yes" => Ok(Some(true)),
                "false" | "0" | "no" => Ok(Some(false)),
                _ => Err(E::custom(format!("invalid bool string: \"{v}\""))),
            }
        }
    }

    deserializer.deserialize_any(OptionBoolOrString)
}
