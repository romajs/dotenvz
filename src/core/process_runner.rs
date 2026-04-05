use std::collections::HashMap;
use std::process::Command;

use crate::errors::{DotenvzError, Result};

/// Run a shell command string with injected environment variables.
///
/// The command string is split on whitespace. The first token becomes the
/// program name; the rest become arguments. For commands with quoted
/// arguments or shell operators, use `run_process` directly.
///
/// # TODO
/// Replace naive whitespace splitting with a proper shell-word parser
/// (e.g. the `shell-words` crate) once the basic flow is validated.
pub fn run_command_string(command_str: &str, env: &HashMap<String, String>) -> Result<()> {
    let mut parts = command_str.split_whitespace();
    let program = parts
        .next()
        .ok_or_else(|| DotenvzError::ProcessExec("Empty command string".into()))?;
    let args: Vec<&str> = parts.collect();
    run_process(program, &args, env)
}

/// Spawn a child process with the given program, arguments, and environment.
///
/// The current process environment is inherited; entries in `env` are
/// overlaid on top, allowing secrets to override or supplement existing vars.
///
/// Returns an error if the process fails to start or exits with a non-zero code.
pub fn run_process(
    program: &str,
    args: &[&str],
    env: &HashMap<String, String>,
) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .envs(env)
        .status()
        .map_err(|e| DotenvzError::ProcessExec(e.to_string()))?;

    if !status.success() {
        let code = status.code().unwrap_or(-1);
        return Err(DotenvzError::ProcessExec(format!(
            "Process exited with code {code}"
        )));
    }

    Ok(())
}
