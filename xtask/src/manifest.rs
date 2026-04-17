use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, anyhow, bail};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestManifest {
    pub tiers: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub blocked_tiers: BTreeMap<String, String>,
    pub buckets: BTreeMap<String, BucketConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BucketConfig {
    pub expected_seconds: u64,
    pub timeout_seconds: u64,
    pub commands: Vec<String>,
}

impl TestManifest {
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let contents = fs::read_to_string(path)
            .with_context(|| format!("failed to read manifest at {}", path.display()))?;

        Self::from_str(&contents)
            .with_context(|| format!("failed to load manifest at {}", path.display()))
    }

    pub fn from_str(contents: &str) -> anyhow::Result<Self> {
        let manifest: Self = toml::from_str(contents)
            .map_err(|error| anyhow!("failed to parse test manifest: {error}"))?;
        manifest.validate()
    }

    fn validate(self) -> anyhow::Result<Self> {
        for (bucket_name, bucket) in &self.buckets {
            if bucket.commands.is_empty() {
                bail!("bucket '{bucket_name}' must define at least one command");
            }

            if bucket.expected_seconds == 0 {
                bail!("bucket '{bucket_name}' expected_seconds must be > 0");
            }

            if bucket.timeout_seconds == 0 {
                bail!("bucket '{bucket_name}' timeout_seconds must be > 0");
            }

            if bucket.timeout_seconds < bucket.expected_seconds {
                bail!("bucket '{bucket_name}' timeout_seconds must be >= expected_seconds");
            }
        }

        for (tier_name, bucket_names) in &self.tiers {
            if bucket_names.is_empty() {
                bail!("tier '{tier_name}' must define at least one bucket");
            }

            for bucket_name in bucket_names {
                if !self.buckets.contains_key(bucket_name) {
                    bail!("tier '{tier_name}' references missing bucket '{bucket_name}'");
                }
            }
        }

        for (tier_name, reason) in &self.blocked_tiers {
            if !self.tiers.contains_key(tier_name) {
                bail!("blocked tier '{tier_name}' references missing tier");
            }

            if reason.trim().is_empty() {
                bail!("blocked tier '{tier_name}' must include a reason");
            }
        }

        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::TestManifest;
    use std::path::PathBuf;

    fn manifest_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_tiers.toml")
    }

    #[test]
    fn manifest_tests_dev_and_full_include_dashboard_bucket() {
        let manifest = TestManifest::load(manifest_path()).expect("load manifest");

        assert!(
            manifest.buckets.contains_key("dashboard"),
            "manifest should define a dashboard bucket"
        );
        assert!(
            manifest
                .tiers
                .get("dev")
                .is_some_and(|buckets| buckets.iter().any(|bucket| bucket == "dashboard")),
            "dev tier should run dashboard tests"
        );
        assert!(
            manifest
                .tiers
                .get("full")
                .is_some_and(|buckets| buckets.iter().any(|bucket| bucket == "dashboard")),
            "full tier should run dashboard tests"
        );
    }
}
