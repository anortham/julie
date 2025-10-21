//! Primary workspace main entry point
//! This is a minimal fixture for testing primary workspace functionality

/// Calculate the sum of two numbers
pub fn calculate_sum(a: i32, b: i32) -> i32 {
    a + b
}

/// Primary workspace marker function
pub fn primary_marker_function() {
    println!("PRIMARY_WORKSPACE_MARKER");
}

fn main() {
    let result = calculate_sum(5, 3);
    println!("Sum: {}", result);
    primary_marker_function();
}
