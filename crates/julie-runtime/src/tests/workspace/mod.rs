// Workspace tests — handler-free, safe to run inside julie-runtime.
pub mod registry; // ID generation, name sanitization, expiration logic
pub mod root_safety; // Sensitive-root rejection (macOS /var/root, HOME symlink, etc.)
