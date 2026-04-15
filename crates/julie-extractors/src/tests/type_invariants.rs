use crate::extract_canonical;

#[test]
fn test_go_return_type_map_is_exact() {
    let workspace_root = std::path::PathBuf::from("/test/workspace");
    let code = r#"
package main

func GetUserName(userId int) string {
    return fmt.Sprintf("User%d", userId)
}

func GetAllUsers() ([]User, error) {
    return repository.FindAll()
}

func GetUserScores() map[string]int {
    return make(map[string]int)
}
"#;

    let results = extract_canonical("src/service.go", code, &workspace_root)
        .expect("canonical extraction should succeed");

    let type_map: std::collections::HashMap<_, _> = results
        .types
        .values()
        .filter_map(|type_info| {
            results
                .symbols
                .iter()
                .find(|symbol| symbol.id == type_info.symbol_id)
                .map(|symbol| (symbol.name.as_str(), type_info.resolved_type.as_str()))
        })
        .collect();

    assert_eq!(type_map.get("GetUserName"), Some(&"string"));
    assert_eq!(type_map.get("GetUserScores"), Some(&"map[string]int"));
    assert!(!type_map.contains_key("GetAllUsers"));
}
