//! Lenient serde deserializers for MCP tool parameters.
//!
//! Some MCP clients serialize numeric values as strings (e.g. `"30"` instead
//! of `30`). These helpers accept both representations so Julie doesn't reject
//! valid tool calls.

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
            v.trim().parse().map_err(|_| E::custom(format!("invalid u32 string: \"{v}\"")))
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
