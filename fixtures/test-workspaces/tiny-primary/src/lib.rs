//! Primary workspace library
//! Contains simple data structures for testing

/// A user in the primary workspace
pub struct PrimaryUser {
    pub id: u64,
    pub name: String,
}

impl PrimaryUser {
    /// Create a new user
    pub fn new(id: u64, name: String) -> Self {
        Self { id, name }
    }

    /// Get user display name
    pub fn display_name(&self) -> String {
        format!("User #{}: {}", self.id, self.name)
    }
}

/// Process user data (primary workspace function)
pub fn process_primary_data(user: &PrimaryUser) -> String {
    user.display_name()
}
