use tempfile::TempDir;

use crate::resolver::resolve_structured_batch;
use julie_core::Symbol;
use julie_core::database::{FileInfo, SymbolDatabase};
use julie_extractors::{
    Relationship, RelationshipKind, StructuredPendingRelationship, SymbolKind, UnresolvedTarget,
    Visibility,
};

fn detect_lang(path: &str) -> String {
    match std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
    {
        Some("cs") => "csharp",
        Some("java") => "java",
        Some("go") => "go",
        Some("py") => "python",
        Some("ts") => "typescript",
        Some("js") => "javascript",
        _ => "rust",
    }
    .to_string()
}

fn make_symbol(
    id: &str,
    name: &str,
    kind: SymbolKind,
    file_path: &str,
    parent_id: Option<&str>,
) -> Symbol {
    Symbol {
        extracted: julie_extractors::Symbol {
            id: id.to_string(),
            name: name.to_string(),
            kind,
            language: detect_lang(file_path),
            file_path: file_path.to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 10,
            end_column: 0,
            start_byte: 0,
            end_byte: 100,
            signature: None,
            doc_comment: None,
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(str::to_string),
            metadata: None,
            semantic_group: None,
            confidence: None,
            content_type: None,
            body_span: None,
            body_hash: None,
            annotations: Vec::new(),
        },
        code_context: None,
    }
}

fn make_file_info(path: &str) -> FileInfo {
    FileInfo {
        path: path.to_string(),
        language: detect_lang(path),
        hash: format!("hash_{path}"),
        size: 100,
        last_modified: 1,
        last_indexed: 1,
        symbol_count: 1,
        line_count: 10,
        content: None,
    }
}

#[test]
fn same_name_members_on_unrelated_classes_resolve_by_receiver() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let files = vec![
        make_file_info("src/a.rs"),
        make_file_info("src/b.rs"),
        make_file_info("src/caller.rs"),
    ];
    db.bulk_store_files(&files).unwrap();

    let symbols = vec![
        make_symbol("sym_class_a", "ClassA", SymbolKind::Class, "src/a.rs", None),
        make_symbol(
            "sym_render_a",
            "render",
            SymbolKind::Method,
            "src/a.rs",
            Some("sym_class_a"),
        ),
        make_symbol("sym_class_b", "ClassB", SymbolKind::Class, "src/b.rs", None),
        make_symbol(
            "sym_render_b",
            "render",
            SymbolKind::Method,
            "src/b.rs",
            Some("sym_class_b"),
        ),
    ];
    db.bulk_store_symbols(&symbols, "ws").unwrap();

    let pending_a = StructuredPendingRelationship {
        pending: julie_extractors::PendingRelationship {
            from_symbol_id: "caller_fn".to_string(),
            callee_name: "render".to_string(),
            kind: RelationshipKind::Calls,
            file_path: "src/caller.rs".to_string(),
            line_number: 5,
            confidence: 0.9,
        },
        target: UnresolvedTarget::simple("render".to_string()),
        caller_scope_symbol_id: None,
        span: None,
        reference_site_is_exact: false,
        receiver_type: Some("ClassA".to_string()),
    };

    let (resolved, stats) = resolve_structured_batch(&[pending_a], &db);
    assert_eq!(stats.resolved, 1);
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].to_symbol_id, "sym_render_a");
    assert!(!resolved[0].reference_site_is_exact);

    let pending_b = StructuredPendingRelationship {
        pending: julie_extractors::PendingRelationship {
            from_symbol_id: "caller_fn".to_string(),
            callee_name: "render".to_string(),
            kind: RelationshipKind::Calls,
            file_path: "src/caller.rs".to_string(),
            line_number: 6,
            confidence: 0.9,
        },
        target: UnresolvedTarget::simple("render".to_string()),
        caller_scope_symbol_id: None,
        span: None,
        reference_site_is_exact: false,
        receiver_type: Some("ClassB".to_string()),
    };

    let (resolved_b, stats_b) = resolve_structured_batch(&[pending_b], &db);
    assert_eq!(stats_b.resolved, 1);
    assert_eq!(resolved_b.len(), 1);
    assert_eq!(resolved_b[0].to_symbol_id, "sym_render_b");
}

#[test]
fn self_call_resolves_to_enclosing_class_member() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let files = vec![make_file_info("src/service.rs")];
    db.bulk_store_files(&files).unwrap();

    let symbols = vec![
        make_symbol(
            "sym_service",
            "Service",
            SymbolKind::Class,
            "src/service.rs",
            None,
        ),
        make_symbol(
            "sym_do_work",
            "do_work",
            SymbolKind::Method,
            "src/service.rs",
            Some("sym_service"),
        ),
        make_symbol(
            "sym_helper",
            "helper",
            SymbolKind::Method,
            "src/service.rs",
            Some("sym_service"),
        ),
    ];
    db.bulk_store_symbols(&symbols, "ws").unwrap();

    let pending = StructuredPendingRelationship {
        pending: julie_extractors::PendingRelationship {
            from_symbol_id: "sym_do_work".to_string(),
            callee_name: "helper".to_string(),
            kind: RelationshipKind::Calls,
            file_path: "src/service.rs".to_string(),
            line_number: 3,
            confidence: 0.95,
        },
        target: UnresolvedTarget::simple("helper".to_string()),
        caller_scope_symbol_id: Some("sym_do_work".to_string()),
        span: None,
        reference_site_is_exact: false,
        receiver_type: Some("self".to_string()),
    };

    let (resolved, stats) = resolve_structured_batch(&[pending], &db);
    assert_eq!(stats.resolved, 1);
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].to_symbol_id, "sym_helper");
}

#[test]
fn base_call_resolves_to_super_class_member() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let files = vec![make_file_info("src/hierarchy.rs")];
    db.bulk_store_files(&files).unwrap();

    let symbols = vec![
        make_symbol(
            "sym_base",
            "BaseClass",
            SymbolKind::Class,
            "src/hierarchy.rs",
            None,
        ),
        make_symbol(
            "sym_derived",
            "DerivedClass",
            SymbolKind::Class,
            "src/hierarchy.rs",
            None,
        ),
        make_symbol(
            "sym_base_init",
            "init",
            SymbolKind::Method,
            "src/hierarchy.rs",
            Some("sym_base"),
        ),
        make_symbol(
            "sym_derived_init",
            "init",
            SymbolKind::Method,
            "src/hierarchy.rs",
            Some("sym_derived"),
        ),
    ];
    db.bulk_store_symbols(&symbols, "ws").unwrap();

    let relationship = Relationship {
        id: "rel_extends".to_string(),
        from_symbol_id: "sym_derived".to_string(),
        to_symbol_id: "sym_base".to_string(),
        kind: RelationshipKind::Extends,
        file_path: "src/hierarchy.rs".to_string(),
        line_number: 1,
        span: None,
        reference_site_is_exact: true,
        confidence: 1.0,
        metadata: None,
    };
    db.store_relationships(&[relationship]).unwrap();

    let pending = StructuredPendingRelationship {
        pending: julie_extractors::PendingRelationship {
            from_symbol_id: "sym_derived_init".to_string(),
            callee_name: "init".to_string(),
            kind: RelationshipKind::Calls,
            file_path: "src/hierarchy.rs".to_string(),
            line_number: 5,
            confidence: 0.9,
        },
        target: UnresolvedTarget::simple("init".to_string()),
        caller_scope_symbol_id: Some("sym_derived_init".to_string()),
        span: None,
        reference_site_is_exact: false,
        receiver_type: Some("super".to_string()),
    };

    let (resolved, stats) = resolve_structured_batch(&[pending], &db);
    assert_eq!(stats.resolved, 1);
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].to_symbol_id, "sym_base_init");
}

#[test]
fn ambiguous_owners_across_namespaces_remains_unresolved() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let files = vec![
        make_file_info("src/ns1/mod.rs"),
        make_file_info("src/ns2/mod.rs"),
        make_file_info("src/other/caller.rs"),
    ];
    db.bulk_store_files(&files).unwrap();

    let symbols = vec![
        make_symbol(
            "sym_c1",
            "Client",
            SymbolKind::Class,
            "src/ns1/mod.rs",
            None,
        ),
        make_symbol(
            "sym_connect1",
            "connect",
            SymbolKind::Method,
            "src/ns1/mod.rs",
            Some("sym_c1"),
        ),
        make_symbol(
            "sym_c2",
            "Client",
            SymbolKind::Class,
            "src/ns2/mod.rs",
            None,
        ),
        make_symbol(
            "sym_connect2",
            "connect",
            SymbolKind::Method,
            "src/ns2/mod.rs",
            Some("sym_c2"),
        ),
    ];
    db.bulk_store_symbols(&symbols, "ws").unwrap();

    let pending = StructuredPendingRelationship {
        pending: julie_extractors::PendingRelationship {
            from_symbol_id: "caller_fn".to_string(),
            callee_name: "connect".to_string(),
            kind: RelationshipKind::Calls,
            file_path: "src/other/caller.rs".to_string(),
            line_number: 10,
            confidence: 0.8,
        },
        target: UnresolvedTarget::simple("connect".to_string()),
        caller_scope_symbol_id: None,
        span: None,
        reference_site_is_exact: false,
        receiver_type: Some("Client".to_string()),
    };

    let (resolved, stats) = resolve_structured_batch(&[pending], &db);
    assert_eq!(stats.resolved, 0);
    assert_eq!(resolved.len(), 0);
}

#[test]
fn absent_receiver_hint_falls_back_to_unrestricted_scoring() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let files = vec![
        make_file_info("src/mod.rs"),
        make_file_info("src/caller.rs"),
    ];
    db.bulk_store_files(&files).unwrap();

    let symbols = vec![make_symbol(
        "sym_foo",
        "foo",
        SymbolKind::Function,
        "src/mod.rs",
        None,
    )];
    db.bulk_store_symbols(&symbols, "ws").unwrap();

    let pending = StructuredPendingRelationship {
        pending: julie_extractors::PendingRelationship {
            from_symbol_id: "caller_fn".to_string(),
            callee_name: "foo".to_string(),
            kind: RelationshipKind::Calls,
            file_path: "src/caller.rs".to_string(),
            line_number: 3,
            confidence: 0.8,
        },
        target: UnresolvedTarget::simple("foo".to_string()),
        caller_scope_symbol_id: None,
        span: None,
        reference_site_is_exact: false,
        receiver_type: None,
    };

    let (resolved, stats) = resolve_structured_batch(&[pending], &db);
    assert_eq!(stats.resolved, 1);
    assert_eq!(resolved[0].to_symbol_id, "sym_foo");
    assert!(!resolved[0].reference_site_is_exact);
}

#[test]
fn multi_class_disambiguation_and_mismatched_receiver_rejection() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let files = vec![
        make_file_info("src/orders.rs"),
        make_file_info("src/payments.rs"),
        make_file_info("src/notifications.rs"),
        make_file_info("src/audit.rs"),
        make_file_info("src/caller.rs"),
    ];
    db.bulk_store_files(&files).unwrap();

    let symbols = vec![
        make_symbol(
            "sym_order_proc",
            "OrderProcessor",
            SymbolKind::Class,
            "src/orders.rs",
            None,
        ),
        make_symbol(
            "sym_order_process",
            "process",
            SymbolKind::Method,
            "src/orders.rs",
            Some("sym_order_proc"),
        ),
        make_symbol(
            "sym_payment_proc",
            "PaymentProcessor",
            SymbolKind::Class,
            "src/payments.rs",
            None,
        ),
        make_symbol(
            "sym_payment_process",
            "process",
            SymbolKind::Method,
            "src/payments.rs",
            Some("sym_payment_proc"),
        ),
        make_symbol(
            "sym_notif_proc",
            "NotificationProcessor",
            SymbolKind::Class,
            "src/notifications.rs",
            None,
        ),
        make_symbol(
            "sym_notif_process",
            "process",
            SymbolKind::Method,
            "src/notifications.rs",
            Some("sym_notif_proc"),
        ),
        make_symbol(
            "sym_audit_logger",
            "AuditLogger",
            SymbolKind::Class,
            "src/audit.rs",
            None,
        ),
        make_symbol(
            "sym_audit_log",
            "log",
            SymbolKind::Method,
            "src/audit.rs",
            Some("sym_audit_logger"),
        ),
    ];
    db.bulk_store_symbols(&symbols, "ws").unwrap();

    let make_call = |receiver: Option<&str>| StructuredPendingRelationship {
        pending: julie_extractors::PendingRelationship {
            from_symbol_id: "caller_fn".to_string(),
            callee_name: "process".to_string(),
            kind: RelationshipKind::Calls,
            file_path: "src/caller.rs".to_string(),
            line_number: 10,
            confidence: 0.9,
        },
        target: UnresolvedTarget::simple("process".to_string()),
        caller_scope_symbol_id: None,
        span: None,
        reference_site_is_exact: false,
        receiver_type: receiver.map(str::to_string),
    };

    let (res_pay, stat_pay) = resolve_structured_batch(&[make_call(Some("PaymentProcessor"))], &db);
    assert_eq!(stat_pay.resolved, 1);
    assert_eq!(res_pay[0].to_symbol_id, "sym_payment_process");
    assert!(!res_pay[0].reference_site_is_exact);

    let (res_ord, stat_ord) = resolve_structured_batch(&[make_call(Some("OrderProcessor"))], &db);
    assert_eq!(stat_ord.resolved, 1);
    assert_eq!(res_ord[0].to_symbol_id, "sym_order_process");
    assert!(!res_ord[0].reference_site_is_exact);

    let (res_not, stat_not) =
        resolve_structured_batch(&[make_call(Some("NotificationProcessor"))], &db);
    assert_eq!(stat_not.resolved, 1);
    assert_eq!(res_not[0].to_symbol_id, "sym_notif_process");
    assert!(!res_not[0].reference_site_is_exact);

    let (res_audit, stat_audit) = resolve_structured_batch(&[make_call(Some("AuditLogger"))], &db);
    assert_eq!(stat_audit.resolved, 0);
    assert!(res_audit.is_empty());

    let (res_unknown, stat_unknown) =
        resolve_structured_batch(&[make_call(Some("NonExistentProcessor"))], &db);
    assert_eq!(stat_unknown.resolved, 0);
    assert!(res_unknown.is_empty());

    let (res_none, stat_none) = resolve_structured_batch(&[make_call(None)], &db);
    assert_eq!(stat_none.resolved, 1);
    assert!(!res_none[0].reference_site_is_exact);
}

#[test]
fn qualified_namespace_disambiguation_by_receiver() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let files = vec![
        make_file_info("src/services/order/mod.rs"),
        make_file_info("src/services/payment/mod.rs"),
        make_file_info("src/caller.rs"),
    ];
    db.bulk_store_files(&files).unwrap();

    let symbols = vec![
        make_symbol(
            "sym_order_p",
            "Processor",
            SymbolKind::Class,
            "src/services/order/mod.rs",
            None,
        ),
        make_symbol(
            "sym_order_handle",
            "handle",
            SymbolKind::Method,
            "src/services/order/mod.rs",
            Some("sym_order_p"),
        ),
        make_symbol(
            "sym_pay_p",
            "Processor",
            SymbolKind::Class,
            "src/services/payment/mod.rs",
            None,
        ),
        make_symbol(
            "sym_pay_handle",
            "handle",
            SymbolKind::Method,
            "src/services/payment/mod.rs",
            Some("sym_pay_p"),
        ),
    ];
    db.bulk_store_symbols(&symbols, "ws").unwrap();

    let pending_order = StructuredPendingRelationship {
        pending: julie_extractors::PendingRelationship {
            from_symbol_id: "caller_fn".to_string(),
            callee_name: "handle".to_string(),
            kind: RelationshipKind::Calls,
            file_path: "src/caller.rs".to_string(),
            line_number: 12,
            confidence: 0.9,
        },
        target: UnresolvedTarget::simple("handle".to_string()),
        caller_scope_symbol_id: None,
        span: None,
        reference_site_is_exact: false,
        receiver_type: Some("order.Processor".to_string()),
    };

    let (res_order, stat_order) = resolve_structured_batch(&[pending_order], &db);
    assert_eq!(stat_order.resolved, 1);
    assert_eq!(res_order[0].to_symbol_id, "sym_order_handle");
    assert!(!res_order[0].reference_site_is_exact);

    let pending_pay = StructuredPendingRelationship {
        pending: julie_extractors::PendingRelationship {
            from_symbol_id: "caller_fn".to_string(),
            callee_name: "handle".to_string(),
            kind: RelationshipKind::Calls,
            file_path: "src/caller.rs".to_string(),
            line_number: 14,
            confidence: 0.9,
        },
        target: UnresolvedTarget::simple("handle".to_string()),
        caller_scope_symbol_id: None,
        span: None,
        reference_site_is_exact: false,
        receiver_type: Some("payment.Processor".to_string()),
    };

    let (res_pay, stat_pay) = resolve_structured_batch(&[pending_pay], &db);
    assert_eq!(stat_pay.resolved, 1);
    assert_eq!(res_pay[0].to_symbol_id, "sym_pay_handle");
    assert!(!res_pay[0].reference_site_is_exact);
}

#[test]
fn inherited_method_resolution_via_ancestor_traversal() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let files = vec![
        make_file_info("src/worker.rs"),
        make_file_info("src/email.rs"),
        make_file_info("src/caller.rs"),
    ];
    db.bulk_store_files(&files).unwrap();

    let symbols = vec![
        make_symbol(
            "sym_base_worker",
            "BaseWorker",
            SymbolKind::Class,
            "src/worker.rs",
            None,
        ),
        make_symbol(
            "sym_base_start",
            "start",
            SymbolKind::Method,
            "src/worker.rs",
            Some("sym_base_worker"),
        ),
        make_symbol(
            "sym_email_worker",
            "EmailWorker",
            SymbolKind::Class,
            "src/email.rs",
            None,
        ),
    ];
    db.bulk_store_symbols(&symbols, "ws").unwrap();

    let rel_extends = Relationship {
        id: "rel_email_extends_worker".to_string(),
        from_symbol_id: "sym_email_worker".to_string(),
        to_symbol_id: "sym_base_worker".to_string(),
        kind: RelationshipKind::Extends,
        file_path: "src/email.rs".to_string(),
        line_number: 1,
        span: None,
        reference_site_is_exact: true,
        confidence: 1.0,
        metadata: None,
    };
    db.store_relationships(&[rel_extends]).unwrap();

    let pending = StructuredPendingRelationship {
        pending: julie_extractors::PendingRelationship {
            from_symbol_id: "caller_fn".to_string(),
            callee_name: "start".to_string(),
            kind: RelationshipKind::Calls,
            file_path: "src/caller.rs".to_string(),
            line_number: 20,
            confidence: 0.9,
        },
        target: UnresolvedTarget::simple("start".to_string()),
        caller_scope_symbol_id: None,
        span: None,
        reference_site_is_exact: false,
        receiver_type: Some("EmailWorker".to_string()),
    };

    let (resolved, stats) = resolve_structured_batch(&[pending], &db);
    assert_eq!(stats.resolved, 1);
    assert_eq!(resolved[0].to_symbol_id, "sym_base_start");
    assert!(!resolved[0].reference_site_is_exact);
}

#[test]
fn ambiguous_multiple_inheritance_diamond_remains_unresolved() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let files = vec![
        make_file_info("src/device.rs"),
        make_file_info("src/caller.rs"),
    ];
    db.bulk_store_files(&files).unwrap();

    let symbols = vec![
        make_symbol(
            "sym_alpha",
            "InterfaceAlpha",
            SymbolKind::Interface,
            "src/device.rs",
            None,
        ),
        make_symbol(
            "sym_alpha_calc",
            "calculate",
            SymbolKind::Method,
            "src/device.rs",
            Some("sym_alpha"),
        ),
        make_symbol(
            "sym_beta",
            "InterfaceBeta",
            SymbolKind::Interface,
            "src/device.rs",
            None,
        ),
        make_symbol(
            "sym_beta_calc",
            "calculate",
            SymbolKind::Method,
            "src/device.rs",
            Some("sym_beta"),
        ),
        make_symbol(
            "sym_multi",
            "MultiDevice",
            SymbolKind::Class,
            "src/device.rs",
            None,
        ),
    ];
    db.bulk_store_symbols(&symbols, "ws").unwrap();

    let rel_alpha = Relationship {
        id: "rel_implements_alpha".to_string(),
        from_symbol_id: "sym_multi".to_string(),
        to_symbol_id: "sym_alpha".to_string(),
        kind: RelationshipKind::Implements,
        file_path: "src/device.rs".to_string(),
        line_number: 10,
        span: None,
        reference_site_is_exact: true,
        confidence: 1.0,
        metadata: None,
    };
    let rel_beta = Relationship {
        id: "rel_implements_beta".to_string(),
        from_symbol_id: "sym_multi".to_string(),
        to_symbol_id: "sym_beta".to_string(),
        kind: RelationshipKind::Implements,
        file_path: "src/device.rs".to_string(),
        line_number: 11,
        span: None,
        reference_site_is_exact: true,
        confidence: 1.0,
        metadata: None,
    };
    db.store_relationships(&[rel_alpha, rel_beta]).unwrap();

    let pending = StructuredPendingRelationship {
        pending: julie_extractors::PendingRelationship {
            from_symbol_id: "caller_fn".to_string(),
            callee_name: "calculate".to_string(),
            kind: RelationshipKind::Calls,
            file_path: "src/caller.rs".to_string(),
            line_number: 30,
            confidence: 0.9,
        },
        target: UnresolvedTarget::simple("calculate".to_string()),
        caller_scope_symbol_id: None,
        span: None,
        reference_site_is_exact: false,
        receiver_type: Some("MultiDevice".to_string()),
    };

    let (resolved, stats) = resolve_structured_batch(&[pending], &db);
    assert_eq!(stats.resolved, 0);
    assert!(resolved.is_empty());
}

#[test]
fn directory_name_does_not_override_contradictory_namespace_facts() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let files = vec![
        make_file_info("Expected/Client.cs"),
        make_file_info("Caller.cs"),
    ];
    db.bulk_store_files(&files).unwrap();

    let mut sym_ns = make_symbol(
        "sym_ns_other",
        "Other",
        SymbolKind::Namespace,
        "Expected/Client.cs",
        None,
    );
    sym_ns.extracted.language = "csharp".to_string();

    let mut sym_client = make_symbol(
        "sym_client",
        "Client",
        SymbolKind::Class,
        "Expected/Client.cs",
        Some("sym_ns_other"),
    );
    sym_client.extracted.language = "csharp".to_string();

    let mut sym_connect = make_symbol(
        "sym_connect",
        "Connect",
        SymbolKind::Method,
        "Expected/Client.cs",
        Some("sym_client"),
    );
    sym_connect.extracted.language = "csharp".to_string();

    db.bulk_store_symbols(&[sym_ns, sym_client, sym_connect], "ws")
        .unwrap();

    let pending = StructuredPendingRelationship {
        pending: julie_extractors::PendingRelationship {
            from_symbol_id: "caller_fn".to_string(),
            callee_name: "Connect".to_string(),
            kind: RelationshipKind::Calls,
            file_path: "Caller.cs".to_string(),
            line_number: 10,
            confidence: 0.9,
        },
        target: UnresolvedTarget::simple("Connect".to_string()),
        caller_scope_symbol_id: None,
        span: None,
        reference_site_is_exact: false,
        receiver_type: Some("Expected.Client".to_string()),
    };

    let (resolved, stats) = resolve_structured_batch(&[pending], &db);
    assert_eq!(stats.resolved, 0);
    assert!(resolved.is_empty());
}
