//! Tests for C struct/union field extraction as SymbolKind::Field children

use super::{extract_symbols, SymbolKind};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_struct_field_extraction_basic() {
        let code = r#"
struct Point {
    double x;
    double y;
    const char *label;
};
"#;
        let symbols = extract_symbols(code);

        let struct_sym = symbols
            .iter()
            .find(|s| s.name == "Point" && s.kind == SymbolKind::Struct)
            .expect("Should extract Point struct");
        let struct_id = struct_sym.id.clone();

        let fields: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Field && s.parent_id.as_deref() == Some(&struct_id))
            .collect();

        assert_eq!(fields.len(), 3, "Point should have 3 fields, got: {:?}",
            fields.iter().map(|f| &f.name).collect::<Vec<_>>());

        // Verify field names exist
        assert!(fields.iter().any(|f| f.name == "x"), "Should have field 'x'");
        assert!(fields.iter().any(|f| f.name == "y"), "Should have field 'y'");
        assert!(fields.iter().any(|f| f.name == "label"), "Should have field 'label'");

        // Verify type info in signatures
        let x_field = fields.iter().find(|f| f.name == "x").unwrap();
        assert!(
            x_field.signature.as_ref().unwrap().contains("double"),
            "x field signature should contain 'double', got: {}",
            x_field.signature.as_ref().unwrap()
        );

        let label_field = fields.iter().find(|f| f.name == "label").unwrap();
        let label_sig = label_field.signature.as_ref().unwrap();
        assert!(
            label_sig.contains("char"),
            "label field signature should contain 'char', got: {}",
            label_sig
        );
    }

    #[test]
    fn test_union_field_extraction() {
        let code = r#"
union DataValue {
    int i_val;
    float f_val;
    char *s_val;
    double d_val;
};
"#;
        let symbols = extract_symbols(code);

        let union_sym = symbols
            .iter()
            .find(|s| s.name == "DataValue" && s.kind == SymbolKind::Union)
            .expect("Should extract DataValue union");
        let union_id = union_sym.id.clone();

        let fields: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Field && s.parent_id.as_deref() == Some(&union_id))
            .collect();

        assert_eq!(fields.len(), 4, "DataValue should have 4 fields, got: {:?}",
            fields.iter().map(|f| &f.name).collect::<Vec<_>>());

        assert!(fields.iter().any(|f| f.name == "i_val"), "Should have field 'i_val'");
        assert!(fields.iter().any(|f| f.name == "f_val"), "Should have field 'f_val'");
        assert!(fields.iter().any(|f| f.name == "s_val"), "Should have field 's_val'");
        assert!(fields.iter().any(|f| f.name == "d_val"), "Should have field 'd_val'");

        // Verify type in signature
        let i_field = fields.iter().find(|f| f.name == "i_val").unwrap();
        assert!(
            i_field.signature.as_ref().unwrap().contains("int"),
            "i_val field signature should contain 'int', got: {}",
            i_field.signature.as_ref().unwrap()
        );
    }

    #[test]
    fn test_struct_multi_field_declaration() {
        // In C, you can declare multiple fields with the same type: `int x, y;`
        let code = r#"
struct Vector {
    int x, y, z;
    float magnitude;
};
"#;
        let symbols = extract_symbols(code);

        let struct_sym = symbols
            .iter()
            .find(|s| s.name == "Vector" && s.kind == SymbolKind::Struct)
            .expect("Should extract Vector struct");
        let struct_id = struct_sym.id.clone();

        let fields: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Field && s.parent_id.as_deref() == Some(&struct_id))
            .collect();

        assert_eq!(fields.len(), 4, "Vector should have 4 fields (x, y, z, magnitude), got: {:?}",
            fields.iter().map(|f| &f.name).collect::<Vec<_>>());

        assert!(fields.iter().any(|f| f.name == "x"), "Should have field 'x'");
        assert!(fields.iter().any(|f| f.name == "y"), "Should have field 'y'");
        assert!(fields.iter().any(|f| f.name == "z"), "Should have field 'z'");
        assert!(fields.iter().any(|f| f.name == "magnitude"), "Should have field 'magnitude'");
    }

    #[test]
    fn test_struct_pointer_and_array_fields() {
        let code = r#"
struct Buffer {
    char *data;
    size_t length;
    int flags[8];
    void (*callback)(int);
};
"#;
        let symbols = extract_symbols(code);

        let struct_sym = symbols
            .iter()
            .find(|s| s.name == "Buffer" && s.kind == SymbolKind::Struct)
            .expect("Should extract Buffer struct");
        let struct_id = struct_sym.id.clone();

        let fields: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Field && s.parent_id.as_deref() == Some(&struct_id))
            .collect();

        // data, length, flags should be extracted; function pointer callback may or may not
        // depending on how extract_variable_name handles function declarators
        assert!(fields.len() >= 3, "Buffer should have at least 3 fields, got: {:?}",
            fields.iter().map(|f| &f.name).collect::<Vec<_>>());

        assert!(fields.iter().any(|f| f.name == "data"), "Should have field 'data'");
        assert!(fields.iter().any(|f| f.name == "length"), "Should have field 'length'");
        assert!(fields.iter().any(|f| f.name == "flags"), "Should have field 'flags'");
    }

    #[test]
    fn test_typedef_struct_fields() {
        // Fields should be extracted even for typedef'd structs
        let code = r#"
typedef struct {
    int width;
    int height;
} Dimensions;
"#;
        let symbols = extract_symbols(code);

        // The struct might be extracted via the type_definition path.
        // Look for any struct that has these fields
        let dim_fields: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Field)
            .collect();

        // We should have at least width and height as fields
        assert!(
            dim_fields.iter().any(|f| f.name == "width"),
            "Should have field 'width' from typedef struct. All symbols: {:?}",
            symbols.iter().map(|s| (&s.name, &s.kind)).collect::<Vec<_>>()
        );
        assert!(
            dim_fields.iter().any(|f| f.name == "height"),
            "Should have field 'height' from typedef struct"
        );
    }

    #[test]
    fn test_named_typedef_struct_no_duplicate_fields() {
        // When a typedef wraps a named struct (e.g. `typedef struct Point { ... } Point;`),
        // fields should be extracted exactly once — from the type_definition handler only,
        // not again when visit_node recurses into the inner struct_specifier.
        let code = r#"
typedef struct Point {
    double x;
    double y;
} Point;
"#;
        let symbols = extract_symbols(code);

        let fields: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Field)
            .collect();

        assert_eq!(
            fields.len(),
            2,
            "Fields should not be duplicated. Got: {:?}",
            fields
                .iter()
                .map(|f| (&f.name, &f.parent_id))
                .collect::<Vec<_>>()
        );

        assert!(fields.iter().any(|f| f.name == "x"), "Should have field 'x'");
        assert!(fields.iter().any(|f| f.name == "y"), "Should have field 'y'");
    }

    #[test]
    fn test_named_typedef_union_no_duplicate_fields() {
        // Same guard applies to unions inside typedefs
        let code = r#"
typedef union Value {
    int i;
    float f;
    char c;
} Value;
"#;
        let symbols = extract_symbols(code);

        let fields: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Field)
            .collect();

        assert_eq!(
            fields.len(),
            3,
            "Union fields should not be duplicated. Got: {:?}",
            fields
                .iter()
                .map(|f| (&f.name, &f.parent_id))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_field_parent_id_correctness() {
        // Ensure fields point to the correct parent when multiple structs exist
        let code = r#"
struct A {
    int a1;
    int a2;
};

struct B {
    float b1;
    float b2;
    float b3;
};
"#;
        let symbols = extract_symbols(code);

        let struct_a = symbols
            .iter()
            .find(|s| s.name == "A" && s.kind == SymbolKind::Struct)
            .expect("Should extract struct A");
        let struct_b = symbols
            .iter()
            .find(|s| s.name == "B" && s.kind == SymbolKind::Struct)
            .expect("Should extract struct B");

        let a_fields: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Field && s.parent_id.as_deref() == Some(&struct_a.id))
            .collect();
        let b_fields: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Field && s.parent_id.as_deref() == Some(&struct_b.id))
            .collect();

        assert_eq!(a_fields.len(), 2, "Struct A should have 2 fields");
        assert_eq!(b_fields.len(), 3, "Struct B should have 3 fields");

        assert!(a_fields.iter().any(|f| f.name == "a1"));
        assert!(a_fields.iter().any(|f| f.name == "a2"));
        assert!(b_fields.iter().any(|f| f.name == "b1"));
        assert!(b_fields.iter().any(|f| f.name == "b2"));
        assert!(b_fields.iter().any(|f| f.name == "b3"));
    }
}
