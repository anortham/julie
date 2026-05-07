use crate::extractors::pipeline::extract_canonical;
use std::fs;
use std::path::{Path, PathBuf};

struct ExpectedSymbol {
    name: &'static str,
    kind: Option<&'static str>,
}

struct RealWorldCase {
    language: &'static str,
    path: &'static str,
    symbols: &'static [ExpectedSymbol],
    identifiers: &'static [&'static str],
}

macro_rules! sym {
    ($name:literal) => {
        ExpectedSymbol {
            name: $name,
            kind: None,
        }
    };
    ($name:literal, $kind:literal) => {
        ExpectedSymbol {
            name: $name,
            kind: Some($kind),
        }
    };
}

const CASES: &[RealWorldCase] = &[
    RealWorldCase {
        language: "bash",
        path: "fixtures/real-world/bash/system-admin-script.sh",
        symbols: &[
            sym!("get_system_info", "function"),
            sym!("mysql_operations", "function"),
        ],
        identifiers: &["hostname"],
    },
    RealWorldCase {
        language: "c",
        path: "fixtures/real-world/c/binary_search_tree.c",
        symbols: &[sym!("Node"), sym!("BST"), sym!("create_node", "function")],
        identifiers: &["malloc"],
    },
    RealWorldCase {
        language: "cpp",
        path: "fixtures/real-world/cpp/graph_algorithms.cpp",
        symbols: &[
            sym!("Graph", "class"),
            sym!("add_edge", "method"),
            sym!("dfs", "method"),
        ],
        identifiers: &["contains_vertex"],
    },
    RealWorldCase {
        language: "csharp",
        path: "fixtures/real-world/csharp/Program.cs",
        symbols: &[
            sym!("Program", "class"),
            sym!("Calculator", "class"),
            sym!("Add", "method"),
        ],
        identifiers: &["Add"],
    },
    RealWorldCase {
        language: "css",
        path: "fixtures/real-world/css/flexbox-grid.css",
        symbols: &[sym!(".container"), sym!(".row"), sym!(".col-xs-12")],
        identifiers: &[],
    },
    RealWorldCase {
        language: "dart",
        path: "fixtures/real-world/dart/flutter_isolate_demo.dart",
        symbols: &[
            sym!("main", "function"),
            sym!("IsolateExampleApp", "class"),
            sym!("HomePage", "class"),
        ],
        identifiers: &["runApp"],
    },
    RealWorldCase {
        language: "gdscript",
        path: "fixtures/real-world/gdscript/player_controller.gd",
        symbols: &[
            sym!("PlayerController", "class"),
            sym!("_ready", "method"),
            sym!("health_changed"),
        ],
        identifiers: &["_change_state"],
    },
    RealWorldCase {
        language: "go",
        path: "fixtures/real-world/go/main.go",
        symbols: &[
            sym!("main", "function"),
            sym!("Helper", "function"),
            sym!("DemoStruct", "struct"),
        ],
        identifiers: &["Helper"],
    },
    RealWorldCase {
        language: "html",
        path: "fixtures/real-world/html/popup-info-web-component.html",
        symbols: &[sym!("html"), sym!("head"), sym!("popup-info")],
        identifiers: &[],
    },
    RealWorldCase {
        language: "java",
        path: "fixtures/real-world/java/Main.java",
        symbols: &[
            sym!("Main", "class"),
            sym!("main", "method"),
            sym!("acceptModel", "method"),
        ],
        identifiers: &["printHello"],
    },
    RealWorldCase {
        language: "javascript",
        path: "fixtures/real-world/javascript/vue.config.js",
        symbols: &[
            sym!("devServer", "property"),
            sym!("chainWebpack", "function"),
        ],
        identifiers: &["resolve"],
    },
    RealWorldCase {
        language: "json",
        path: "fixtures/real-world/json/memories.jsonl",
        symbols: &[sym!("type"), sym!("content"), sym!("timestamp")],
        identifiers: &[],
    },
    RealWorldCase {
        language: "kotlin",
        path: "fixtures/real-world/kotlin/Main.kt",
        symbols: &[
            sym!("Main", "class"),
            sym!("main", "method"),
            sym!("acceptModel", "method"),
        ],
        identifiers: &["printHello"],
    },
    RealWorldCase {
        language: "lua",
        path: "fixtures/real-world/lua/web_server_framework.lua",
        symbols: &[
            sym!("HttpServer", "class"),
            sym!("new", "method"),
            sym!("parse_headers", "function"),
        ],
        identifiers: &["setmetatable"],
    },
    RealWorldCase {
        language: "php",
        path: "fixtures/real-world/php/index.php",
        symbols: &[
            sym!("greet", "function"),
            sym!("useHelperFunction", "function"),
        ],
        identifiers: &["helperFunction"],
    },
    RealWorldCase {
        language: "powershell",
        path: "fixtures/real-world/powershell/system-health-check.ps1",
        symbols: &[
            sym!("Write-HealthLog", "function"),
            sym!("Test-DiskSpace", "function"),
        ],
        identifiers: &["Write-Host"],
    },
    RealWorldCase {
        language: "python",
        path: "fixtures/real-world/python/test_database.py",
        symbols: &[
            sym!("temp_db", "function"),
            sym!("sample_entities", "function"),
            sym!("TestDatabaseInitialization", "class"),
        ],
        identifiers: &["MillerDatabase"],
    },
    RealWorldCase {
        language: "qml",
        path: "fixtures/qml/real-world/cool-retro-term-main.qml",
        symbols: &[
            sym!("terminalWindow"),
            sym!("showMenubarAction"),
            sym!("qtquickMenuLoader"),
        ],
        identifiers: &["appSettings"],
    },
    RealWorldCase {
        language: "r",
        path: "fixtures/r/real-world/ggplot2-geom-point.R",
        symbols: &[
            sym!("geom_point", "function"),
            sym!("GeomPoint"),
            sym!("translate_shape_string", "function"),
        ],
        identifiers: &["layer"],
    },
    RealWorldCase {
        language: "razor",
        path: "fixtures/real-world/razor/MainLayout.razor",
        symbols: &[
            sym!("@inherits", "import"),
            sym!("rendermode InteractiveWebAssembly", "property"),
        ],
        identifiers: &[],
    },
    RealWorldCase {
        language: "regex",
        path: "fixtures/real-world/regex/validation_patterns.regex",
        symbols: &[sym!("[A-Za-z0-9._%+-]", "class")],
        identifiers: &[],
    },
    RealWorldCase {
        language: "ruby",
        path: "fixtures/real-world/ruby/main.rb",
        symbols: &[
            sym!("DemoClass", "class"),
            sym!("initialize", "constructor"),
            sym!("helper_function", "method"),
        ],
        identifiers: &["Calculator"],
    },
    RealWorldCase {
        language: "rust",
        path: "fixtures/real-world/rust/lib.rs",
        symbols: &[sym!("add", "function"), sym!("multiply", "function")],
        identifiers: &[],
    },
    RealWorldCase {
        language: "sql",
        path: "fixtures/real-world/sql/postgresql-migrations.sql",
        symbols: &[sym!("apply_migration", "function")],
        identifiers: &[],
    },
    RealWorldCase {
        language: "swift",
        path: "fixtures/real-world/swift/main.swift",
        symbols: &[
            sym!("main", "function"),
            sym!("Calculator", "class"),
            sym!("User", "struct"),
        ],
        identifiers: &["Calculator"],
    },
    RealWorldCase {
        language: "typescript",
        path: "fixtures/real-world/typescript/user-service.ts",
        symbols: &[
            sym!("UserService", "class"),
            sym!("User", "interface"),
            sym!("validateEmail", "function"),
        ],
        identifiers: &["Logger"],
    },
    RealWorldCase {
        language: "tsx",
        path: "fixtures/real-world/typescript/user-dashboard.tsx",
        symbols: &[
            sym!("UserDashboard", "function"),
            sym!("UserDashboardProps", "interface"),
        ],
        identifiers: &["useState"],
    },
    RealWorldCase {
        language: "vue",
        path: "fixtures/real-world/vue/HelloWorld.vue",
        symbols: &[sym!("defineProps", "function"), sym!("HelloWorld", "class")],
        identifiers: &[],
    },
    RealWorldCase {
        language: "zig",
        path: "fixtures/real-world/zig/memory_allocator.zig",
        symbols: &[
            sym!("PoolAllocator", "function"),
            sym!("Self"),
            sym!("init", "method"),
        ],
        identifiers: &["Allocator"],
    },
];

#[test]
fn real_world_parser_upgrade_contracts_assert_expected_outputs() {
    let workspace_root = workspace_root();

    for case in CASES {
        let absolute_path = workspace_root.join(case.path);
        let content = fs::read_to_string(&absolute_path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", absolute_path.display()));
        let results = extract_canonical(case.path, &content, &workspace_root)
            .unwrap_or_else(|err| panic!("{} should extract cleanly: {err}", case.path));

        let symbol_dump = results
            .symbols
            .iter()
            .map(|symbol| format!("{}:{}", symbol.name, symbol.kind))
            .collect::<Vec<_>>()
            .join(", ");

        for expected in case.symbols {
            let found = results.symbols.iter().any(|symbol| {
                symbol.name == expected.name
                    && expected
                        .kind
                        .map(|kind| symbol.kind.to_string() == kind)
                        .unwrap_or(true)
            });
            assert!(
                found,
                "{} real-world {} fixture missing expected symbol {}{:?}; got [{}]",
                case.language, case.path, expected.name, expected.kind, symbol_dump
            );
        }

        let identifier_dump = results
            .identifiers
            .iter()
            .map(|identifier| identifier.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");

        for expected_identifier in case.identifiers {
            assert!(
                results
                    .identifiers
                    .iter()
                    .any(|identifier| identifier.name == *expected_identifier),
                "{} real-world {} fixture missing expected identifier {}; got [{}]",
                case.language,
                case.path,
                expected_identifier,
                identifier_dump
            );
        }
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}
