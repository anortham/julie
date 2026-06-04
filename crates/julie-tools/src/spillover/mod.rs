pub mod store;

use anyhow::Result;
use julie_core::mcp_compat::{CallToolResult, Content};
use schemars::JsonSchema;
use serde::Deserialize;

use julie_context::ToolContext;

pub use store::{SpilloverFormat, SpilloverPage, SpilloverStore};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SpilloverGetTool {
    /// Opaque handle emitted by a prior paged tool response.
    pub spillover_handle: String,
    /// Maximum rows to return for this page.
    #[serde(
        default,
        deserialize_with = "julie_core::serde_lenient::deserialize_option_u32_lenient"
    )]
    pub limit: Option<u32>,
    /// Optional output format. Omit to keep the format captured in the handle.
    #[serde(default)]
    pub format: Option<String>,
}

impl SpilloverGetTool {
    pub async fn call_tool(&self, handler: &dyn ToolContext) -> Result<CallToolResult> {
        let format = self
            .format
            .as_deref()
            .map(SpilloverFormat::parse_strict)
            .transpose()
            .map_err(anyhow::Error::msg)?;
        let page = handler.spillover_store().page(
            handler.session_id(),
            &self.spillover_handle,
            self.limit.map(|value| value as usize),
            format,
        )?;

        Ok(CallToolResult::success(vec![Content::text(format_page(
            &page,
        ))]))
    }
}

pub fn more_available_marker(handle: &str) -> String {
    format!(
        "More available: spillover_handle={handle}\nNext page: spillover_get(spillover_handle=\"{handle}\")"
    )
}

fn format_page(page: &SpilloverPage) -> String {
    let mut output = String::new();
    output.push_str(&page.title);

    match page.format {
        SpilloverFormat::Readable => {
            output.push_str("\n\n");
        }
        SpilloverFormat::Compact => {
            output.push('\n');
        }
    }

    for (index, row) in page.rows.iter().enumerate() {
        output.push_str(row);
        if index + 1 < page.rows.len() {
            output.push('\n');
        }
    }

    if let Some(next_handle) = &page.next_handle {
        output.push_str("\n\n");
        output.push_str(&more_available_marker(next_handle));
    }

    output
}
