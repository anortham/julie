use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn handler_construction_sets_workspace_root() -> Result<()> {
    let handler = JulieServerHandler::new_for_test().await?;
    let handler_root = handler
        .current_workspace_root()
        .canonicalize()
        .unwrap_or_else(|_| handler.current_workspace_root());
    let cwd = std::env::current_dir()?
        .canonicalize()
        .unwrap_or_else(|_| std::env::current_dir().unwrap());
    let temp_root = std::env::temp_dir()
        .canonicalize()
        .unwrap_or_else(|_| std::env::temp_dir());

    assert!(
        handler_root.starts_with(&temp_root),
        "new_for_test should use isolated temp storage, got {}",
        handler_root.display()
    );
    assert_ne!(
        handler_root, cwd,
        "new_for_test should not anchor handlers in the repo cwd"
    );
    assert_eq!(handler.current_workspace_id(), None);
    // workspace should start as None (lazy init)
    let ws = handler.workspace.read().await;
    assert!(
        ws.is_none(),
        "workspace should be None before initialization"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tool_list_matches_public_surface() -> Result<()> {
    let handler = JulieServerHandler::new_for_test().await?;

    let (server_transport, client_transport) = tokio::io::duplex(64);
    drop(client_transport);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);

    let tools = <JulieServerHandler as ServerHandler>::list_tools(
        &handler,
        Some(PaginatedRequestParams::default()),
        RequestContext::new(NumberOrString::Number(1), service.peer().clone()),
    )
    .await?;

    assert!(
        tools
            .tools
            .iter()
            .all(|tool| tool.name.as_ref() != "query_metrics"),
        "query_metrics should not appear in the public tool list"
    );
    assert!(
        tools
            .tools
            .iter()
            .all(|tool| tool.name.as_ref() != "edit_symbol"),
        "edit_symbol should not appear in the public tool list"
    );
    assert!(
        tools
            .tools
            .iter()
            .any(|tool| tool.name.as_ref() == "rewrite_symbol"),
        "rewrite_symbol should appear in the public tool list"
    );
    assert!(
        tools
            .tools
            .iter()
            .any(|tool| tool.name.as_ref() == "call_path"),
        "call_path should appear in the public tool list"
    );
    assert!(
        tools
            .tools
            .iter()
            .any(|tool| tool.name.as_ref() == "blast_radius"),
        "blast_radius should appear in the public tool list"
    );
    assert!(
        tools
            .tools
            .iter()
            .any(|tool| tool.name.as_ref() == "spillover_get"),
        "spillover_get should appear in the public tool list"
    );

    let _ = service.cancel().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_fast_search_public_docs_describe_file_mode() -> Result<()> {
    let handler = JulieServerHandler::new_for_test().await?;

    let (server_transport, client_transport) = tokio::io::duplex(64);
    drop(client_transport);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);

    let tools = <JulieServerHandler as ServerHandler>::list_tools(
        &handler,
        Some(PaginatedRequestParams::default()),
        RequestContext::new(NumberOrString::Number(1), service.peer().clone()),
    )
    .await?;

    let fast_search = tools
        .tools
        .iter()
        .find(|tool| tool.name.as_ref() == "fast_search")
        .expect("fast_search should appear in the public tool list");

    let description = fast_search
        .description
        .as_deref()
        .expect("fast_search should publish a tool description");
    // After T8, search_target is removed from the public surface.
    // Verify the tool still publishes a non-empty description.
    assert!(
        !description.is_empty(),
        "fast_search should publish a non-empty description"
    );
    // Verify the description covers unified search (the T8 replacement).
    assert!(
        description.contains("unified") || description.contains("search"),
        "tool description should describe the search capability, got: {description}"
    );

    let properties = fast_search
        .input_schema
        .get("properties")
        .and_then(Value::as_object)
        .expect("fast_search input schema should expose properties");

    // After T8, search_target is no longer a public parameter.
    assert!(
        !properties.contains_key("search_target"),
        "search_target should NOT appear in the public schema after T8 cutover"
    );

    // context_lines and return_format still exist.
    assert!(
        properties.contains_key("context_lines"),
        "context_lines should still be a public parameter"
    );
    assert!(
        properties.contains_key("return_format"),
        "return_format should still be a public parameter"
    );

    let _ = service.cancel().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_edit_file_public_surface_marks_apply_destructive_and_occurrence_finite() -> Result<()>
{
    let handler = JulieServerHandler::new_for_test().await?;

    let (server_transport, client_transport) = tokio::io::duplex(64);
    drop(client_transport);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);

    let tools = <JulieServerHandler as ServerHandler>::list_tools(
        &handler,
        Some(PaginatedRequestParams::default()),
        RequestContext::new(NumberOrString::Number(1), service.peer().clone()),
    )
    .await?;

    let edit_file = tools
        .tools
        .iter()
        .find(|tool| tool.name.as_ref() == "edit_file")
        .expect("edit_file should appear in the public tool list");
    let annotations = edit_file
        .annotations
        .as_ref()
        .expect("edit_file should publish annotations");
    assert_eq!(
        annotations.destructive_hint,
        Some(true),
        "edit_file can write to disk when dry_run=false"
    );

    let root_schema = Value::Object((*edit_file.input_schema).clone());
    let properties = root_schema
        .get("properties")
        .and_then(Value::as_object)
        .expect("edit_file input schema should expose properties");
    let occurrence_schema = properties
        .get("occurrence")
        .expect("edit_file schema should include occurrence");
    let mut values = Vec::new();
    collect_schema_enum_strings(&root_schema, occurrence_schema, &mut values);
    values.sort();
    values.dedup();
    assert_eq!(
        values,
        vec!["all".to_string(), "first".to_string(), "last".to_string()],
        "occurrence should be a finite enum in the published schema"
    );

    let _ = service.cancel().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_manage_workspace_public_surface_is_marked_destructive() -> Result<()> {
    let handler = JulieServerHandler::new_for_test().await?;

    let (server_transport, client_transport) = tokio::io::duplex(64);
    drop(client_transport);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);

    let tools = <JulieServerHandler as ServerHandler>::list_tools(
        &handler,
        Some(PaginatedRequestParams::default()),
        RequestContext::new(NumberOrString::Number(1), service.peer().clone()),
    )
    .await?;

    let manage_workspace = tools
        .tools
        .iter()
        .find(|tool| tool.name.as_ref() == "manage_workspace")
        .expect("manage_workspace should appear in the public tool list");
    let annotations = manage_workspace
        .annotations
        .as_ref()
        .expect("manage_workspace should publish annotations");
    assert_eq!(
        annotations.destructive_hint,
        Some(true),
        "manage_workspace exposes remove, clean, and force reindex operations"
    );

    let _ = service.cancel().await;
    Ok(())
}
