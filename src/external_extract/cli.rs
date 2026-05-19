use std::ffi::OsString;
use std::path::PathBuf;

use clap::{ArgAction, Args, CommandFactory, Parser, Subcommand};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalExtractArgs {
    pub db: PathBuf,
    pub root: Option<PathBuf>,
    pub strict_schema: bool,
    pub ignore_files: Vec<PathBuf>,
    pub workspace_id: Option<String>,
    pub analyze: bool,
    pub command: ExternalExtractCommand,
}

impl ExternalExtractArgs {
    pub fn try_parse_from<I, T>(itr: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        let parsed = ExternalExtractParser::try_parse_from(itr)?;
        parsed.raw.validate()
    }
}

impl ExternalExtractRawArgs {
    pub fn validate(self) -> Result<ExternalExtractArgs, clap::Error> {
        let Some(db) = self.db else {
            return Err(clap::Error::raw(
                clap::error::ErrorKind::MissingRequiredArgument,
                "the following required arguments were not provided: --db",
            ));
        };

        if self.command.requires_root() && self.root.is_none() {
            return Err(clap::Error::raw(
                clap::error::ErrorKind::MissingRequiredArgument,
                "the following required arguments were not provided: --root",
            ));
        }

        Ok(ExternalExtractArgs {
            db,
            root: self.root,
            strict_schema: self.strict_schema,
            ignore_files: self.ignore_files,
            workspace_id: self.workspace_id,
            analyze: self.analyze,
            command: self.command,
        })
    }
}

impl CommandFactory for ExternalExtractArgs {
    fn command() -> clap::Command {
        ExternalExtractParser::command()
    }

    fn command_for_update() -> clap::Command {
        ExternalExtractParser::command_for_update()
    }
}

#[derive(Debug, Clone, Parser, PartialEq, Eq)]
#[command(name = "extract", about = "Manage external extractor SQLite inputs")]
struct ExternalExtractParser {
    #[command(flatten)]
    raw: ExternalExtractRawArgs,
}

#[derive(Debug, Clone, Args, PartialEq, Eq)]
pub struct ExternalExtractRawArgs {
    #[arg(long, global = true)]
    db: Option<PathBuf>,

    #[arg(long, global = true)]
    root: Option<PathBuf>,

    #[arg(long, global = true, action = ArgAction::SetTrue)]
    strict_schema: bool,

    #[arg(long = "ignore-file", global = true)]
    ignore_files: Vec<PathBuf>,

    #[arg(long, global = true)]
    workspace_id: Option<String>,

    #[arg(long, global = true, action = ArgAction::SetTrue)]
    analyze: bool,

    #[command(subcommand)]
    command: ExternalExtractCommand,
}

#[derive(Debug, Clone, Subcommand, PartialEq, Eq)]
pub enum ExternalExtractCommand {
    Scan {
        #[arg(long, action = ArgAction::SetTrue)]
        force: bool,
    },
    Update {
        #[arg(long)]
        file: PathBuf,
    },
    Delete {
        #[arg(long)]
        file: PathBuf,
    },
    Analyze,
    Info,
}

impl ExternalExtractCommand {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Scan { .. } => "scan",
            Self::Update { .. } => "update",
            Self::Delete { .. } => "delete",
            Self::Analyze => "analyze",
            Self::Info => "info",
        }
    }

    fn requires_root(&self) -> bool {
        !matches!(self, Self::Analyze | Self::Info)
    }
}
