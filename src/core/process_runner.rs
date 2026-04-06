use std::collections::HashMap;
use std::process::Command;

use crate::errors::{DotenvzError, Result};

/// Run a shell command string with injected environment variables.
///
/// The command string is split using `shell-words` so that quoted arguments and
/// escaped characters are handled correctly — e.g. `next dev --port "3000"`.
pub fn run_command_string(command_str: &str, env: &HashMap<String, String>) -> Result<()> {
    let parts = shell_words::split(command_str).map_err(|e| {
        DotenvzError::ProcessExec(format!("Failed to parse command `{command_str}`: {e}"))
    })?;

    let (program, rest) = parts
        .split_first()
        .ok_or_else(|| DotenvzError::ProcessExec("Empty command string".into()))?;

    let args: Vec<&str> = rest.iter().map(String::as_str).collect();
    run_process(program, &args, env)
}

/// Spawn a child process with the given program, arguments, and environment.
///
/// The current process environment is inherited; entries in `env` are
/// overlaid on top, allowing secrets to override or supplement existing vars.
///
/// Returns a descriptive error if the program is not found or exits non-zero.
pub fn run_process(program: &str, args: &[&str], env: &HashMap<String, String>) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .envs(env)
        .status()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                DotenvzError::ProcessExec(format!(
                    "Command not found: `{program}`. Is it installed and on your PATH?"
                ))
            } else {
                DotenvzError::ProcessExec(e.to_string())
            }
        })?;

    if !status.success() {
        let code = status.code().unwrap_or(-1);
        return Err(DotenvzError::ProcessExec(format!(
            "Process `{program}` exited with code {code}"
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_process_succeeds_for_echo() {
        let env = HashMap::new();
        run_process("echo", &["hello"], &env).unwrap();
    }

    #[test]
    fn run_process_returns_not_found_error() {
        let env = HashMap::new();
        let err = run_process("__no_such_binary__", &[], &env).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not found") || msg.contains("__no_such_binary__"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn run_process_injects_env_variables() {
        // Run `sh -c 'test "$DOTENVZ_INJECT" = "hello"'` — exits 0 only if the
        // variable is injected correctly.
        let mut env = HashMap::new();
        env.insert("DOTENVZ_INJECT".to_string(), "hello".to_string());
        run_process("sh", &["-c", r#"test "$DOTENVZ_INJECT" = "hello""#], &env).unwrap();
    }

    #[test]
    fn run_command_string_handles_quoted_args() {
        let env = HashMap::new();
        // `echo` with a quoted argument that contains whitespace — shell-words
        // should treat it as a single arg, so echo exits 0.
        run_command_string("echo 'hello world'", &env).unwrap();
    }

    #[test]
    fn run_command_string_error_on_empty_string() {
        let env = HashMap::new();
        assert!(run_command_string("", &env).is_err());
    }
}
