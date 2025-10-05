// Bash Extractor Tests (ported from Miller's bash-extractor.test.ts)
// Following TDD methodology: RED -> GREEN -> REFACTOR -> ENHANCE

#[cfg(test)]
mod bash_extractor_tests {
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
        assert_eq!(
            python_cmd.doc_comment,
            Some("[Python 3 Interpreter Call]".to_string())
        );

        let node_cmd = commands.iter().find(|c| c.name == "node");
        assert!(node_cmd.is_some(), "node command not found");
        let node_cmd = node_cmd.unwrap();
        assert_eq!(
            node_cmd.doc_comment,
            Some("[Node.js Runtime Call]".to_string())
        );

        let docker_cmd = commands.iter().find(|c| c.name == "docker");
        assert!(docker_cmd.is_some(), "docker command not found");
        let docker_cmd = docker_cmd.unwrap();
        assert_eq!(
            docker_cmd.doc_comment,
            Some("[Docker Container Call]".to_string())
        );

        let kubectl_cmd = commands.iter().find(|c| c.name == "kubectl");
        assert!(kubectl_cmd.is_some(), "kubectl command not found");
        let kubectl_cmd = kubectl_cmd.unwrap();
        assert_eq!(
            kubectl_cmd.doc_comment,
            Some("[Kubernetes CLI Call]".to_string())
        );

        let terraform_cmd = commands.iter().find(|c| c.name == "terraform");
        assert!(terraform_cmd.is_some(), "terraform command not found");
        let terraform_cmd = terraform_cmd.unwrap();
        assert_eq!(
            terraform_cmd.doc_comment,
            Some("[Infrastructure as Code Call]".to_string())
        );

        let bun_cmd = commands.iter().find(|c| c.name == "bun");
        assert!(bun_cmd.is_some(), "bun command not found");
        let bun_cmd = bun_cmd.unwrap();
        assert_eq!(bun_cmd.doc_comment, Some("[Bun Runtime Call]".to_string()));
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
        assert!(docker_host
            .doc_comment
            .as_ref()
            .unwrap()
            .contains("Environment Variable"));

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
        assert_eq!(if_block.doc_comment, Some("[IF control flow]".to_string()));

        let for_block = control_flow.iter().find(|c| c.name.contains("for block"));
        assert!(for_block.is_some(), "for block not found");
        let for_block = for_block.unwrap();
        assert_eq!(
            for_block.doc_comment,
            Some("[FOR control flow]".to_string())
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
        assert_eq!(version_var.doc_comment, Some("[READONLY]".to_string()));

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
    use crate::extractors::base::IdentifierKind;
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
}
