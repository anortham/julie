use std::ffi::OsString;
use std::io;
use std::process::{Command, ExitStatus};

#[cfg(windows)]
use std::path::{Path, PathBuf};

pub fn cargo_status(args: &[&str]) -> io::Result<ExitStatus> {
    let mut command = program_command("cargo");
    command.args(args).status()
}

#[cfg(not(windows))]
pub fn manifest_command(command: &str) -> io::Result<Command> {
    let mut shell = Command::new("sh");
    shell.arg("-c").arg(command);
    Ok(shell)
}

#[cfg(windows)]
pub fn manifest_command(command: &str) -> io::Result<Command> {
    let parts = split_command(command)?;
    let Some((program, args)) = parts.split_first() else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty command"));
    };

    let mut command = program_command(program);
    command.args(args);
    Ok(command)
}

#[cfg(windows)]
fn split_command(command: &str) -> io::Result<Vec<String>> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut in_word = false;

    for character in command.chars() {
        match quote {
            Some(quote_character) if character == quote_character => {
                quote = None;
                in_word = true;
            }
            Some(_) => {
                current.push(character);
                in_word = true;
            }
            None if character == '\'' || character == '"' => {
                quote = Some(character);
                in_word = true;
            }
            None if character.is_whitespace() => {
                if in_word {
                    parts.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            None => {
                current.push(character);
                in_word = true;
            }
        }
    }

    if quote.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unterminated quote in command: {command}"),
        ));
    }

    if in_word {
        parts.push(current);
    }

    Ok(parts)
}

#[cfg(not(windows))]
fn program_command(program: &str) -> Command {
    Command::new(program)
}

#[cfg(windows)]
fn program_command(program: &str) -> Command {
    let program = find_program_on_path(program)
        .map(PathBuf::into_os_string)
        .unwrap_or_else(|| OsString::from(program));
    Command::new(program)
}

#[cfg(windows)]
fn find_program_on_path(program: &str) -> Option<PathBuf> {
    let path = std::env::vars_os()
        .find(|(key, _)| key.to_string_lossy().eq_ignore_ascii_case("PATH"))
        .map(|(_, value)| value)?;

    for directory in std::env::split_paths(&path) {
        for extension in executable_extensions(program) {
            let candidate = directory.join(format!("{program}{extension}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

#[cfg(windows)]
fn executable_extensions(program: &str) -> Vec<String> {
    if Path::new(program).extension().is_some() {
        return vec![String::new()];
    }

    let mut extensions: Vec<String> = std::env::var_os("PATHEXT")
        .map(|value| {
            value
                .to_string_lossy()
                .split(';')
                .filter(|extension| !extension.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default();

    for fallback in [".COM", ".EXE", ".BAT", ".CMD"] {
        if !extensions
            .iter()
            .any(|extension| extension.eq_ignore_ascii_case(fallback))
        {
            extensions.push(fallback.to_string());
        }
    }

    extensions
}

#[cfg(all(test, windows))]
mod tests {
    use super::manifest_command;

    #[test]
    fn process_tests_manifest_command_preserves_quoted_filter_expression() {
        let command = manifest_command(
            "cargo nextest run -p julie-extractors -E 'test(golden) | test(capability_matrix)'",
        )
        .expect("parse command");

        let args: Vec<String> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();

        assert_eq!(
            args,
            vec![
                "nextest",
                "run",
                "-p",
                "julie-extractors",
                "-E",
                "test(golden) | test(capability_matrix)"
            ]
        );
    }
}
