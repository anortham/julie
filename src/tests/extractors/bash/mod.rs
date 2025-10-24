// Bash Extractor Tests (ported from Miller's bash-extractor.test.ts)
// Following TDD methodology: RED -> GREEN -> REFACTOR -> ENHANCE

#[cfg(test)]
mod bash_extractor_tests {
    #![allow(unused_imports)]
    #![allow(unused_variables)]

    use crate::extractors::base::{IdentifierKind, RelationshipKind, Symbol, SymbolKind};
    use crate::extractors::bash::BashExtractor;
    use tree_sitter::Parser;

    fn init_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_bash::LANGUAGE.into())
            .expect("Error loading Bash grammar");
        parser
    }

    fn extract_symbols(code: &str) -> Vec<Symbol> {
        let mut parser = init_parser();
        let tree = parser.parse(code, None).expect("Failed to parse code");
        let mut extractor =
            BashExtractor::new("bash".to_string(), "test.sh".to_string(), code.to_string());
        extractor.extract_symbols(&tree)
    }

    fn extract_symbols_and_relationships(
        code: &str,
    ) -> (Vec<Symbol>, Vec<crate::extractors::base::Relationship>) {
        let mut parser = init_parser();
        let tree = parser.parse(code, None).expect("Failed to parse code");
        let mut extractor =
            BashExtractor::new("bash".to_string(), "test.sh".to_string(), code.to_string());
        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);
        (symbols, relationships)
    }

    #[test]
    fn test_extract_bash_functions_and_parameters() {
        let bash_code = r#"#!/bin/bash

# Main deployment function
deploy_app() {
    local environment=$1
    local app_name=$2

    echo "Deploying $app_name to $environment"
    build_app "$app_name"
    test_app
}

# Build function
build_app() {
    local name=$1
    npm install
    npm run build
}

test_app() {
    npm test
}

# Environment variables
export NODE_ENV="production"
DATABASE_URL="postgres://localhost:5432/app"
readonly API_KEY="secret123"
declare -r CONFIG_PATH="/etc/app/config"
"#;

        let symbols = extract_symbols(bash_code);

        // Should extract functions
        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();
        assert!(
            functions.len() >= 3,
            "Expected at least 3 functions, got {}",
            functions.len()
        );

        let deploy_app = functions.iter().find(|f| f.name == "deploy_app");
        assert!(deploy_app.is_some(), "deploy_app function not found");
        let deploy_app = deploy_app.unwrap();
        assert_eq!(
            deploy_app.signature,
            Some("function deploy_app()".to_string())
        );
        assert_eq!(
            deploy_app.visibility,
            Some(crate::extractors::base::Visibility::Public)
        );

        let build_app = functions.iter().find(|f| f.name == "build_app");
        assert!(build_app.is_some(), "build_app function not found");

        let test_app = functions.iter().find(|f| f.name == "test_app");
        assert!(test_app.is_some(), "test_app function not found");

        // Should extract variables
        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable || s.kind == SymbolKind::Constant)
            .collect();
        assert!(
            variables.len() >= 4,
            "Expected at least 4 variables, got {}",
            variables.len()
        );

        let node_env = variables.iter().find(|v| v.name == "NODE_ENV");
        assert!(node_env.is_some(), "NODE_ENV variable not found");
        let node_env = node_env.unwrap();
        assert_eq!(
            node_env.visibility,
            Some(crate::extractors::base::Visibility::Public)
        ); // exported

        let api_key = variables.iter().find(|v| v.name == "API_KEY");
        assert!(api_key.is_some(), "API_KEY variable not found");
        let api_key = api_key.unwrap();
        assert_eq!(api_key.kind, SymbolKind::Constant); // readonly

        // Should extract positional parameters
        let parameters: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.name.starts_with('$') && s.kind == SymbolKind::Variable)
            .collect();
        assert!(
            parameters.len() >= 2,
            "Expected at least 2 positional parameters, got {}",
            parameters.len()
        );

        let param1 = parameters.iter().find(|p| p.name == "$1");
        assert!(param1.is_some(), "$1 parameter not found");
        let param1 = param1.unwrap();
        assert_eq!(
            param1.signature,
            Some("$1 (positional parameter)".to_string())
        );
    }

    #[test]
    fn test_extract_devops_and_cross_language_command_calls() {
        let bash_code = r#"#!/bin/bash

# DevOps deployment script
setup_environment() {
    # Python application setup
    python3 setup.py install
    pip install -r requirements.txt

    # Node.js service
    npm install
    bun install --production
    node server.js &

    # Go microservice
    go build -o service ./cmd/service
    ./service &

    # Container orchestration
    docker build -t myapp .
    docker-compose up -d
    kubectl apply -f k8s/

    # Infrastructure
    terraform plan
    terraform apply -auto-approve

    # Version control
    git pull origin main
    git push origin feature/new-deploy
}

# Database operations
database_ops() {
    # Java application
    java -jar app.jar migrate
    mvn spring-boot:run &

    # .NET service
    dotnet build
    dotnet run &

    # PHP web service
    php composer.phar install
    php -S localhost:8080 &

    # Ruby service
    bundle install
    ruby app.rb &
}

# Monitoring and tools
monitoring_setup() {
    curl -X POST https://api.service.com/health
    ssh deploy@server "systemctl status myapp"
    scp config.json deploy@server:/etc/myapp/
}
"#;

        let symbols = extract_symbols(bash_code);

        // Should extract cross-language commands
        let commands: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| {
                s.kind == SymbolKind::Function
                    && [
                        "python3",
                        "npm",
                        "bun",
                        "node",
                        "go",
                        "docker",
                        "kubectl",
                        "terraform",
                        "git",
                        "java",
                        "mvn",
                        "dotnet",
                        "php",
                        "ruby",
                        "curl",
                        "ssh",
                        "scp",
                    ]
                    .contains(&s.name.as_str())
            })
            .collect();

        assert!(
            commands.len() >= 10,
            "Expected at least 10 cross-language commands, got {}",
            commands.len()
        );

        // Verify specific commands
        let python_cmd = commands.iter().find(|c| c.name == "python3");
        assert!(python_cmd.is_some(), "python3 command not found");
        let python_cmd = python_cmd.unwrap();
        // Now extracts real doc comment from code
        assert_eq!(
            python_cmd.doc_comment,
            Some("# Python application setup".to_string())
        );

        let node_cmd = commands.iter().find(|c| c.name == "node");
        assert!(node_cmd.is_some(), "node command not found");
        let node_cmd = node_cmd.unwrap();
        // Now extracts real doc comment from code (shebang + description)
        assert!(
            node_cmd
                .doc_comment
                .as_ref()
                .map(|d| d.contains("DevOps")
                    || d.contains("deployment")
                    || d.contains("Node.js service"))
                .unwrap_or(false),
            "Node comment should mention deployment or service, got: {:?}",
            node_cmd.doc_comment
        );

        let docker_cmd = commands.iter().find(|c| c.name == "docker");
        assert!(docker_cmd.is_some(), "docker command not found");
        let docker_cmd = docker_cmd.unwrap();
        // Now extracts real doc comment from code (Container orchestration section)
        assert!(
            docker_cmd
                .doc_comment
                .as_ref()
                .map(|d| d.contains("Container") || d.contains("orchestration"))
                .unwrap_or(false),
            "Docker comment should mention container orchestration, got: {:?}",
            docker_cmd.doc_comment
        );

        let kubectl_cmd = commands.iter().find(|c| c.name == "kubectl");
        assert!(kubectl_cmd.is_some(), "kubectl command not found");
        let kubectl_cmd = kubectl_cmd.unwrap();
        // Now extracts real doc comment from code
        assert!(
            kubectl_cmd.doc_comment.is_some(),
            "kubectl should have doc_comment"
        );

        let terraform_cmd = commands.iter().find(|c| c.name == "terraform");
        assert!(terraform_cmd.is_some(), "terraform command not found");
        let terraform_cmd = terraform_cmd.unwrap();
        // Now extracts real doc comment from code
        assert!(
            terraform_cmd.doc_comment.is_some(),
            "terraform should have doc_comment"
        );

        let bun_cmd = commands.iter().find(|c| c.name == "bun");
        assert!(bun_cmd.is_some(), "bun command not found");
        let bun_cmd = bun_cmd.unwrap();
        // Now extracts real doc comment from code
        assert!(bun_cmd.doc_comment.is_some(), "bun should have doc_comment");
    }

    #[test]
    fn test_extract_control_flow_constructs_and_environment_variables() {
        let bash_code = r#"#!/bin/bash

# Environment setup
export DOCKER_HOST="tcp://localhost:2376"
export KUBECONFIG="/home/user/.kube/config"
PATH="/usr/local/bin:$PATH"
HOME="/home/deploy"
NODE_ENV="development"

# Conditional deployment
if [ "$NODE_ENV" = "production" ]; then
    echo "Production deployment"
    for service in api frontend worker; do
        echo "Starting $service"
        docker start $service
    done
elif [ "$NODE_ENV" = "staging" ]; then
    echo "Staging deployment"
    while read -r line; do
        echo "Processing: $line"
    done < services.txt
else
    echo "Development environment"
fi

# Function with complex logic
deploy_with_rollback() {
    local deployment_id=$1

    if deploy_service "$deployment_id"; then
        echo "Deployment successful"
        return 0
    else
        echo "Deployment failed, rolling back"
        rollback_service "$deployment_id"
        return 1
    fi
}
"#;

        let symbols = extract_symbols(bash_code);

        // Should extract environment variables
        let env_vars: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| {
                (s.kind == SymbolKind::Constant || s.kind == SymbolKind::Variable)
                    && ["DOCKER_HOST", "KUBECONFIG", "PATH", "HOME", "NODE_ENV"]
                        .contains(&s.name.as_str())
            })
            .collect();
        assert!(
            env_vars.len() >= 3,
            "Expected at least 3 environment variables, got {}",
            env_vars.len()
        );

        let docker_host = env_vars.iter().find(|v| v.name == "DOCKER_HOST");
        assert!(docker_host.is_some(), "DOCKER_HOST variable not found");
        let docker_host = docker_host.unwrap();
        assert_eq!(
            docker_host.visibility,
            Some(crate::extractors::base::Visibility::Public)
        ); // exported
           // Now extracts real doc comment from code
           // May contain shebang and section comment, so just check it contains the setup comment
        assert!(
            docker_host
                .doc_comment
                .as_ref()
                .map(|d| d.contains("Environment setup"))
                .unwrap_or(false),
            "DOCKER_HOST comment should contain 'Environment setup', got: {:?}",
            docker_host.doc_comment
        );

        let kube_config = env_vars.iter().find(|v| v.name == "KUBECONFIG");
        assert!(kube_config.is_some(), "KUBECONFIG variable not found");
        let kube_config = kube_config.unwrap();
        assert_eq!(
            kube_config.visibility,
            Some(crate::extractors::base::Visibility::Public)
        ); // exported

        // Should extract control flow
        let control_flow: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Method)
            .collect();
        assert!(
            control_flow.len() >= 2,
            "Expected at least 2 control flow blocks, got {}",
            control_flow.len()
        );

        let if_block = control_flow.iter().find(|c| c.name.contains("if block"));
        assert!(if_block.is_some(), "if block not found");
        let if_block = if_block.unwrap();
        // Now extracts real doc comment from code (Conditional deployment section)
        assert!(
            if_block
                .doc_comment
                .as_ref()
                .map(|d| d.contains("Conditional") || d.contains("deployment"))
                .unwrap_or(false),
            "If block comment should mention deployment, got: {:?}",
            if_block.doc_comment
        );

        let for_block = control_flow.iter().find(|c| c.name.contains("for block"));
        assert!(for_block.is_some(), "for block not found");
        let for_block = for_block.unwrap();
        // Now extracts real doc comment from code (Conditional deployment section)
        assert!(
            for_block.doc_comment.is_some(),
            "For block should have doc_comment"
        );

        // Should extract functions
        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();
        let deploy_func = functions.iter().find(|f| f.name == "deploy_with_rollback");
        assert!(
            deploy_func.is_some(),
            "deploy_with_rollback function not found"
        );
        let deploy_func = deploy_func.unwrap();
        assert_eq!(
            deploy_func.signature,
            Some("function deploy_with_rollback()".to_string())
        );
    }

    #[test]
    fn test_infer_variable_types_and_extract_documentation() {
        let bash_code = r#"#!/bin/bash

# Configuration variables
PORT=8080                    # integer
HOST="localhost"             # string
DEBUG=true                   # boolean
RATE_LIMIT=10.5             # float
CONFIG_PATH="/etc/app"       # path
ARRAY=("item1" "item2")      # array

# Special declarations
declare -i COUNTER=0         # integer declaration
declare -r VERSION="1.0.0"  # readonly string
export -n LOCAL_VAR="test"   # unexported variable
readonly -a SERVICES=("api" "worker")  # readonly array

# Function with local variables
configure_app() {
    local app_name=$1
    local -i retry_count=3
    local -r max_attempts=10

    echo "Configuring $app_name"
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(bash_code, None).expect("Failed to parse code");
        let mut extractor = BashExtractor::new(
            "bash".to_string(),
            "types.sh".to_string(),
            bash_code.to_string(),
        );
        let symbols = extractor.extract_symbols(&tree);
        let types = extractor.infer_types(&symbols);

        // Should infer types correctly
        assert_eq!(types.get("PORT"), Some(&"integer".to_string()));
        assert_eq!(types.get("HOST"), Some(&"string".to_string()));
        assert_eq!(types.get("DEBUG"), Some(&"boolean".to_string()));
        assert_eq!(types.get("RATE_LIMIT"), Some(&"float".to_string()));
        assert_eq!(types.get("CONFIG_PATH"), Some(&"path".to_string()));

        // Extract symbols to verify declarations
        let declarations: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| ["COUNTER", "VERSION", "LOCAL_VAR", "SERVICES"].contains(&s.name.as_str()))
            .collect();
        assert!(
            declarations.len() >= 3,
            "Expected at least 3 declarations, got {}",
            declarations.len()
        );

        let version_var = declarations.iter().find(|d| d.name == "VERSION");
        assert!(version_var.is_some(), "VERSION variable not found");
        let version_var = version_var.unwrap();
        assert_eq!(version_var.kind, SymbolKind::Constant); // readonly
                                                            // Now extracts real doc comment from code (readonly declaration comment)
        assert!(
            version_var.doc_comment.is_some(),
            "VERSION should have doc comment"
        );
        // The comment is either "# Special declarations" or "declare -r ..." depending on
        // how find_doc_comment locates it. Just verify it's not the old "[READONLY]" annotation.
        let doc = version_var.doc_comment.as_ref().unwrap();
        assert!(
            !doc.contains("[READONLY]"),
            "Should not have [READONLY] annotation anymore"
        );

        let counter_var = declarations.iter().find(|d| d.name == "COUNTER");
        assert!(counter_var.is_some(), "COUNTER variable not found");
        let counter_var = counter_var.unwrap();
        assert_eq!(counter_var.signature, Some("declare COUNTER".to_string()));
    }

    #[test]
    fn test_extract_function_call_relationships() {
        let bash_code = r#"#!/bin/bash

# Main orchestration function
main() {
    setup_environment
    deploy_services
    verify_deployment
}

# Setup function that calls other functions
setup_environment() {
    install_dependencies
    configure_services
    start_monitoring
}

# Individual service functions
install_dependencies() {
    npm install
    python3 -m pip install -r requirements.txt
}

deploy_services() {
    docker-compose up -d
    kubectl apply -f ./k8s/
}

verify_deployment() {
    curl -f http://localhost:8080/health
    python3 scripts/verify.py
}

# Entry point
main "$@"
"#;

        let (symbols, relationships) = extract_symbols_and_relationships(bash_code);

        // Should extract function call relationships
        let call_relationships: Vec<&crate::extractors::base::Relationship> = relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Calls)
            .collect();
        assert!(
            call_relationships.len() >= 3,
            "Expected at least 3 call relationships, got {}",
            call_relationships.len()
        );

        // Verify specific relationships
        let main_function = symbols
            .iter()
            .find(|s| s.name == "main" && s.kind == SymbolKind::Function);
        let setup_function = symbols
            .iter()
            .find(|s| s.name == "setup_environment" && s.kind == SymbolKind::Function);

        assert!(main_function.is_some(), "main function not found");
        assert!(
            setup_function.is_some(),
            "setup_environment function not found"
        );

        let main_function = main_function.unwrap();
        let setup_function = setup_function.unwrap();

        // Should have relationship from main to setup_environment
        let main_to_setup = call_relationships
            .iter()
            .find(|r| r.from_symbol_id == main_function.id && r.to_symbol_id == setup_function.id);
        assert!(
            main_to_setup.is_some(),
            "main -> setup_environment relationship not found"
        );

        // Should extract external command calls
        let commands: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| {
                s.kind == SymbolKind::Function
                    && ["npm", "python3", "docker-compose", "kubectl", "curl"]
                        .contains(&s.name.as_str())
            })
            .collect();
        assert!(
            commands.len() >= 4,
            "Expected at least 4 external command calls, got {}",
            commands.len()
        );
    }

    #[test]
    fn test_handle_malformed_bash_and_extraction_errors_gracefully() {
        let malformed_bash = r#"#!/bin/bash

# Function with minor issues but still parseable
working_function() {
    echo "This should work"
    export VALID_VAR="value"
    # Some undefined variables (not syntax errors)
    echo $UNDEFINED_VAR
}

# Another valid function
helper_function() {
    echo "Helper function"
}
"#;

        // Should not panic
        let symbols = extract_symbols(malformed_bash);

        // Should still extract valid symbols
        let valid_function = symbols.iter().find(|s| s.name == "working_function");
        assert!(valid_function.is_some(), "working_function not found");

        let valid_var = symbols.iter().find(|s| s.name == "VALID_VAR");
        assert!(valid_var.is_some(), "VALID_VAR not found");
    }

    #[test]
    fn test_handle_empty_files_and_minimal_content() {
        let empty_bash = "";
        let minimal_bash = "#!/bin/bash\n# Just a comment\n";

        let empty_symbols = extract_symbols(empty_bash);
        let minimal_symbols = extract_symbols(minimal_bash);

        // Should handle gracefully without errors
        assert!(
            empty_symbols.is_empty(),
            "Empty bash should produce no symbols"
        );
        assert!(
            minimal_symbols.is_empty(),
            "Minimal bash should produce no symbols"
        );
    }
}

// Bash Identifier Extraction Tests (TDD RED phase)
//
// These tests validate the extract_identifiers() functionality which extracts:
// - Command invocations (command nodes)
// - Array/subscript access (subscript nodes)
// - Proper containing symbol tracking (file-scoped)
//
// Following the Rust extractor reference implementation pattern

#[cfg(test)]
mod identifier_extraction_tests {
    use crate::extractors::base::{IdentifierKind, SymbolKind};
    use crate::extractors::bash::BashExtractor;
    use tree_sitter::Parser;

    fn init_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_bash::LANGUAGE.into())
            .expect("Error loading Bash grammar");
        parser
    }

    #[test]
    fn test_extract_function_calls() {
        let bash_code = r#"#!/bin/bash

deploy_app() {
    local environment=$1

    echo "Deploying to $environment"
    build_app "$environment"      # Command call to build_app
    npm install                    # Command call to npm
    test_deployment                # Command call to test_deployment
}

build_app() {
    echo "Building app"
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(bash_code, None).unwrap();

        let mut extractor = BashExtractor::new(
            "bash".to_string(),
            "test.sh".to_string(),
            bash_code.to_string(),
        );

        // Extract symbols first
        let symbols = extractor.extract_symbols(&tree);

        // NOW extract identifiers (this will FAIL until we implement it)
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Verify we found the command calls
        let build_call = identifiers.iter().find(|id| id.name == "build_app");
        assert!(
            build_call.is_some(),
            "Should extract 'build_app' command call identifier"
        );
        let build_call = build_call.unwrap();
        assert_eq!(build_call.kind, IdentifierKind::Call);

        let npm_call = identifiers.iter().find(|id| id.name == "npm");
        assert!(
            npm_call.is_some(),
            "Should extract 'npm' command call identifier"
        );
        let npm_call = npm_call.unwrap();
        assert_eq!(npm_call.kind, IdentifierKind::Call);

        // Verify containing symbol is set correctly (should be inside deploy_app function)
        assert!(
            build_call.containing_symbol_id.is_some(),
            "Command call should have containing symbol"
        );

        // Find the deploy_app function symbol
        let deploy_method = symbols.iter().find(|s| s.name == "deploy_app").unwrap();

        // Verify the build_app call is contained within deploy_app function
        assert_eq!(
            build_call.containing_symbol_id.as_ref(),
            Some(&deploy_method.id),
            "build_app call should be contained within deploy_app function"
        );
    }

    #[test]
    fn test_extract_member_access() {
        let bash_code = r#"#!/bin/bash

process_data() {
    # Array access using subscript
    local names=("Alice" "Bob" "Charlie")

    echo "${names[0]}"      # Subscript access: names[0]
    local first="${names[1]}"  # Subscript access: names[1]
    echo "${data[key]}"     # Subscript access: data[key]
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(bash_code, None).unwrap();

        let mut extractor = BashExtractor::new(
            "bash".to_string(),
            "test.sh".to_string(),
            bash_code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Verify we found subscript access identifiers
        let names_access = identifiers
            .iter()
            .filter(|id| id.name == "names" && id.kind == IdentifierKind::MemberAccess)
            .count();
        assert!(
            names_access > 0,
            "Should extract 'names' subscript access identifier"
        );

        let data_access = identifiers
            .iter()
            .filter(|id| id.name == "data" && id.kind == IdentifierKind::MemberAccess)
            .count();
        assert!(
            data_access > 0,
            "Should extract 'data' subscript access identifier"
        );
    }

    #[test]
    fn test_file_scoped_containing_symbol() {
        // This test ensures we ONLY match symbols from the SAME FILE
        // Critical bug fix from Rust implementation
        let bash_code = r#"#!/bin/bash

main_function() {
    helper_function    # Call to helper_function in same file
}

helper_function() {
    echo "Helper"
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(bash_code, None).unwrap();

        let mut extractor = BashExtractor::new(
            "bash".to_string(),
            "test.sh".to_string(),
            bash_code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Find the helper_function call
        let helper_call = identifiers.iter().find(|id| id.name == "helper_function");
        assert!(helper_call.is_some());
        let helper_call = helper_call.unwrap();

        // Verify it has a containing symbol (the main_function)
        assert!(
            helper_call.containing_symbol_id.is_some(),
            "helper_function call should have containing symbol from same file"
        );

        // Verify the containing symbol is the main_function
        let main_func = symbols.iter().find(|s| s.name == "main_function").unwrap();
        assert_eq!(
            helper_call.containing_symbol_id.as_ref(),
            Some(&main_func.id),
            "helper_function call should be contained within main_function"
        );
    }

    #[test]
    fn test_chained_member_access() {
        let bash_code = r#"#!/bin/bash

process_config() {
    # Nested array access
    local config=("${settings[0]}" "${options[1]}")

    echo "${matrix[row][col]}"     # Chained subscript access
    local value="${data[key1][key2]}"  # Chained subscript access
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(bash_code, None).unwrap();

        let mut extractor = BashExtractor::new(
            "bash".to_string(),
            "test.sh".to_string(),
            bash_code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Should extract subscript access from nested structures
        let settings_access = identifiers
            .iter()
            .find(|id| id.name == "settings" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            settings_access.is_some(),
            "Should extract 'settings' from subscript access"
        );

        let matrix_access = identifiers
            .iter()
            .find(|id| id.name == "matrix" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            matrix_access.is_some(),
            "Should extract 'matrix' from chained subscript access"
        );
    }

    #[test]
    fn test_no_duplicate_identifiers() {
        let bash_code = r#"#!/bin/bash

run_tests() {
    npm test
    npm test  # Same command twice
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(bash_code, None).unwrap();

        let mut extractor = BashExtractor::new(
            "bash".to_string(),
            "test.sh".to_string(),
            bash_code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Should extract BOTH calls (they're at different locations)
        let npm_calls: Vec<_> = identifiers
            .iter()
            .filter(|id| id.name == "npm" && id.kind == IdentifierKind::Call)
            .collect();

        assert_eq!(
            npm_calls.len(),
            2,
            "Should extract both npm calls at different locations"
        );

        // Verify they have different line numbers
        assert_ne!(
            npm_calls[0].start_line, npm_calls[1].start_line,
            "Duplicate calls should have different line numbers"
        );
    }

    #[test]
    fn test_extract_complex_parameter_expansion() {
        let bash_code = r##"#!/bin/bash

# Complex parameter expansion patterns
process_filename() {
    local filename=$1

    # Pattern replacement
    local base="${filename%.*}"        # Remove extension
    local extension="${filename##*.}"  # Get extension
    local dirname="${filename%/*}"     # Get directory
    local basename="${filename##*/}"   # Get basename

    # Advanced pattern replacement
    local clean_name="${filename//[^a-zA-Z0-9]/_}"  # Replace non-alnum with _
    local upper_name="${filename^^}"                 # Uppercase
    local lower_name="${filename,,}"                 # Lowercase

    # Conditional expansion
    local default_value="${undefined_var:-default}"
    local error_value="${undefined_var:?ERROR: var not set}"
    local alt_value="${filename:+alternate}"

    # Substring operations
    local first_five="${filename:0:5}"
    local from_pos="${filename:3}"
    local last_five="${filename: -5}"

    # Array parameter expansion
    local files=("$@")
    local first_file="${files[0]}"
    local all_files="${files[*]}"
    local count="${#files[@]}"

    echo "Processed: $clean_name"
}

# Test various expansion patterns
test_expansions() {
    local text="Hello_World-123.test"

    # Pattern replacement
    local snake_case="${text//-/_}"
    local no_digits="${text//[0-9]/}"
    local reversed="${text//Hello/World}"

    # Length and substring
    local len="${#text}"
    local prefix="${text:0:5}"
    local suffix="${text: -4}"

    echo "Length: $len, Prefix: $prefix, Suffix: $suffix"
}

# Use in command substitution
backup_files() {
    local pattern="${1:-*.txt}"
    local backup_dir="${2:-/tmp/backup}"

    # Create backup directory if it doesn't exist
    mkdir -p "$backup_dir"

    # Copy files with timestamp
    local timestamp="$(date +%Y%m%d_%H%M%S)"
    cp $pattern "$backup_dir/backup_$timestamp/"

    echo "Backed up files matching '$pattern' to '$backup_dir'"
}
"##;

        let mut parser = init_parser();
        let tree = parser.parse(bash_code, None).expect("Failed to parse code");
        let mut extractor = BashExtractor::new(
            "bash".to_string(),
            "expansion.sh".to_string(),
            bash_code.to_string(),
        );
        let symbols = extractor.extract_symbols(&tree);

        // Functions should be extracted
        let process_filename = symbols.iter().find(|s| s.name == "process_filename");
        assert!(process_filename.is_some());
        assert_eq!(process_filename.unwrap().kind, SymbolKind::Function);

        let test_expansions = symbols.iter().find(|s| s.name == "test_expansions");
        assert!(test_expansions.is_some());
        assert_eq!(test_expansions.unwrap().kind, SymbolKind::Function);

        let backup_files = symbols.iter().find(|s| s.name == "backup_files");
        assert!(backup_files.is_some());
        assert_eq!(backup_files.unwrap().kind, SymbolKind::Function);

        // Variables with complex expansions should be extracted
        let filename_var = symbols.iter().find(|s| s.name == "filename");
        assert!(filename_var.is_some());

        let base_var = symbols.iter().find(|s| s.name == "base");
        assert!(base_var.is_some());

        let extension_var = symbols.iter().find(|s| s.name == "extension");
        assert!(extension_var.is_some());

        let clean_name_var = symbols.iter().find(|s| s.name == "clean_name");
        assert!(clean_name_var.is_some());

        let upper_name_var = symbols.iter().find(|s| s.name == "upper_name");
        assert!(upper_name_var.is_some());

        let lower_name_var = symbols.iter().find(|s| s.name == "lower_name");
        assert!(lower_name_var.is_some());
    }

    #[test]
    fn test_extract_process_substitution() {
        let bash_code = r##"#!/bin/bash

# Process substitution examples
compare_files() {
    local file1=$1
    local file2=$2

    # Process substitution for diff input
    if diff <(sort "$file1") <(sort "$file2") > /dev/null; then
        echo "Files are identical when sorted"
    else
        echo "Files differ"
    fi
}

# Process substitution with output redirection
generate_report() {
    local output_file=$1

    # Redirect output of process substitution
    cat > "$output_file" < <(echo "Report generated at $(date)")
}

# Process substitution in command arguments
analyze_logs() {
    local log_dir=$1

    # Use process substitution as command input
    grep "ERROR" <(find "$log_dir" -name "*.log" -exec cat {} \;) |
        sort |
        uniq -c |
        sort -nr > error_summary.txt
}

# Process substitution with tee
backup_and_compress() {
    local source_dir=$1
    local backup_file=$2

    # Backup while showing progress
    tar -czf "$backup_file" "$source_dir" 2>&1 |
        tee >(grep "tar:" >&2) |
        grep -v "tar:" > /dev/null
}

# Advanced process substitution
merge_data() {
    local file1=$1
    local file2=$2

    # Merge sorted data from two sources
    sort -m <(sort "$file1") <(sort "$file2") |
        uniq > merged_data.txt
}

# Process substitution in loops
process_multiple_files() {
    local pattern=$1

    # Read from process substitution in while loop
    while IFS= read -r line; do
        echo "Processing: $line"
    done < <(find . -name "$pattern" -type f)
}
"##;

        let mut parser = init_parser();
        let tree = parser.parse(bash_code, None).expect("Failed to parse code");
        let mut extractor = BashExtractor::new(
            "bash".to_string(),
            "process.sh".to_string(),
            bash_code.to_string(),
        );
        let symbols = extractor.extract_symbols(&tree);

        // Functions should be extracted
        let compare_files = symbols.iter().find(|s| s.name == "compare_files");
        assert!(compare_files.is_some());
        assert_eq!(compare_files.unwrap().kind, SymbolKind::Function);

        let generate_report = symbols.iter().find(|s| s.name == "generate_report");
        assert!(generate_report.is_some());
        assert_eq!(generate_report.unwrap().kind, SymbolKind::Function);

        let analyze_logs = symbols.iter().find(|s| s.name == "analyze_logs");
        assert!(analyze_logs.is_some());
        assert_eq!(analyze_logs.unwrap().kind, SymbolKind::Function);

        let backup_and_compress = symbols.iter().find(|s| s.name == "backup_and_compress");
        assert!(backup_and_compress.is_some());
        assert_eq!(backup_and_compress.unwrap().kind, SymbolKind::Function);

        let merge_data = symbols.iter().find(|s| s.name == "merge_data");
        assert!(merge_data.is_some());
        assert_eq!(merge_data.unwrap().kind, SymbolKind::Function);

        let process_multiple_files = symbols.iter().find(|s| s.name == "process_multiple_files");
        assert!(process_multiple_files.is_some());
        assert_eq!(process_multiple_files.unwrap().kind, SymbolKind::Function);
    }

    #[test]
    fn test_extract_arrays_and_associative_arrays() {
        let bash_code = r###"#!/bin/bash

# Indexed arrays
declare -a fruits=("apple" "banana" "cherry")
fruits[3]="date"
fruits+=( "elderberry" )

# Simple associative arrays
declare -A colors
colors["red"]="red_value"
colors["green"]="green_value"
colors["blue"]="blue_value"

# Array operations
process_fruits() {
    local -a local_fruits=("$@")

    # Array length
    echo "Number of fruits: ${#local_fruits[@]}"

    # All elements
    echo "All fruits: ${local_fruits[*]}"

    # Specific indices
    echo "First fruit: ${local_fruits[0]}"
    echo "Last fruit: ${local_fruits[-1]}"

    # Slicing
    echo "First three: ${local_fruits[@]:0:3}"
    echo "From index 2: ${local_fruits[@]:2}"
}

# Associative array operations
manage_colors() {
    local -A color_map=("$@")

    # All keys
    echo "Available colors: ${!color_map[@]}"

    # All values
    echo "Color codes: ${color_map[@]}"

    # Specific key
    echo "Red color: ${color_map["red"]}"

    # Check if key exists
    if [[ -v color_map["purple"] ]]; then
        echo "Purple exists: ${color_map["purple"]}"
    else
        echo "Purple not found, adding it"
        color_map["purple"]="#800080"
    fi
}

# Array manipulation functions
array_utils() {
    local -a numbers=(1 2 3 4 5)

    # Append elements
    numbers+=(6 7 8)

    # Remove elements
    unset numbers[2]  # Remove element at index 2

    # Insert elements
    numbers=( "${numbers[@]:0:2}" 10 "${numbers[@]:2}" )

    # Reverse array
    local -a reversed=()
    for ((i=${#numbers[@]}-1; i>=0; i--)); do
        reversed+=("${numbers[i]}")
    done

    echo "Original: ${numbers[*]}"
    echo "Reversed: ${reversed[*]}"
}

# Multidimensional arrays (simulated with associative arrays)
matrix_operations() {
    local -A matrix=()

    # Set matrix values
    matrix["0,0"]=1
    matrix["0,1"]=2
    matrix["1,0"]=3
    matrix["1,1"]=4

    # Access matrix values
    echo "Matrix[0,0]: ${matrix["0,0"]}"
    echo "Matrix[1,1]: ${matrix["1,1"]}"

    # Iterate over matrix
    for key in "${!matrix[@]}"; do
        echo "Matrix[$key] = ${matrix[$key]}"
    done
}

# Array with command substitution
collect_files() {
    local -a script_files=()
    local -a config_files=()

    # Populate arrays with command output
    mapfile -t script_files < <(find . -name "*.sh" -type f)
    mapfile -t config_files < <(find . -name "*.conf" -type f)

    echo "Found ${#script_files[@]} scripts and ${#config_files[@]} configs"
}
"###;

        let mut parser = init_parser();
        let tree = parser.parse(bash_code, None).expect("Failed to parse code");
        let mut extractor = BashExtractor::new(
            "bash".to_string(),
            "arrays.sh".to_string(),
            bash_code.to_string(),
        );
        let symbols = extractor.extract_symbols(&tree);

        // Functions should be extracted
        let process_fruits = symbols.iter().find(|s| s.name == "process_fruits");
        assert!(process_fruits.is_some());
        assert_eq!(process_fruits.unwrap().kind, SymbolKind::Function);

        let manage_colors = symbols.iter().find(|s| s.name == "manage_colors");
        assert!(manage_colors.is_some());
        assert_eq!(manage_colors.unwrap().kind, SymbolKind::Function);

        let array_utils = symbols.iter().find(|s| s.name == "array_utils");
        assert!(array_utils.is_some());
        assert_eq!(array_utils.unwrap().kind, SymbolKind::Function);

        let matrix_operations = symbols.iter().find(|s| s.name == "matrix_operations");
        assert!(matrix_operations.is_some());
        assert_eq!(matrix_operations.unwrap().kind, SymbolKind::Function);

        let collect_files = symbols.iter().find(|s| s.name == "collect_files");
        assert!(collect_files.is_some());
        assert_eq!(collect_files.unwrap().kind, SymbolKind::Function);

        // Array variables and their elements should be extracted
        // The extractor captures array declarations and individual element accesses
        let fruits_elements = symbols
            .iter()
            .filter(|s| s.name.starts_with("fruits"))
            .count();
        assert!(
            fruits_elements >= 3,
            "Expected at least 3 fruits-related symbols, got {}",
            fruits_elements
        );

        let colors_elements = symbols.iter().filter(|s| s.name.contains("colors")).count();
        assert!(
            colors_elements >= 4,
            "Expected at least 4 colors-related symbols, got {}",
            colors_elements
        );

        let numbers_elements = symbols.iter().filter(|s| s.name == "numbers").count();
        assert!(
            numbers_elements >= 4,
            "Expected at least 4 numbers-related symbols, got {}",
            numbers_elements
        );

        let matrix_elements = symbols.iter().filter(|s| s.name.contains("matrix")).count();
        assert!(
            matrix_elements >= 6,
            "Expected at least 6 matrix-related symbols, got {}",
            matrix_elements
        );

        // Verify specific array element access patterns
        let red_color = symbols.iter().find(|s| s.name == "colors[\"red\"]");
        assert!(
            red_color.is_some(),
            "colors[\"red\"] element access not found"
        );

        let matrix_00 = symbols.iter().find(|s| s.name == "matrix[\"0,0\"]");
        assert!(
            matrix_00.is_some(),
            "matrix[\"0,0\"] element access not found"
        );
    }
}

mod doc_comments;
