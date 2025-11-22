// Python signatures inline tests extracted from extractors/python/signatures.rs

use crate::base::Visibility;
use crate::python::signatures;

#[test]
fn test_infer_visibility_dunder() {
    let vis = signatures::infer_visibility("__init__");
    assert_eq!(vis, Visibility::Public);
}

#[test]
fn test_infer_visibility_private() {
    let vis = signatures::infer_visibility("_private_method");
    assert_eq!(vis, Visibility::Private);
}

#[test]
fn test_infer_visibility_public() {
    let vis = signatures::infer_visibility("public_method");
    assert_eq!(vis, Visibility::Public);
}
