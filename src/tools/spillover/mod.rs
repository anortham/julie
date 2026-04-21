pub mod store;

use anyhow::Result;
use rmcp::model::{CallToolResult, Content};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::handler::JulieServerHandler;

pub use store::{SpilloverFormat, SpilloverPage, SpilloverStore};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SpilloverGetTool {
    pub spillover_handle: String,
    #[serde(
        default,
        deserialize_with = "crate::utils::serde_lenient::deserialize_option_u32_lenient"
    )]
    pub limit: Option<u32>,
    #[serde(default)]
    pub format: Option<String>,
}

impl SpilloverGetTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        let page = handler.spillover_store.page(
            &handler.session_metrics.session_id,
            &self.spillover_handle,
            self.limit.map(|value| value as usize),
            Some(SpilloverFormat::from_option(self.format.as_deref())),
        )?;

        Ok(CallToolResult::success(vec![Content::text(format_page(
            &page,
        ))]))
    }
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
        output.push_str(&format!("More available: spillover_handle={next_handle}"));
    }

    output
}
