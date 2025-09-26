use std::collections::HashMap;

pub fn fetch_user_info(id: u32) -> Option<String> {
    let mut users = HashMap::new();
    users.insert(1, "Alice".to_string());
    users.insert(2, "Bob".to_string());

    users.get(&id).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fetch_user_info() {
        assert_eq!(fetch_user_info(1), Some("Alice".to_string()));
        assert_eq!(fetch_user_info(3), None);
    }
}