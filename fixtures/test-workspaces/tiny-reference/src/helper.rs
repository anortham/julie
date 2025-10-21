//! Reference workspace helper functions
//! This is a minimal fixture for testing reference workspace functionality

/// Calculate the product of two numbers
pub fn calculate_product(x: f64, y: f64) -> f64 {
    x * y
}

/// Reference workspace marker function
pub fn reference_marker_function() {
    println!("REFERENCE_WORKSPACE_MARKER");
}

/// Format a greeting message
pub fn format_greeting(name: &str) -> String {
    format!("Hello from reference workspace, {}!", name)
}
