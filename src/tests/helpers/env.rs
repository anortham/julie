//! Process-environment guard that restores the previous value on drop.

use std::collections::HashMap;
use std::ffi::OsString;

pub struct EnvVarGuard {
    original: HashMap<String, Option<OsString>>,
}

impl EnvVarGuard {
    pub fn new() -> Self {
        Self {
            original: HashMap::new(),
        }
    }

    fn remember(&mut self, key: &str) {
        if !self.original.contains_key(key) {
            self.original.insert(key.to_string(), std::env::var_os(key));
        }
    }

    pub fn set(&mut self, key: &str, value: impl Into<OsString>) {
        self.remember(key);
        unsafe {
            std::env::set_var(key, value.into());
        }
    }

    pub fn remove(&mut self, key: &str) {
        self.remember(key);
        unsafe {
            std::env::remove_var(key);
        }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        for (key, value) in self.original.drain() {
            match value {
                Some(original_value) => unsafe {
                    std::env::set_var(&key, original_value);
                },
                None => unsafe {
                    std::env::remove_var(&key);
                },
            }
        }
    }
}
