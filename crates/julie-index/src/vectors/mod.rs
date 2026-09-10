//! Symbol vectors served with a snapshot. Empty until Task 10 fills it.

#[derive(Debug, Default)]
pub struct VectorSet {}

impl VectorSet {
    pub fn empty() -> Self {
        Self {}
    }

    pub fn is_empty(&self) -> bool {
        true
    }
}
