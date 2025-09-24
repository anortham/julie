// Julie's Test Infrastructure
//
// This module contains test utilities and infrastructure for testing extractors,
// search functionality, and other Julie components.

#[cfg(test)]
pub mod test_helpers {
    use std::path::Path;
    use tempfile::TempDir;
    use anyhow::Result;

    /// Create a temporary test workspace
    pub fn create_test_workspace() -> Result<TempDir> {
        Ok(tempfile::tempdir()?)
    }

    /// Create a test file with content
    pub fn create_test_file(dir: &Path, filename: &str, content: &str) -> Result<std::path::PathBuf> {
        use std::fs;
        let file_path = dir.join(filename);
        fs::write(&file_path, content)?;
        Ok(file_path)
    }

    /// Common test code snippets for various languages
    pub mod test_code {
        /// TypeScript test code
        pub const TYPESCRIPT_SAMPLE: &str = r#"
interface User {
    id: number;
    name: string;
    email: string;
}

class UserService {
    private users: User[] = [];

    constructor(private apiUrl: string) {}

    async getUser(id: number): Promise<User | null> {
        const response = await fetch(`${this.apiUrl}/users/${id}`);
        return response.json();
    }

    addUser(user: User): void {
        this.users.push(user);
    }
}

export { User, UserService };
        "#;

        /// Python test code
        pub const PYTHON_SAMPLE: &str = r#"
from typing import List, Optional
import asyncio

class User:
    def __init__(self, id: int, name: str, email: str):
        self.id = id
        self.name = name
        self.email = email

class UserService:
    def __init__(self, api_url: str):
        self.api_url = api_url
        self.users: List[User] = []

    async def get_user(self, id: int) -> Optional[User]:
        # Simulate API call
        await asyncio.sleep(0.1)
        return next((u for u in self.users if u.id == id), None)

    def add_user(self, user: User) -> None:
        self.users.append(user)
        "#;

        /// Rust test code
        pub const RUST_SAMPLE: &str = r#"
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub email: String,
}

pub struct UserService {
    api_url: String,
    users: HashMap<u64, User>,
}

impl UserService {
    pub fn new(api_url: String) -> Self {
        Self {
            api_url,
            users: HashMap::new(),
        }
    }

    pub async fn get_user(&self, id: u64) -> Option<&User> {
        self.users.get(&id)
    }

    pub fn add_user(&mut self, user: User) {
        self.users.insert(user.id, user);
    }
}
        "#;
    }
}

// TODO: Add specific test modules as extractors are implemented:
// pub mod typescript_tests;
// pub mod python_tests;
// pub mod rust_tests;
// etc.