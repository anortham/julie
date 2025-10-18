// Vue extractor helper utilities and regex patterns
//
// Contains shared regex patterns and helper functions used across Vue extraction modules

use regex::Regex;
use std::sync::LazyLock;

// Static regex patterns compiled once for performance
pub(super) static TEMPLATE_START_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^<template(\s+[^>]*)?>").unwrap());

pub(super) static SCRIPT_START_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^<script(\s+[^>]*)?>").unwrap());

pub(super) static STYLE_START_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^<style(\s+[^>]*)?>").unwrap());

pub(super) static SECTION_END_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^</(template|script|style)>").unwrap());

pub(super) static LANG_ATTR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"lang=["']?([^"'\s>]+)"#).unwrap());

pub(super) static DATA_FUNCTION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*data\s*\(\s*\)\s*\{").unwrap());

pub(super) static METHODS_OBJECT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*methods\s*:\s*\{").unwrap());

pub(super) static COMPUTED_OBJECT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*computed\s*:\s*\{").unwrap());

pub(super) static PROPS_OBJECT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*props\s*:\s*\{").unwrap());

pub(super) static FUNCTION_DEF_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*([a-zA-Z_$][a-zA-Z0-9_$]*)\s*\([^)]*\)\s*\{").unwrap());

pub(super) static COMPONENT_USAGE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<([A-Z][a-zA-Z0-9-]*)").unwrap());

pub(super) static DIRECTIVE_USAGE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s(v-[a-zA-Z-]+)=").unwrap());

pub(super) static CSS_CLASS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.([a-zA-Z_-][a-zA-Z0-9_-]*)\s*\{").unwrap());
