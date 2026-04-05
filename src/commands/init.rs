use crate::config::{write_config, DotenvzConfig, CONFIG_FILENAME};
use crate::errors::Result;

/// Scaffold a `.dotenvz.toml` in the current directory.
///
/// Does nothing if the file already exists, preserving any existing config.
pub fn run(project_name: Option<&str>) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let config_path = cwd.join(CONFIG_FILENAME);

    if config_path.exists() {
        eprintln!("warning: .dotenvz.toml already exists — skipping.");
        return Ok(());
    }

    let name = project_name.unwrap_or_else(|| {
        cwd.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("my-app")
    });

    let config = DotenvzConfig::scaffold(name);
    write_config(&config_path, &config)?;

    println!("✓ Created .dotenvz.toml for project '{}'", config.project);
    println!("  Edit the [commands] section to add your project's aliases.");
    Ok(())
}
