// Rust Extractor Tests
//
// Direct Implementation of Rust extractor tests (TDD RED phase)
//

// Submodule declarations
pub mod cross_file_relationships;
pub mod extractor;
pub mod functions;
pub mod helpers;
pub mod identifiers;
pub mod relationships;
pub mod signatures;
pub mod types;
// This isImplementation of most comprehensive extractors with 2000+ lines of tests
// covering everything from basic structs to unsafe FFI code and procedural macros.

use crate::extractors::base::{SymbolKind, Visibility};
use crate::extractors::rust::RustExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

/// Initialize Rust parser
fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("Error loading Rust grammar");
    parser
}

/// Get test workspace root
fn test_workspace_root() -> PathBuf {
    PathBuf::from("/tmp/test")
}

#[cfg(test)]
mod rust_extractor_tests {
    use super::*;

    mod struct_extraction {
        use super::*;

        #[test]
        fn test_extract_basic_struct_definitions() {
            let rust_code = r#"
#[derive(Debug, Clone)]
pub struct User {
    pub id: u64,
    name: String,
    email: Option<String>,
}

struct Point(f64, f64);
"#;

            let mut parser = init_parser();
            let tree = parser.parse(rust_code, None).unwrap();

            let workspace_root = test_workspace_root();
            let mut extractor = RustExtractor::new(
                "rust".to_string(),
                "test.rs".to_string(),
                rust_code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            let user_struct = symbols.iter().find(|s| s.name == "User");
            assert!(user_struct.is_some());
            let user_struct = user_struct.unwrap();
            assert_eq!(user_struct.kind, SymbolKind::Class);
            assert!(
                user_struct
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("pub struct User")
            );
            assert_eq!(
                user_struct.visibility.as_ref().unwrap(),
                &Visibility::Public
            );

            let point_struct = symbols.iter().find(|s| s.name == "Point");
            assert!(point_struct.is_some());
            let point_struct = point_struct.unwrap();
            assert_eq!(point_struct.kind, SymbolKind::Class);
            assert_eq!(
                point_struct.visibility.as_ref().unwrap(),
                &Visibility::Private
            );
        }
    }

    mod enum_extraction {
        use super::*;

        #[test]
        fn test_extract_enum_definitions_with_variants() {
            let rust_code = r#"
#[derive(Debug)]
pub enum Status {
    Active,
    Inactive,
    Pending(String),
    Processing { task_id: u64, progress: f32 },
}

enum Color { Red, Green, Blue }
"#;

            let mut parser = init_parser();
            let tree = parser.parse(rust_code, None).unwrap();

            let workspace_root = test_workspace_root();
            let mut extractor = RustExtractor::new(
                "rust".to_string(),
                "test.rs".to_string(),
                rust_code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            let status_enum = symbols.iter().find(|s| s.name == "Status");
            assert!(status_enum.is_some());
            let status_enum = status_enum.unwrap();
            assert_eq!(status_enum.kind, SymbolKind::Class);
            assert!(
                status_enum
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("pub enum Status")
            );
            assert_eq!(
                status_enum.visibility.as_ref().unwrap(),
                &Visibility::Public
            );

            let color_enum = symbols.iter().find(|s| s.name == "Color");
            assert!(color_enum.is_some());
            let color_enum = color_enum.unwrap();
            assert_eq!(
                color_enum.visibility.as_ref().unwrap(),
                &Visibility::Private
            );
        }
    }

    mod trait_extraction {
        use super::*;

        #[test]
        fn test_extract_trait_definitions() {
            let rust_code = r#"
pub trait Display {
    fn fmt(&self) -> String;
    fn print(&self) {
        println!("{}", self.fmt());
    }
}

trait Clone {
    fn clone(&self) -> Self;
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(rust_code, None).unwrap();

            let workspace_root = test_workspace_root();
            let mut extractor = RustExtractor::new(
                "rust".to_string(),
                "test.rs".to_string(),
                rust_code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            let display_trait = symbols.iter().find(|s| s.name == "Display");
            assert!(display_trait.is_some());
            let display_trait = display_trait.unwrap();
            assert_eq!(display_trait.kind, SymbolKind::Interface);
            assert_eq!(
                display_trait.signature.as_ref().unwrap(),
                "pub trait Display"
            );
            assert_eq!(
                display_trait.visibility.as_ref().unwrap(),
                &Visibility::Public
            );

            let clone_trait = symbols.iter().find(|s| s.name == "Clone");
            assert!(clone_trait.is_some());
            let clone_trait = clone_trait.unwrap();
            assert_eq!(
                clone_trait.visibility.as_ref().unwrap(),
                &Visibility::Private
            );
        }
    }

    mod function_extraction {
        use super::*;

        #[test]
        fn test_extract_standalone_functions_with_various_signatures() {
            let rust_code = r#"
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub async fn fetch_data(url: &str) -> Result<String, Error> {
    // async implementation
    Ok("data".to_string())
}

unsafe fn raw_memory_access() -> *mut u8 {
    std::ptr::null_mut()
}

fn private_helper() {}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(rust_code, None).unwrap();

            let workspace_root = test_workspace_root();
            let mut extractor = RustExtractor::new(
                "rust".to_string(),
                "test.rs".to_string(),
                rust_code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            let add_func = symbols.iter().find(|s| s.name == "add");
            assert!(add_func.is_some());
            let add_func = add_func.unwrap();
            assert_eq!(add_func.kind, SymbolKind::Function);
            assert!(
                add_func
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("pub fn add(a: i32, b: i32)")
            );
            assert_eq!(add_func.visibility.as_ref().unwrap(), &Visibility::Public);

            let fetch_func = symbols.iter().find(|s| s.name == "fetch_data");
            assert!(fetch_func.is_some());
            let fetch_func = fetch_func.unwrap();
            assert!(
                fetch_func
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("pub async fn fetch_data")
            );

            let unsafe_func = symbols.iter().find(|s| s.name == "raw_memory_access");
            assert!(unsafe_func.is_some());
            let unsafe_func = unsafe_func.unwrap();
            assert!(
                unsafe_func
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("unsafe fn raw_memory_access")
            );

            let private_func = symbols.iter().find(|s| s.name == "private_helper");
            assert!(private_func.is_some());
            let private_func = private_func.unwrap();
            assert_eq!(
                private_func.visibility.as_ref().unwrap(),
                &Visibility::Private
            );
        }

        #[test]
        fn test_extract_methods_from_impl_blocks() {
            let rust_code = r#"
struct Calculator {
    value: f64,
}

impl Calculator {
    pub fn new(value: f64) -> Self {
        Self { value }
    }

    fn add(&mut self, other: f64) {
        self.value += other;
    }

    pub fn get_value(&self) -> f64 {
        self.value
    }
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(rust_code, None).unwrap();

            let workspace_root = test_workspace_root();
            let mut extractor = RustExtractor::new(
                "rust".to_string(),
                "test.rs".to_string(),
                rust_code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            let new_method = symbols.iter().find(|s| s.name == "new");
            assert!(new_method.is_some());
            let new_method = new_method.unwrap();
            assert_eq!(new_method.kind, SymbolKind::Method);
            assert!(
                new_method
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("pub fn new(value: f64)")
            );

            let add_method = symbols.iter().find(|s| s.name == "add");
            assert!(add_method.is_some());
            let add_method = add_method.unwrap();
            assert_eq!(add_method.kind, SymbolKind::Method);
            assert!(
                add_method
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("fn add(&mut self, other: f64)")
            );

            let get_value_method = symbols.iter().find(|s| s.name == "get_value");
            assert!(get_value_method.is_some());
            let get_value_method = get_value_method.unwrap();
            assert!(
                get_value_method
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("&self")
            );
        }

        #[test]
        fn test_extract_methods_from_impl_blocks_without_local_struct_definition() {
            let rust_code = r#"
impl SymbolDatabase {
    pub fn store_file_info(&self, path: &str) {
        let _ = path;
    }

    fn drop_file_indexes(&mut self) {}
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(rust_code, None).unwrap();

            let workspace_root = test_workspace_root();
            let mut extractor = RustExtractor::new(
                "rust".to_string(),
                "database/files.rs".to_string(),
                rust_code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            let store_method = symbols.iter().find(|s| s.name == "store_file_info");
            assert!(
                store_method.is_some(),
                "expected to find method extracted from cross-file impl"
            );
            let store_method = store_method.unwrap();
            assert_eq!(store_method.kind, SymbolKind::Method);
            let metadata = store_method
                .metadata
                .as_ref()
                .expect("method metadata should be populated for impl methods");
            assert_eq!(
                metadata
                    .get("impl_type_name")
                    .and_then(|value| value.as_str()),
                Some("SymbolDatabase"),
                "method metadata should preserve impl type name for cross-file resolution"
            );
            assert!(
                store_method.parent_id.is_none(),
                "cross-file impl methods should not assume a parent id from another file"
            );

            let drop_method = symbols.iter().find(|s| s.name == "drop_file_indexes");
            assert!(
                drop_method.is_some(),
                "expected private impl method to be extracted"
            );
            let drop_method = drop_method.unwrap();
            assert_eq!(drop_method.kind, SymbolKind::Method);
            assert!(
                drop_method.visibility.as_ref().unwrap() == &Visibility::Private,
                "non-pub methods in impls should remain private"
            );
        }
    }

    mod module_extraction {
        use super::*;

        #[test]
        fn test_extract_module_definitions() {
            let rust_code = r#"
pub mod utils {
    pub fn helper() {}
}

mod private_module {
    fn internal_function() {}
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(rust_code, None).unwrap();

            let workspace_root = test_workspace_root();
            let mut extractor = RustExtractor::new(
                "rust".to_string(),
                "test.rs".to_string(),
                rust_code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            let utils_module = symbols.iter().find(|s| s.name == "utils");
            assert!(utils_module.is_some());
            let utils_module = utils_module.unwrap();
            assert_eq!(utils_module.kind, SymbolKind::Namespace);
            assert_eq!(utils_module.signature.as_ref().unwrap(), "pub mod utils");
            assert_eq!(
                utils_module.visibility.as_ref().unwrap(),
                &Visibility::Public
            );

            let private_module = symbols.iter().find(|s| s.name == "private_module");
            assert!(private_module.is_some());
            let private_module = private_module.unwrap();
            assert_eq!(
                private_module.visibility.as_ref().unwrap(),
                &Visibility::Private
            );
        }
    }

    mod use_statement_extraction {
        use super::*;

        #[test]
        fn test_extract_use_declarations_and_imports() {
            let rust_code = r#"
use std::collections::HashMap;
use std::fmt::{Debug, Display};
use super::utils as util;
use crate::model::User;
"#;

            let mut parser = init_parser();
            let tree = parser.parse(rust_code, None).unwrap();

            let workspace_root = test_workspace_root();
            let mut extractor = RustExtractor::new(
                "rust".to_string(),
                "test.rs".to_string(),
                rust_code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            let hashmap_import = symbols.iter().find(|s| s.name == "HashMap");
            assert!(hashmap_import.is_some());
            let hashmap_import = hashmap_import.unwrap();
            assert_eq!(hashmap_import.kind, SymbolKind::Import);
            assert!(
                hashmap_import
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("use std::collections::HashMap")
            );

            let alias_import = symbols.iter().find(|s| s.name == "util");
            assert!(alias_import.is_some());
            let alias_import = alias_import.unwrap();
            assert!(
                alias_import
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("use super::utils as util")
            );

            let user_import = symbols.iter().find(|s| s.name == "User");
            assert!(user_import.is_some());
            let user_import = user_import.unwrap();
            assert!(
                user_import
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("use crate::model::User")
            );
        }
    }

    mod constants_and_statics {
        use super::*;

        #[test]
        fn test_extract_const_and_static_declarations() {
            let rust_code = r#"
const MAX_SIZE: usize = 1024;
pub const VERSION: &str = "1.0.0";

static mut COUNTER: i32 = 0;
static GLOBAL_CONFIG: Config = Config::new();
"#;

            let mut parser = init_parser();
            let tree = parser.parse(rust_code, None).unwrap();

            let workspace_root = test_workspace_root();
            let mut extractor = RustExtractor::new(
                "rust".to_string(),
                "test.rs".to_string(),
                rust_code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            let max_size_const = symbols.iter().find(|s| s.name == "MAX_SIZE");
            assert!(max_size_const.is_some());
            let max_size_const = max_size_const.unwrap();
            assert_eq!(max_size_const.kind, SymbolKind::Constant);
            assert!(
                max_size_const
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("const MAX_SIZE: usize = 1024")
            );

            let version_const = symbols.iter().find(|s| s.name == "VERSION");
            assert!(version_const.is_some());
            let version_const = version_const.unwrap();
            assert_eq!(
                version_const.visibility.as_ref().unwrap(),
                &Visibility::Public
            );

            let counter_static = symbols.iter().find(|s| s.name == "COUNTER");
            assert!(counter_static.is_some());
            let counter_static = counter_static.unwrap();
            assert_eq!(counter_static.kind, SymbolKind::Variable);
            assert!(
                counter_static
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("static mut COUNTER: i32 = 0")
            );

            let config_static = symbols.iter().find(|s| s.name == "GLOBAL_CONFIG");
            assert!(config_static.is_some());
            let config_static = config_static.unwrap();
            assert!(
                config_static
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("static GLOBAL_CONFIG")
            );
        }
    }

    mod macro_extraction {
        use super::*;

        #[test]
        fn test_extract_macro_definitions() {
            let rust_code = r#"
macro_rules! vec_of_strings {
    ($($x:expr),*) => {
        vec![$($x.to_string()),*]
    };
}

macro_rules! create_function {
    ($func_name:ident) => {
        fn $func_name() {
            println!("Function {} called", stringify!($func_name));
        }
    };
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(rust_code, None).unwrap();

            let workspace_root = test_workspace_root();
            let mut extractor = RustExtractor::new(
                "rust".to_string(),
                "test.rs".to_string(),
                rust_code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            let vec_macro = symbols.iter().find(|s| s.name == "vec_of_strings");
            assert!(vec_macro.is_some());
            let vec_macro = vec_macro.unwrap();
            assert_eq!(vec_macro.kind, SymbolKind::Function);
            assert_eq!(
                vec_macro.signature.as_ref().unwrap(),
                "macro_rules! vec_of_strings"
            );

            let create_func_macro = symbols.iter().find(|s| s.name == "create_function");
            assert!(create_func_macro.is_some());
            let create_func_macro = create_func_macro.unwrap();
            assert_eq!(
                create_func_macro.signature.as_ref().unwrap(),
                "macro_rules! create_function"
            );
        }
    }

    mod advanced_generics_and_type_parameters {
        use super::*;

        #[test]
        fn test_extract_generic_structs_traits_and_functions_with_constraints() {
            let rust_code = r#"
use std::fmt::{Debug, Display};
use std::cmp::Ord;

/// Generic container with lifetime parameter
pub struct Container<'a, T: Debug + Clone> {
    pub data: &'a T,
    pub metadata: Option<String>,
}

/// Generic trait with associated type
pub trait Iterator<T> {
    type Item;
    type Error;

    fn next(&mut self) -> Option<Self::Item>;
    fn collect<C>(self) -> Result<C, Self::Error>
    where
        Self: Sized,
        C: FromIterator<Self::Item>;
}

/// Generic function with multiple constraints
pub fn sort_and_display<T>(mut items: Vec<T>) -> String
where
    T: Ord + Display + Clone,
{
    items.sort();
    items.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ")
}

/// Higher-ranked trait bounds
pub fn closure_example<F>(f: F) -> i32
where
    F: for<'a> Fn(&'a str) -> i32,
{
    f("test")
}

/// Associated type with bounds
pub trait Collect {
    type Output: Debug;

    fn collect(&self) -> Self::Output;
}

/// Generic enum with phantom data
use std::marker::PhantomData;

pub enum Either<L, R> {
    Left(L),
    Right(R),
    Neither(PhantomData<(L, R)>),
}

/// Type alias for complex generic type
type UserMap<K> = std::collections::HashMap<K, User>
where
    K: std::hash::Hash + Eq;
"#;

            let mut parser = init_parser();
            let tree = parser.parse(rust_code, None).unwrap();

            let workspace_root = test_workspace_root();
            let mut extractor = RustExtractor::new(
                "rust".to_string(),
                "test.rs".to_string(),
                rust_code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            let container = symbols.iter().find(|s| s.name == "Container");
            assert!(container.is_some());
            let container = container.unwrap();
            assert_eq!(container.kind, SymbolKind::Class);
            assert!(
                container
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("pub struct Container")
            );
            assert!(
                container
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("<'a, T: Debug + Clone>")
            );

            let iterator_trait = symbols.iter().find(|s| s.name == "Iterator");
            assert!(iterator_trait.is_some());
            let iterator_trait = iterator_trait.unwrap();
            assert_eq!(iterator_trait.kind, SymbolKind::Interface);
            assert!(
                iterator_trait
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("pub trait Iterator<T>")
            );

            let sort_func = symbols.iter().find(|s| s.name == "sort_and_display");
            assert!(sort_func.is_some());
            let sort_func = sort_func.unwrap();
            assert!(
                sort_func
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("pub fn sort_and_display<T>")
            );
            assert!(
                sort_func
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("T: Ord + Display + Clone")
            );

            let closure_func = symbols.iter().find(|s| s.name == "closure_example");
            assert!(closure_func.is_some());
            let closure_func = closure_func.unwrap();
            assert!(
                closure_func
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("for<'a> Fn(&'a str) -> i32")
            );

            let collect_trait = symbols.iter().find(|s| s.name == "Collect");
            assert!(collect_trait.is_some());
            let collect_trait = collect_trait.unwrap();
            assert!(
                collect_trait
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("type Output: Debug")
            );

            let either_enum = symbols.iter().find(|s| s.name == "Either");
            assert!(either_enum.is_some());
            let either_enum = either_enum.unwrap();
            assert!(
                either_enum
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("pub enum Either<L, R>")
            );

            let user_map_type = symbols.iter().find(|s| s.name == "UserMap");
            assert!(user_map_type.is_some());
            let user_map_type = user_map_type.unwrap();
            assert_eq!(user_map_type.kind, SymbolKind::Type);
            assert!(
                user_map_type
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("type UserMap<K>")
            );
        }
    }

    mod unsafe_code_and_ffi {
        use super::*;

        #[test]
        fn test_extract_unsafe_blocks_raw_pointers_and_ffi_functions() {
            let rust_code = r#"
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::ptr;
use std::slice;

/// External C functions
extern "C" {
    fn malloc(size: usize) -> *mut c_void;
    fn free(ptr: *mut c_void);
    fn strlen(s: *const c_char) -> usize;
    fn printf(format: *const c_char, ...) -> c_int;
}

/// Unsafe struct with raw pointers
#[repr(C)]
pub struct RawBuffer {
    data: *mut u8,
    len: usize,
    capacity: usize,
}

impl RawBuffer {
    /// Unsafe constructor
    pub unsafe fn new(capacity: usize) -> Self {
        let data = malloc(capacity) as *mut u8;
        if data.is_null() {
            panic!("Failed to allocate memory");
        }

        Self {
            data,
            len: 0,
            capacity,
        }
    }

    /// Safe wrapper around unsafe operations
    pub fn push(&mut self, byte: u8) -> Result<(), &'static str> {
        if self.len >= self.capacity {
            return Err("Buffer overflow");
        }

        unsafe {
            *self.data.add(self.len) = byte;
        }
        self.len += 1;
        Ok(())
    }

    /// Unsafe access to raw data
    pub unsafe fn as_slice(&self) -> &[u8] {
        slice::from_raw_parts(self.data, self.len)
    }
}

/// Union type for type punning
#[repr(C)]
union FloatBytes {
    f: f32,
    bytes: [u8; 4],
}

pub fn float_to_bytes(f: f32) -> [u8; 4] {
    unsafe {
        FloatBytes { f }.bytes
    }
}

/// Export functions for C
#[no_mangle]
pub extern "C" fn create_point(x: f64, y: f64, z: f64) -> Point3D {
    Point3D { x, y, z }
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(rust_code, None).unwrap();

            let workspace_root = test_workspace_root();
            let mut extractor = RustExtractor::new(
                "rust".to_string(),
                "test.rs".to_string(),
                rust_code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            // Check extern block
            let malloc_extern = symbols.iter().find(|s| s.name == "malloc");
            assert!(malloc_extern.is_some());
            let malloc_extern = malloc_extern.unwrap();
            assert!(
                malloc_extern
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("fn malloc(size: usize)")
            );

            let raw_buffer = symbols.iter().find(|s| s.name == "RawBuffer");
            assert!(raw_buffer.is_some());
            let raw_buffer = raw_buffer.unwrap();
            assert_eq!(raw_buffer.kind, SymbolKind::Class);
            assert!(
                raw_buffer
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("pub struct RawBuffer")
            );

            let unsafe_new = symbols
                .iter()
                .find(|s| s.name == "new" && s.parent_id == Some(raw_buffer.id.clone()));
            assert!(unsafe_new.is_some());
            let unsafe_new = unsafe_new.unwrap();
            assert!(
                unsafe_new
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("pub unsafe fn new")
            );

            let as_slice = symbols.iter().find(|s| s.name == "as_slice");
            assert!(as_slice.is_some());
            let as_slice = as_slice.unwrap();
            assert!(
                as_slice
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("pub unsafe fn as_slice")
            );

            let float_bytes = symbols.iter().find(|s| s.name == "FloatBytes");
            assert!(float_bytes.is_some());
            let float_bytes = float_bytes.unwrap();
            assert_eq!(float_bytes.kind, SymbolKind::Union);
            assert!(
                float_bytes
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("union FloatBytes")
            );

            let float_to_bytes = symbols.iter().find(|s| s.name == "float_to_bytes");
            assert!(float_to_bytes.is_some());
            let float_to_bytes = float_to_bytes.unwrap();
            assert!(
                float_to_bytes
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("pub fn float_to_bytes")
            );

            let create_point = symbols.iter().find(|s| s.name == "create_point");
            assert!(create_point.is_some());
            let create_point = create_point.unwrap();
            assert!(
                create_point
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("pub extern \"C\" fn create_point")
            );
        }
    }

    mod comprehensive_rust_features {
        use super::*;

        #[test]
        fn test_handle_comprehensive_rust_code() {
            let rust_code = r#"
use std::collections::HashMap;
use std::fmt::{Debug, Display};

/// A user management system
#[derive(Debug, Clone)]
pub struct User {
    pub id: u64,
    name: String,
    email: Option<String>,
}

pub trait UserOperations {
    fn get_id(&self) -> u64;
    fn update_email(&mut self, email: String) -> Result<(), String>;
}

impl UserOperations for User {
    fn get_id(&self) -> u64 {
        self.id
    }

    fn update_email(&mut self, email: String) -> Result<(), String> {
        if email.contains('@') {
            self.email = Some(email);
            Ok(())
        } else {
            Err("Invalid email".to_string())
        }
    }
}

impl User {
    pub fn new(id: u64, name: String) -> Self {
        Self {
            id,
            name,
            email: None,
        }
    }

    pub async fn fetch_from_db(id: u64) -> Option<User> {
        // Async function implementation
        None
    }
}

#[derive(Debug)]
pub enum Status {
    Active,
    Inactive,
    Pending(String),
}

pub mod utils {
    pub fn validate_email(email: &str) -> bool {
        email.contains('@')
    }
}

macro_rules! create_user {
    ($id:expr, $name:expr) => {
        User::new($id, $name.to_string())
    };
}

const MAX_USERS: usize = 1000;
static mut COUNTER: i32 = 0;
"#;

            let mut parser = init_parser();
            let tree = parser.parse(rust_code, None).unwrap();

            let workspace_root = test_workspace_root();
            let mut extractor = RustExtractor::new(
                "rust".to_string(),
                "test.rs".to_string(),
                rust_code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            // Check we extracted all major symbols
            assert!(symbols.iter().find(|s| s.name == "User").is_some());
            assert!(
                symbols
                    .iter()
                    .find(|s| s.name == "UserOperations")
                    .is_some()
            );
            assert!(symbols.iter().find(|s| s.name == "Status").is_some());
            assert!(symbols.iter().find(|s| s.name == "utils").is_some());
            assert!(symbols.iter().find(|s| s.name == "create_user").is_some());
            assert!(symbols.iter().find(|s| s.name == "MAX_USERS").is_some());
            assert!(symbols.iter().find(|s| s.name == "COUNTER").is_some());

            // Check specific features
            let user_struct = symbols.iter().find(|s| s.name == "User").unwrap();
            assert!(
                user_struct
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("pub struct User")
            );

            let fetch_method = symbols.iter().find(|s| s.name == "fetch_from_db").unwrap();
            assert!(
                fetch_method
                    .signature
                    .as_ref()
                    .unwrap()
                    .contains("pub async fn fetch_from_db")
            );

            let macro_symbol = symbols.iter().find(|s| s.name == "create_user").unwrap();
            assert_eq!(macro_symbol.kind, SymbolKind::Function);

            println!("🦀 Extracted {} Rust symbols successfully", symbols.len());
        }
    }

    // TODO: Continue with remaining test suites:
    // - Advanced Async and Concurrency tests
    // - Error Handling and Result Types tests
    // - Pattern Matching and Control Flow tests
    // - Procedural Macros and Attributes tests
    // - Performance and Edge Cases tests
    // - Type Inference tests
    // - Relationship Extraction tests

    // ========================================================================
    // Identifier Extraction Tests (TDD - matches C# reference pattern)
    // ========================================================================
    //
    // These tests validate the extract_identifiers() functionality:
    // - Function calls (call_expression) → IdentifierKind::Call
    // - Field access (field_expression) → IdentifierKind::MemberAccess
    // - Proper containing symbol tracking (file-scoped)
    //
    // Following the proven pattern from C#, Python, etc.

    mod identifier_extraction_tests {
        use super::*;
        use crate::extractors::base::IdentifierKind;

        #[test]
        fn test_rust_function_calls() {
            let rust_code = r#"
struct Calculator;

impl Calculator {
    fn add(&self, a: i32, b: i32) -> i32 {
        a + b
    }

    fn calculate(&self) -> i32 {
        let result = self.add(5, 3);      // Method call to add
        println!("{}", result);            // Function call to println macro
        result
    }
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(rust_code, None).unwrap();

            let workspace_root = test_workspace_root();
            let mut extractor = RustExtractor::new(
                "rust".to_string(),
                "test.rs".to_string(),
                rust_code.to_string(),
                &workspace_root,
            );

            // Extract symbols first
            let symbols = extractor.extract_symbols(&tree);

            // NOW extract identifiers
            let identifiers = extractor.extract_identifiers(&tree, &symbols);

            // Verify we found the function call to 'add'
            let add_call = identifiers.iter().find(|id| id.name == "add");
            assert!(
                add_call.is_some(),
                "Should extract 'add' method call identifier"
            );
            let add_call = add_call.unwrap();
            assert_eq!(add_call.kind, IdentifierKind::Call);

            // Verify containing symbol is set correctly (should be inside calculate method)
            assert!(
                add_call.containing_symbol_id.is_some(),
                "Method call should have containing symbol"
            );

            // Find the calculate method symbol
            let calculate_method = symbols.iter().find(|s| s.name == "calculate").unwrap();

            // Verify the add call is contained within calculate method
            assert_eq!(
                add_call.containing_symbol_id.as_ref(),
                Some(&calculate_method.id),
                "add call should be contained within calculate method"
            );
        }

        #[test]
        fn test_rust_member_access() {
            let rust_code = r#"
struct Point {
    x: f64,
    y: f64,
}

fn get_coordinates(point: &Point) -> (f64, f64) {
    let x_val = point.x;     // Field access: point.x
    let y_val = point.y;     // Field access: point.y
    (x_val, y_val)
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(rust_code, None).unwrap();

            let workspace_root = test_workspace_root();
            let mut extractor = RustExtractor::new(
                "rust".to_string(),
                "test.rs".to_string(),
                rust_code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);
            let identifiers = extractor.extract_identifiers(&tree, &symbols);

            // Verify we found member access identifiers
            let x_access = identifiers
                .iter()
                .filter(|id| id.name == "x" && id.kind == IdentifierKind::MemberAccess)
                .count();
            assert!(x_access > 0, "Should extract 'x' field access identifier");

            let y_access = identifiers
                .iter()
                .filter(|id| id.name == "y" && id.kind == IdentifierKind::MemberAccess)
                .count();
            assert!(y_access > 0, "Should extract 'y' field access identifier");
        }

        #[test]
        fn test_rust_identifiers_have_containing_symbol() {
            // This test ensures we ONLY match symbols from the SAME FILE
            // Critical bug fix from reference implementation
            let rust_code = r#"
fn helper() -> i32 {
    42
}

fn process() -> i32 {
    let result = helper();    // Function call to helper
    result
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(rust_code, None).unwrap();

            let workspace_root = test_workspace_root();
            let mut extractor = RustExtractor::new(
                "rust".to_string(),
                "test.rs".to_string(),
                rust_code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);
            let identifiers = extractor.extract_identifiers(&tree, &symbols);

            // Find the helper call
            let helper_call = identifiers
                .iter()
                .find(|id| id.name == "helper" && id.kind == IdentifierKind::Call)
                .expect("Should find helper function call");

            // Verify it has a containing symbol
            assert!(
                helper_call.containing_symbol_id.is_some(),
                "Helper call should have containing symbol"
            );

            // Verify it's contained in the process function (not some other file's symbol)
            let process_fn = symbols.iter().find(|s| s.name == "process").unwrap();
            assert_eq!(
                helper_call.containing_symbol_id.as_ref(),
                Some(&process_fn.id),
                "helper call should be contained within process function"
            );
        }

        #[test]
        fn test_rust_chained_member_access() {
            let rust_code = r#"
struct Account {
    balance: i32,
}

struct User {
    account: Account,
}

fn get_balance(user: &User) -> i32 {
    user.account.balance    // Chained field access
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(rust_code, None).unwrap();

            let workspace_root = test_workspace_root();
            let mut extractor = RustExtractor::new(
                "rust".to_string(),
                "test.rs".to_string(),
                rust_code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);
            let identifiers = extractor.extract_identifiers(&tree, &symbols);

            // Should extract both 'account' and 'balance' from the chain
            let account_access = identifiers
                .iter()
                .find(|id| id.name == "account" && id.kind == IdentifierKind::MemberAccess);
            assert!(
                account_access.is_some(),
                "Should extract 'account' from chained member access"
            );

            let balance_access = identifiers
                .iter()
                .find(|id| id.name == "balance" && id.kind == IdentifierKind::MemberAccess);
            assert!(
                balance_access.is_some(),
                "Should extract 'balance' from chained member access"
            );
        }

        #[test]
        fn test_rust_duplicate_calls_at_different_locations() {
            let rust_code = r#"
fn process() -> i32 {
    42
}

fn run() {
    process();
    let x = 10;
    process();
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(rust_code, None).unwrap();

            let workspace_root = test_workspace_root();
            let mut extractor = RustExtractor::new(
                "rust".to_string(),
                "test.rs".to_string(),
                rust_code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);
            let identifiers = extractor.extract_identifiers(&tree, &symbols);

            // Should find TWO separate identifiers for the two process() calls
            let process_calls: Vec<_> = identifiers
                .iter()
                .filter(|id| id.name == "process" && id.kind == IdentifierKind::Call)
                .collect();

            assert_eq!(
                process_calls.len(),
                2,
                "Should extract both process() calls as separate identifiers"
            );

            // Verify they have different line numbers
            assert_ne!(
                process_calls[0].start_line, process_calls[1].start_line,
                "Duplicate calls should be on different lines"
            );
        }
    }

    // ========================================================================
    // Doc Comment Extraction Tests (TDD - matches C# reference pattern)
    // ========================================================================
    //
    // These tests validate doc comment extraction from Rust code:
    // - Single-line doc comments (///)
    // - Multi-line doc comments (/** */)
    // - Crate-level doc comments (//!)
    // - Module-level doc comments (//!)
    // - Doc comments on functions, structs, enums, traits, etc.
    //
    // Following the proven pattern from C#, Python, etc.

    mod doc_comment_extraction {
        use super::*;

        #[test]
        fn test_function_with_single_line_doc_comment() {
            let rust_code = r#"
/// Creates a new database connection with the specified configuration
pub fn create_connection(config: &str) -> Result<Connection, Error> {
    Ok(Connection::new(config))
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(rust_code, None).unwrap();

            let workspace_root = test_workspace_root();
            let mut extractor = RustExtractor::new(
                "rust".to_string(),
                "test.rs".to_string(),
                rust_code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            // Find the create_connection function
            let func = symbols
                .iter()
                .find(|s| s.name == "create_connection")
                .expect("Should extract create_connection function");

            // Verify doc comment is captured
            assert!(
                func.doc_comment.is_some(),
                "Function should have doc_comment extracted"
            );
            assert!(
                func.doc_comment
                    .as_ref()
                    .unwrap()
                    .contains("Creates a new database connection")
            );
        }

        #[test]
        fn test_struct_with_multi_line_doc_comment() {
            let rust_code = r#"
/**
 * A user configuration structure
 * Stores all user-related settings and preferences
 */
pub struct UserConfig {
    pub name: String,
    pub email: String,
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(rust_code, None).unwrap();

            let workspace_root = test_workspace_root();
            let mut extractor = RustExtractor::new(
                "rust".to_string(),
                "test.rs".to_string(),
                rust_code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            // Find the UserConfig struct
            let user_config = symbols
                .iter()
                .find(|s| s.name == "UserConfig")
                .expect("Should extract UserConfig struct");

            // Verify multi-line doc comment is captured
            assert!(
                user_config.doc_comment.is_some(),
                "Struct should have doc_comment extracted"
            );
            let doc = user_config.doc_comment.as_ref().unwrap();
            assert!(doc.contains("user configuration"));
            assert!(doc.contains("user-related settings"));
        }

        #[test]
        fn test_enum_with_doc_comment() {
            let rust_code = r#"
/// Represents the status of an operation
#[derive(Debug, Clone, Copy)]
pub enum Status {
    /// Operation is in progress
    Running,
    /// Operation completed successfully
    Success,
    /// Operation failed
    Failed,
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(rust_code, None).unwrap();

            let workspace_root = test_workspace_root();
            let mut extractor = RustExtractor::new(
                "rust".to_string(),
                "test.rs".to_string(),
                rust_code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            // Find the Status enum
            let status_enum = symbols
                .iter()
                .find(|s| s.name == "Status")
                .expect("Should extract Status enum");

            // Verify enum doc comment is captured
            assert!(
                status_enum.doc_comment.is_some(),
                "Enum should have doc_comment extracted"
            );
            assert!(
                status_enum
                    .doc_comment
                    .as_ref()
                    .unwrap()
                    .contains("status of an operation")
            );
        }

        #[test]
        fn test_trait_with_doc_comment() {
            let rust_code = r#"
/// A trait for serializing objects
/// Implementations should handle all types of serialization
pub trait Serializer {
    /// Serialize the object to a string
    fn serialize(&self) -> String;
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(rust_code, None).unwrap();

            let workspace_root = test_workspace_root();
            let mut extractor = RustExtractor::new(
                "rust".to_string(),
                "test.rs".to_string(),
                rust_code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            // Find the Serializer trait
            let trait_symbol = symbols
                .iter()
                .find(|s| s.name == "Serializer")
                .expect("Should extract Serializer trait");

            // Verify trait doc comment is captured
            assert!(
                trait_symbol.doc_comment.is_some(),
                "Trait should have doc_comment extracted"
            );
            let doc = trait_symbol.doc_comment.as_ref().unwrap();
            // The trait has multi-line doc comments, check both lines are present
            assert!(
                doc.contains("A trait for") && doc.contains("Implementations should"),
                "Trait doc should contain both lines, got: {}",
                doc
            );
        }

        #[test]
        fn test_const_with_doc_comment() {
            let rust_code = r#"
/// The maximum number of concurrent connections
pub const MAX_CONNECTIONS: usize = 1024;
"#;

            let mut parser = init_parser();
            let tree = parser.parse(rust_code, None).unwrap();

            let workspace_root = test_workspace_root();
            let mut extractor = RustExtractor::new(
                "rust".to_string(),
                "test.rs".to_string(),
                rust_code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            // Find the MAX_CONNECTIONS const
            let const_symbol = symbols
                .iter()
                .find(|s| s.name == "MAX_CONNECTIONS")
                .expect("Should extract MAX_CONNECTIONS const");

            // Verify const doc comment is captured
            assert!(
                const_symbol.doc_comment.is_some(),
                "Const should have doc_comment extracted"
            );
            assert!(
                const_symbol
                    .doc_comment
                    .as_ref()
                    .unwrap()
                    .contains("maximum number of concurrent connections")
            );
        }

        #[test]
        fn test_symbol_without_doc_comment() {
            let rust_code = r#"
pub fn no_doc_function() {
    println!("This function has no documentation");
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(rust_code, None).unwrap();

            let workspace_root = test_workspace_root();
            let mut extractor = RustExtractor::new(
                "rust".to_string(),
                "test.rs".to_string(),
                rust_code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            // Find the no_doc_function
            let func = symbols
                .iter()
                .find(|s| s.name == "no_doc_function")
                .expect("Should extract no_doc_function");

            // Verify no doc comment is None
            assert!(
                func.doc_comment.is_none(),
                "Function without doc comment should have None"
            );
        }

        #[test]
        fn test_associated_type_with_doc_comment() {
            let rust_code = r#"
pub trait Container {
    /// The type of items stored in the container
    type Item;

    fn get(&self) -> Self::Item;
}
"#;

            let mut parser = init_parser();
            let tree = parser.parse(rust_code, None).unwrap();

            let workspace_root = test_workspace_root();
            let mut extractor = RustExtractor::new(
                "rust".to_string(),
                "test.rs".to_string(),
                rust_code.to_string(),
                &workspace_root,
            );

            let symbols = extractor.extract_symbols(&tree);

            // Find the Item associated type
            let item_type = symbols
                .iter()
                .find(|s| s.name == "Item")
                .expect("Should extract Item associated type");

            // Verify associated type doc comment is captured
            assert!(
                item_type.doc_comment.is_some(),
                "Associated type should have doc_comment extracted"
            );
            assert!(
                item_type
                    .doc_comment
                    .as_ref()
                    .unwrap()
                    .contains("type of items stored")
            );
        }
    }
}
