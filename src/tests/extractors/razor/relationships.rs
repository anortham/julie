use super::{extract_relationships, extract_symbols};
use crate::extractors::base::RelationshipKind;

#[test]
fn test_method_invocation_relationship() {
    let razor_code = r#"
@page "/invocations"

@code {
    private string GetGreeting() => "Hello";
    private string RenderGreeting() => GetGreeting();
}
"#;

    let symbols = extract_symbols(razor_code);
    let relationships = extract_relationships(razor_code, &symbols);

    let get_greeting = symbols
        .iter()
        .find(|symbol| symbol.name == "GetGreeting")
        .expect("missing GetGreeting symbol");

    let render_greeting = symbols
        .iter()
        .find(|symbol| symbol.name == "RenderGreeting")
        .expect("missing RenderGreeting symbol");

    assert!(
        relationships.iter().any(|relationship| {
            relationship.kind == RelationshipKind::Calls
                && relationship.from_symbol_id == render_greeting.id
                && relationship.to_symbol_id == get_greeting.id
        }),
        "Expected RenderGreeting -> GetGreeting Calls relationship, got {:?}",
        relationships
    );
}

#[test]
fn test_component_identifier_relationship() {
    let razor_code = r#"
@page "/component-identifier"

<AlertBanner />

@code {
    private Type BannerType => typeof(AlertBanner);
}
"#;

    let symbols = extract_symbols(razor_code);
    let relationships = extract_relationships(razor_code, &symbols);

    let component_symbol = symbols
        .iter()
        .find(|symbol| symbol.name == "AlertBanner")
        .expect("missing AlertBanner component symbol");

    let banner_type_symbol = symbols
        .iter()
        .find(|symbol| symbol.name == "BannerType")
        .expect("missing BannerType symbol");

    assert!(
        relationships.iter().any(|relationship| {
            relationship.kind == RelationshipKind::Uses
                && relationship.from_symbol_id == banner_type_symbol.id
                && relationship.to_symbol_id == component_symbol.id
        }),
        "Expected BannerType -> AlertBanner Uses relationship, got {:?}",
        relationships
    );
}

#[test]
fn test_injected_service_identifier_does_not_create_component_relationship() {
    let razor_code = r#"
@page "/logging"
@inject ILogger<LoggingPage> Logger

@code {
    private void LogMessage()
    {
        Logger.LogInformation("Testing");
    }
}
"#;

    let symbols = extract_symbols(razor_code);
    let relationships = extract_relationships(razor_code, &symbols);

    assert!(
        relationships.iter().all(|relationship| {
            relationship
                .metadata
                .as_ref()
                .and_then(|meta| meta.get("component"))
                .and_then(|value| value.as_str())
                != Some("Logger")
        }),
        "Injected service should not be treated as component: {:?}",
        relationships
    );
}

#[test]
fn test_component_invoke_async_relationship_targets_component_symbol() {
    let razor_code = r#"
@page "/invoke-component"

<FeaturedProducts />

@code {
    private async Task RenderProducts()
    {
        await Component.InvokeAsync("FeaturedProducts", new { count = 6 });
    }
}
"#;

    let symbols = extract_symbols(razor_code);
    let relationships = extract_relationships(razor_code, &symbols);

    let render_products = symbols
        .iter()
        .find(|symbol| symbol.name == "RenderProducts")
        .expect("missing RenderProducts symbol");

    let featured_products = symbols
        .iter()
        .find(|symbol| symbol.name == "FeaturedProducts")
        .expect("missing FeaturedProducts component symbol");

    assert!(
        relationships.iter().any(|relationship| {
            relationship.kind == RelationshipKind::Calls
                && relationship.from_symbol_id == render_products.id
                && relationship.to_symbol_id == featured_products.id
        }),
        "Expected RenderProducts -> FeaturedProducts Calls relationship, got {:?}",
        relationships
    );
}
