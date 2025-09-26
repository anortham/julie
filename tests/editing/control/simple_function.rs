use std::collections::HashMap;

pub fn get_user_data(id: u32) -> Option<String> {
    let mut users = HashMap::new();
    users.insert(1, "Alice".to_string());
    users.insert(2, "Bob".to_string());

    users.get(&id).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_user_data() {
        assert_eq!(get_user_data(1), Some("Alice".to_string()));
        assert_eq!(get_user_data(3), None);
    }
}