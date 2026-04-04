use std::collections::HashMap;

pub struct UserService {
    users: HashMap<u64, String>,
    api_url: String,
}

impl UserService {
    pub fn new(api_url: String) -> Self {
        Self {
            users: HashMap::new(),
            api_url,
        }
    }

    pub fn get_user(&self, id: u64) -> Option<&String> {
        self.users.get(&id)
    }

    pub fn add_user(&mut self, id: u64, name: String) {
        self.users.insert(id, name);
    }

    pub fn count(&self) -> usize {
        self.users.len()
    }
}
