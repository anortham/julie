// Generic Type Tests
//
// Tests for Java generics including:
// - Generic classes and interfaces
// - Generic methods
// - Wildcards (? extends, ? super)
// - Type parameters with bounds

use super::*;
use std::path::PathBuf;

#[cfg(test)]
mod generic_tests {
    use super::*;

    #[test]
    fn test_extract_generic_classes() {
        let workspace_root = PathBuf::from("/tmp/test");
        let code = r#"
public class Container<T> {
    private T value;

    public T getValue() {
        return value;
    }
}

class Pair<K, V> {
    private K key;
    private V value;
}

interface Repository<T, ID> {
    T findById(ID id);
    List<T> findAll();
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        let container_class = symbols.iter().find(|s| s.name == "Container");
        assert!(container_class.is_some());
        assert_eq!(container_class.unwrap().kind, SymbolKind::Class);
        assert!(container_class
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("public class Container<T>"));

        let pair_class = symbols.iter().find(|s| s.name == "Pair");
        assert!(pair_class.is_some());
        assert!(pair_class
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("class Pair<K, V>"));

        let repository_interface = symbols.iter().find(|s| s.name == "Repository");
        assert!(repository_interface.is_some());
        assert_eq!(repository_interface.unwrap().kind, SymbolKind::Interface);
        assert!(repository_interface
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("interface Repository<T, ID>"));
    }

    #[test]
    fn test_extract_generic_methods() {
        let workspace_root = PathBuf::from("/tmp/test");
        let code = r#"
public class Utils {
    public static <T> T getFirst(List<T> list) {
        return list.get(0);
    }

    public <K, V> Map<K, V> createMap(K key, V value) {
        Map<K, V> map = new HashMap<>();
        map.put(key, value);
        return map;
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        let get_first_method = symbols.iter().find(|s| s.name == "getFirst");
        assert!(get_first_method.is_some());
        assert_eq!(get_first_method.unwrap().kind, SymbolKind::Method);
        assert!(get_first_method
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("public static <T> T getFirst"));

        let create_map_method = symbols.iter().find(|s| s.name == "createMap");
        assert!(create_map_method.is_some());
        assert!(create_map_method
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("public <K, V> Map<K, V> createMap"));
    }

    #[test]
    fn test_extract_wildcards_and_bounds() {
        let workspace_root = PathBuf::from("/tmp/test");
        let code = r#"
import java.util.*;

public class Processor {
    public void processNumbers(List<? extends Number> numbers) {
        // Process numbers
    }

    public void addNumbers(List<? super Integer> integers) {
        integers.add(42);
    }

    public <T extends Comparable<T>> T findMax(List<T> items) {
        return items.stream().max(T::compareTo).orElse(null);
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        let process_numbers = symbols.iter().find(|s| s.name == "processNumbers");
        assert!(process_numbers.is_some());
        assert!(process_numbers
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("List<? extends Number>"));

        let add_numbers = symbols.iter().find(|s| s.name == "addNumbers");
        assert!(add_numbers.is_some());
        assert!(add_numbers
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("List<? super Integer>"));

        let find_max = symbols.iter().find(|s| s.name == "findMax");
        assert!(find_max.is_some());
        assert!(find_max
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("<T extends Comparable<T>>"));
    }
}
