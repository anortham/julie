//! CLI helper for recover-edit operations.

use super::subcommands::WorkspaceArgs;
use crate::cli_tools::CliToolCommand;
use crate::request_engine::RequestFailure;
use async_trait::async_trait;
use clap::Parser;
use serde_json::{Map, Value};

pub fn populate_workspace_recover_args(args: &WorkspaceArgs, map: &mut Map<String, Value>) {
    if let Some(ref edit_id) = args.edit_id {
        map.insert("edit_id".into(), Value::String(edit_id.clone()));
    }
    if let Some(ref action) = args.action {
        map.insert("recovery_action".into(), Value::String(action.clone()));
    }
}

/// Standalone CLI args for recover-edit command.
#[derive(Debug, Clone, Parser)]
pub struct RecoverEditArgs {
    /// Opaque journal edit ID to recover
    #[arg(long)]
    pub edit_id: String,

    /// Recovery action: resume or rollback
    #[arg(long, value_parser = ["resume", "rollback"])]
    pub action: String,

    /// Target workspace path
    #[arg(id = "target_workspace", long = "target-workspace")]
    pub workspace: Option<String>,
}

#[async_trait]
impl CliToolCommand for RecoverEditArgs {
    fn tool_name(&self) -> &'static str {
        "manage_workspace"
    }

    fn to_tool_args_map(&self) -> Result<Map<String, Value>, RequestFailure> {
        let mut map = Map::new();
        map.insert("operation".into(), Value::String("recover_edit".into()));
        map.insert("edit_id".into(), Value::String(self.edit_id.clone()));
        map.insert("recovery_action".into(), Value::String(self.action.clone()));
        if let Some(ref ws) = self.workspace {
            map.insert("path".into(), Value::String(ws.clone()));
        }
        Ok(map)
    }

    fn validate_standalone(&self) -> anyhow::Result<()> {
        Ok(())
    }
}
