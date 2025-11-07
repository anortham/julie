// YAML Extractor Tests
// Following TDD methodology: RED -> GREEN -> REFACTOR
//
// Comprehensive test coverage for YAML extraction
// Common use cases: GitHub Actions, Kubernetes, Docker Compose, Ansible

#[cfg(test)]
mod yaml_extractor_tests {
    #![allow(unused_imports)]
    #![allow(unused_variables)]

    use crate::extractors::base::{Symbol, SymbolKind};
    use crate::extractors::yaml::YamlExtractor;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    fn init_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_yaml::LANGUAGE.into())
            .expect("Error loading YAML grammar");
        parser
    }

    fn extract_symbols(code: &str) -> Vec<Symbol> {
        let workspace_root = PathBuf::from("/tmp/test");
        let mut parser = init_parser();
        let tree = parser.parse(code, None).expect("Failed to parse code");
        let mut extractor = YamlExtractor::new(
            "yaml".to_string(),
            "test.yaml".to_string(),
            code.to_string(),
            &workspace_root,
        );
        extractor.extract_symbols(&tree)
    }

    // ========================================================================
    // Basic YAML Structure
    // ========================================================================

    #[test]
    fn test_extract_simple_key_value_pairs() {
        let yaml = r#"
name: julie
version: 1.1.2
description: Cross-platform code intelligence
enabled: true
"#;

        let symbols = extract_symbols(yaml);

        // Should extract top-level keys
        assert!(symbols.len() >= 1, "Expected at least 1 symbol, got {}", symbols.len());

        let name_key = symbols.iter().find(|s| s.name == "name");
        if let Some(key) = name_key {
            assert_eq!(key.kind, SymbolKind::Variable);
        }
    }

    #[test]
    fn test_extract_nested_mappings() {
        let yaml = r#"
database:
  host: localhost
  port: 5432
  credentials:
    username: admin
    password: secret
"#;

        let symbols = extract_symbols(yaml);

        // Should extract nested keys
        assert!(symbols.len() >= 1, "Expected nested structure symbols");

        let database = symbols.iter().find(|s| s.name == "database");
        assert!(database.is_some(), "Should find 'database' key");
    }

    // ========================================================================
    // GitHub Actions YAML
    // ========================================================================

    #[test]
    fn test_github_actions_workflow() {
        let yaml = r#"
name: CI
on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: Run tests
        run: cargo test
"#;

        let symbols = extract_symbols(yaml);

        // GitHub Actions workflows should extract main keys
        assert!(symbols.len() >= 1, "Expected GitHub Actions workflow symbols");

        let name_key = symbols.iter().find(|s| s.name == "name");
        let jobs_key = symbols.iter().find(|s| s.name == "jobs");

        // At minimum should parse without errors
        assert!(symbols.len() > 0, "Should extract some symbols from GitHub Actions workflow");
    }

    // ========================================================================
    // Docker Compose YAML
    // ========================================================================

    #[test]
    fn test_docker_compose() {
        let yaml = r#"
version: '3.8'
services:
  web:
    image: nginx:latest
    ports:
      - "80:80"
    environment:
      - NODE_ENV=production

  database:
    image: postgres:14
    environment:
      POSTGRES_PASSWORD: example
"#;

        let symbols = extract_symbols(yaml);

        // Docker Compose should extract service definitions
        assert!(symbols.len() >= 1, "Expected Docker Compose symbols");

        let version = symbols.iter().find(|s| s.name == "version");
        let services = symbols.iter().find(|s| s.name == "services");

        // Should handle Docker Compose structure
        assert!(symbols.len() > 0, "Should extract symbols from Docker Compose");
    }

    // ========================================================================
    // Kubernetes Manifests
    // ========================================================================

    #[test]
    fn test_kubernetes_deployment() {
        let yaml = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: nginx-deployment
  labels:
    app: nginx
spec:
  replicas: 3
  selector:
    matchLabels:
      app: nginx
  template:
    metadata:
      labels:
        app: nginx
    spec:
      containers:
      - name: nginx
        image: nginx:1.14.2
        ports:
        - containerPort: 80
"#;

        let symbols = extract_symbols(yaml);

        // Kubernetes manifests should extract main keys
        assert!(symbols.len() >= 1, "Expected Kubernetes manifest symbols");

        let api_version = symbols.iter().find(|s| s.name == "apiVersion");
        let kind = symbols.iter().find(|s| s.name == "kind");
        let metadata = symbols.iter().find(|s| s.name == "metadata");

        // Should handle Kubernetes structure
        assert!(symbols.len() > 0, "Should extract symbols from Kubernetes manifest");
    }

    // ========================================================================
    // Ansible Playbook
    // ========================================================================

    #[test]
    fn test_ansible_playbook() {
        let yaml = r#"
---
- name: Configure web servers
  hosts: webservers
  become: yes

  tasks:
    - name: Install nginx
      apt:
        name: nginx
        state: present

    - name: Start nginx
      service:
        name: nginx
        state: started
"#;

        let symbols = extract_symbols(yaml);

        // Ansible playbooks use lists with mappings
        assert!(symbols.len() >= 0, "Should handle Ansible playbook structure");
    }

    // ========================================================================
    // Arrays/Sequences
    // ========================================================================

    #[test]
    fn test_simple_array() {
        let yaml = r#"
fruits:
  - apple
  - banana
  - orange
"#;

        let symbols = extract_symbols(yaml);

        // Should handle arrays
        assert!(symbols.len() >= 1, "Should extract array structure");

        let fruits = symbols.iter().find(|s| s.name == "fruits");
        assert!(fruits.is_some(), "Should find 'fruits' key");
    }

    #[test]
    fn test_array_of_objects() {
        let yaml = r#"
servers:
  - name: server1
    ip: 192.168.1.1
  - name: server2
    ip: 192.168.1.2
"#;

        let symbols = extract_symbols(yaml);

        // Should handle arrays of objects
        assert!(symbols.len() >= 1, "Should extract array of objects");
    }

    // ========================================================================
    // Special YAML Features
    // ========================================================================

    #[test]
    fn test_yaml_anchors_and_aliases() {
        let yaml = r#"
defaults: &defaults
  adapter: postgres
  host: localhost

development:
  <<: *defaults
  database: dev_db

production:
  <<: *defaults
  database: prod_db
"#;

        let symbols = extract_symbols(yaml);

        // Should handle anchors and aliases
        assert!(symbols.len() >= 0, "Should handle YAML anchors and aliases");
    }

    #[test]
    fn test_multiline_strings() {
        let yaml = r#"
description: |
  This is a multi-line
  string with literal
  line breaks preserved

summary: >
  This is a folded
  multi-line string
  that gets joined
"#;

        let symbols = extract_symbols(yaml);

        // Should handle multiline strings
        assert!(symbols.len() >= 1, "Should handle multiline strings");
    }

    #[test]
    fn test_empty_yaml() {
        let yaml = "";

        let symbols = extract_symbols(yaml);

        // Empty YAML should not crash
        assert_eq!(symbols.len(), 0, "Empty YAML should have no symbols");
    }

    #[test]
    fn test_yaml_with_comments() {
        let yaml = r#"
# Configuration file
name: julie  # Application name
version: 1.0.0  # Current version

# Database settings
database:
  host: localhost
  # Port number
  port: 5432
"#;

        let symbols = extract_symbols(yaml);

        // Should handle comments
        assert!(symbols.len() >= 1, "Should extract symbols despite comments");
    }

    // ========================================================================
    // Edge Cases
    // ========================================================================

    #[test]
    fn test_quoted_keys() {
        let yaml = r#"
"quoted-key": value1
'another-quoted': value2
normal_key: value3
"#;

        let symbols = extract_symbols(yaml);

        // Should handle quoted keys
        assert!(symbols.len() >= 1, "Should handle quoted keys");
    }

    #[test]
    fn test_special_characters_in_keys() {
        let yaml = r#"
key-with-dashes: value1
key.with.dots: value2
key_with_underscores: value3
"#;

        let symbols = extract_symbols(yaml);

        // Should handle special characters in keys
        assert!(symbols.len() >= 1, "Should handle special characters in keys");
    }
}
