use std::time::Duration;

use crate::manifest::TestManifest;

use super::{BucketResult, BucketStatus, PROGRAM_TIERS, RunSummary};

pub fn render_manifest_listing(manifest: &TestManifest) -> String {
    let mut output = String::from("TIERS\n");
    for (tier_name, buckets) in &manifest.tiers {
        output.push_str(&format!("- {tier_name}: {}\n", buckets.join(", ")));
    }

    output.push_str("\nBUCKETS\n");
    for (bucket_name, bucket) in &manifest.buckets {
        let expensive_marker = if bucket.expensive { " [expensive]" } else { "" };
        output.push_str(&format!("- {bucket_name}{expensive_marker}\n"));
        output.push_str(&format!(
            "  expected_seconds = {}\n",
            bucket.expected_seconds
        ));
        output.push_str(&format!("  timeout_seconds = {}\n", bucket.timeout_seconds));
        output.push_str(&format!("  scope_label = {}\n", bucket.scope_label));
        output.push_str(&format!("  owner = {}\n", bucket.owner));
        output.push_str(&format!("  expensive = {}\n", bucket.expensive));
        if let Some(notes) = &bucket.notes {
            output.push_str(&format!("  notes = {notes}\n"));
        }
        for command in &bucket.commands {
            output.push_str(&format!("  command = {command}\n"));
        }
    }

    output.push_str("\nPROGRAM TIERS\n");
    for (tier_name, buckets) in PROGRAM_TIERS {
        output.push_str(&format!("- {tier_name}: {}\n", buckets.join(", ")));
    }

    output.push_str("\nWORKFLOWS\n");
    output.push_str("- changed: infer buckets from the current git diff\n");

    output
}

pub fn render_summary(summary: &RunSummary) -> String {
    let cold_wall = summary.prebuild_elapsed + summary.total_elapsed;
    let mut output = format!(
        "SUMMARY: {} buckets passed in {} (warm)\nPREBUILD: {}\nCOLD WALL: {}\n",
        summary.passed_buckets,
        format_duration(summary.total_elapsed),
        format_duration(summary.prebuild_elapsed),
        format_duration(cold_wall),
    );

    for result in &summary.bucket_results {
        let slow_marker = if bucket_is_slow(result) { " SLOW" } else { "" };
        output.push_str(&format!(
            "- {}: expected {}, actual {}, commands {}, scope {}{}\n",
            result.bucket_name,
            format_duration(Duration::from_secs(result.expected_seconds)),
            format_duration(result.elapsed),
            result.command_count,
            result.scope_label,
            slow_marker
        ));
    }

    output
}

pub fn render_bucket_result(result: &BucketResult) -> String {
    format!(
        "START {}\nEND {} {} ({})\n",
        result.bucket_name,
        result.bucket_name,
        status_label(result.status),
        format_duration(result.elapsed)
    )
}

pub(super) fn render_bucket_end(result: &BucketResult) -> String {
    format!(
        "END {} {} ({})\n",
        result.bucket_name,
        status_label(result.status),
        format_duration(result.elapsed)
    )
}

pub(super) fn status_label(status: BucketStatus) -> &'static str {
    match status {
        BucketStatus::Passed => "PASS",
        BucketStatus::Failed => "FAIL",
        BucketStatus::TimedOut => "TIMEOUT",
    }
}

pub(super) fn format_duration(duration: Duration) -> String {
    format!("{:.1}s", duration.as_secs_f64())
}

fn bucket_is_slow(result: &BucketResult) -> bool {
    if result.expected_seconds == 0 {
        return false;
    }

    result.elapsed.as_secs_f64() > (result.expected_seconds as f64 * 1.5)
}
