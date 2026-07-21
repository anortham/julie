use std::io;

use xtask_eval::cli::{CliCommand, parse_cli_command};
use xtask_eval::search_ablation::run_eval_ablation_command;
use xtask_eval::search_matrix::run_search_matrix_command;

fn main() -> anyhow::Result<()> {
    let command = parse_cli_command(std::env::args())?;
    let mut stdout = io::stdout().lock();

    match command {
        CliCommand::SearchMatrix(command) => {
            run_search_matrix_command(&command, &mut stdout)?;
        }
        CliCommand::Eval(command) => {
            run_eval_ablation_command(&command, &mut stdout)?;
        }
    }

    Ok(())
}
