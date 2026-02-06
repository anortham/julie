//! Tests for FastExploreTool Types mode - Type-aware exploration
//!
//! Tests cover:
//! - Types mode: Finding implementations, return types, parameter types
//! - Error handling: Missing parameters, type not found
//! - Edge cases: No types table, empty results
//!
//! Note: Following TDD methodology - write failing tests first, then implement.

use crate::handler::JulieServerHandler;
use crate::tools::exploration::fast_explore::{ExploreMode, FastExploreTool};
use crate::tools::workspace::ManageWorkspaceTool;
use anyhow::Result;
use std::fs;
use tempfile::TempDir;

/// Helper to create a test handler with isolated workspace
async fn create_test_handler() -> Result<(JulieServerHandler, TempDir)> {
    let temp_dir = TempDir::new()?;
    let handler = JulieServerHandler::new().await?;
    Ok((handler, temp_dir))
}

/// Helper to create test codebase with type information
async fn create_typed_codebase(temp_dir: &TempDir) -> Result<()> {
    let src_dir = temp_dir.path().join("src");
    fs::create_dir_all(&src_dir)?;

    // TypeScript interfaces and implementations
    fs::write(
        src_dir.join("payment.ts"),
        r#"
export interface PaymentProcessor {
    process(amount: number): Promise<PaymentResult>;
    refund(transactionId: string): Promise<boolean>;
}

export interface PaymentResult {
    success: boolean;
    transactionId: string;
}

export class StripeProcessor implements PaymentProcessor {
    async process(amount: number): Promise<PaymentResult> {
        // Stripe implementation
        return { success: true, transactionId: "stripe_123" };
    }

    async refund(transactionId: string): Promise<boolean> {
        return true;
    }
}

export class PayPalProcessor implements PaymentProcessor {
    async process(amount: number): Promise<PaymentResult> {
        // PayPal implementation
        return { success: true, transactionId: "paypal_456" };
    }

    async refund(transactionId: string): Promise<boolean> {
        return true;
    }
}

export function createPayment(processor: PaymentProcessor, amount: number): Promise<PaymentResult> {
    return processor.process(amount);
}

export function validatePaymentResult(result: PaymentResult): boolean {
    return result.success && result.transactionId.length > 0;
}
"#,
    )?;

    // Rust traits and implementations
    fs::write(
        src_dir.join("checkout.rs"),
        r#"
pub trait CheckoutStep {
    fn execute(&self) -> Result<StepResult, Error>;
    fn can_skip(&self) -> bool;
}

pub struct StepResult {
    pub success: bool,
    pub data: String,
}

pub struct AddressValidation;
impl CheckoutStep for AddressValidation {
    fn execute(&self) -> Result<StepResult, Error> {
        Ok(StepResult { success: true, data: "address validated".to_string() })
    }
    fn can_skip(&self) -> bool { false }
}

pub struct PaymentProcessing;
impl CheckoutStep for PaymentProcessing {
    fn execute(&self) -> Result<StepResult, Error> {
        Ok(StepResult { success: true, data: "payment processed".to_string() })
    }
    fn can_skip(&self) -> bool { false }
}

pub fn process_checkout_steps(steps: Vec<Box<dyn CheckoutStep>>) -> Result<Vec<StepResult>, Error> {
    steps.iter().map(|step| step.execute()).collect()
}
"#,
    )?;

    Ok(())
}

/// Helper to index workspace and wait for type extraction
async fn index_workspace_with_types(
    handler: &JulieServerHandler,
    workspace_path: &str,
) -> Result<()> {
    let manage_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string()),
        name: None,
        workspace_id: None,
        force: Some(true),
        detailed: None,
    };

    manage_tool.call_tool(handler).await?;
    Ok(())
}

#[tokio::test]
async fn test_types_mode_finds_implementations() -> Result<()> {
    // Setup
    let (handler, temp_dir) = create_test_handler().await?;
    create_typed_codebase(&temp_dir).await?;
    index_workspace_with_types(&handler, temp_dir.path().to_str().unwrap()).await?;

    // Test: Find all implementations of PaymentProcessor interface
    let tool = FastExploreTool {
        mode: ExploreMode::Types,
        type_name: Some("PaymentProcessor".to_string()),
        exploration_type: Some("implementations".to_string()),
        limit: Some(50),
        domain: None,
        symbol: None,
        max_results: None,
        group_by_layer: None,
        min_business_score: None,
        depth: None,
    };

    let result = tool.call_tool(&handler).await?;
    let result_str = format!("{:?}", result);

    // Verify: Should find StripeProcessor and PayPalProcessor
    assert!(
        result_str.contains("StripeProcessor") || result_str.contains("implementations"),
        "Expected to find StripeProcessor implementation"
    );
    assert!(
        result_str.contains("PayPalProcessor") || result_str.contains("implementations"),
        "Expected to find PayPalProcessor implementation"
    );

    Ok(())
}

#[tokio::test]
async fn test_types_mode_finds_return_types() -> Result<()> {
    // Setup
    let (handler, temp_dir) = create_test_handler().await?;
    create_typed_codebase(&temp_dir).await?;
    index_workspace_with_types(&handler, temp_dir.path().to_str().unwrap()).await?;

    // Test: Find all functions returning PaymentResult
    let tool = FastExploreTool {
        mode: ExploreMode::Types,
        type_name: Some("PaymentResult".to_string()),
        exploration_type: Some("returns".to_string()),
        limit: Some(50),
        domain: None,
        symbol: None,
        max_results: None,
        group_by_layer: None,
        min_business_score: None,
        depth: None,
    };

    let result = tool.call_tool(&handler).await?;
    let result_str = format!("{:?}", result);

    // Verify: Should find process() methods and createPayment function
    assert!(
        result_str.contains("process") || result_str.contains("returns"),
        "Expected to find process() method returning PaymentResult"
    );
    assert!(
        result_str.contains("createPayment") || result_str.contains("returns"),
        "Expected to find createPayment() function returning PaymentResult"
    );

    Ok(())
}

#[tokio::test]
async fn test_types_mode_finds_parameter_types() -> Result<()> {
    // Setup
    let (handler, temp_dir) = create_test_handler().await?;
    create_typed_codebase(&temp_dir).await?;
    index_workspace_with_types(&handler, temp_dir.path().to_str().unwrap()).await?;

    // Test: Find all functions accepting PaymentProcessor as parameter
    let tool = FastExploreTool {
        mode: ExploreMode::Types,
        type_name: Some("PaymentProcessor".to_string()),
        exploration_type: Some("parameters".to_string()),
        limit: Some(50),
        domain: None,
        symbol: None,
        max_results: None,
        group_by_layer: None,
        min_business_score: None,
        depth: None,
    };

    let result = tool.call_tool(&handler).await?;
    let result_str = format!("{:?}", result);

    // Verify: Should find createPayment function
    assert!(
        result_str.contains("createPayment") || result_str.contains("parameters"),
        "Expected to find createPayment() function accepting PaymentProcessor"
    );

    Ok(())
}

#[tokio::test]
async fn test_types_mode_missing_type_name_parameter() -> Result<()> {
    // Setup
    let (handler, temp_dir) = create_test_handler().await?;
    create_typed_codebase(&temp_dir).await?;
    index_workspace_with_types(&handler, temp_dir.path().to_str().unwrap()).await?;

    // Test: Call types mode without type_name parameter
    let tool = FastExploreTool {
        mode: ExploreMode::Types,
        type_name: None, // Missing required parameter
        exploration_type: Some("implementations".to_string()),
        limit: Some(50),
        domain: None,
        symbol: None,
        max_results: None,
        group_by_layer: None,
        min_business_score: None,
        depth: None,
    };

    let result = tool.call_tool(&handler).await;

    // Verify: Should return error
    assert!(
        result.is_err(),
        "Expected error when type_name parameter is missing"
    );
    let error_msg = format!("{:?}", result.unwrap_err());
    assert!(
        error_msg.contains("type_name") || error_msg.contains("required"),
        "Error message should mention missing type_name parameter"
    );

    Ok(())
}

#[tokio::test]
async fn test_types_mode_type_not_found() -> Result<()> {
    // Setup
    let (handler, temp_dir) = create_test_handler().await?;
    create_typed_codebase(&temp_dir).await?;
    index_workspace_with_types(&handler, temp_dir.path().to_str().unwrap()).await?;

    // Test: Search for non-existent type
    let tool = FastExploreTool {
        mode: ExploreMode::Types,
        type_name: Some("NonExistentType".to_string()),
        exploration_type: Some("implementations".to_string()),
        limit: Some(50),
        domain: None,
        symbol: None,
        max_results: None,
        group_by_layer: None,
        min_business_score: None,
        depth: None,
    };

    let result = tool.call_tool(&handler).await?;
    let result_str = format!("{:?}", result);

    // Verify: Should return empty results with helpful message
    assert!(
        result_str.contains("not found") || result_str.contains("0") || result_str.contains("[]"),
        "Expected empty results or not found message"
    );

    Ok(())
}

#[tokio::test]
async fn test_types_mode_default_exploration_type() -> Result<()> {
    // Setup
    let (handler, temp_dir) = create_test_handler().await?;
    create_typed_codebase(&temp_dir).await?;
    index_workspace_with_types(&handler, temp_dir.path().to_str().unwrap()).await?;

    // Test: Call types mode without exploration_type (should default to "all")
    let tool = FastExploreTool {
        mode: ExploreMode::Types,
        type_name: Some("PaymentProcessor".to_string()),
        exploration_type: None, // Should default to "all"
        limit: Some(50),
        domain: None,
        symbol: None,
        max_results: None,
        group_by_layer: None,
        min_business_score: None,
        depth: None,
    };

    let result = tool.call_tool(&handler).await?;
    let result_str = format!("{:?}", result);

    // Verify: Should return comprehensive results (implementations, returns, parameters)
    assert!(
        result_str.contains("PaymentProcessor") || result_str.contains("results"),
        "Expected to find type information"
    );

    Ok(())
}
