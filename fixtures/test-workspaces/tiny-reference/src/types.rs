//! Reference workspace types
//! Contains simple data structures for testing

/// A product in the reference workspace
pub struct ReferenceProduct {
    pub sku: String,
    pub price: f64,
}

impl ReferenceProduct {
    /// Create a new product
    pub fn new(sku: String, price: f64) -> Self {
        Self { sku, price }
    }

    /// Calculate discount price
    pub fn discounted_price(&self, discount_percent: f64) -> f64 {
        self.price * (1.0 - discount_percent / 100.0)
    }
}

/// Process product data (reference workspace function)
pub fn process_reference_data(product: &ReferenceProduct) -> f64 {
    product.discounted_price(10.0)
}
