use anyhow::Result;
use clap::Subcommand;
use dialoguer::{Confirm, Password};
use serde_json::json;

use crate::api::LinearClient;
use crate::config;
use crate::output::{print_json, OutputOptions};

#[derive(Subcommand)]
pub enum AuthCommands {
    /// Store API key for the current profile
    Login {
        /// API key to store (if omitted, prompt interactively)
        #[arg(long, value_name = "KEY")]
        key: Option<String>,
        /// Validate the API key before saving
        #[arg(long)]
        validate: bool,
        /// Store in OS keyring instead of config file (requires secure-storage feature)
        #[arg(long)]
        secure: bool,
    },
    /// Remove API key for the current profile
    Logout {
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },
    /// Show current auth status
    Status {
        /// Validate API access
        #[arg(long)]
        validate: bool,
    },
    /// Migrate API keys from config file to OS keyring
    #[cfg(feature = "secure-storage")]
    Migrate {
        /// Keep keys in config file after migrating
        #[arg(long)]
        keep_config: bool,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },
}

pub async fn handle(cmd: AuthCommands, output: &OutputOptions) -> Result<()> {
    match cmd {
        AuthCommands::Login {
            key,
            validate,
            secure,
        } => login(key, validate, secure, output).await,
        AuthCommands::Logout { force } => logout(force, output).await,
        AuthCommands::Status { validate } => status(validate, output).await,
        #[cfg(feature = "secure-storage")]
        AuthCommands::Migrate { keep_config, force } => migrate(keep_config, force, output).await,
    }
}

async fn login(
    key: Option<String>,
    validate: bool,
    secure: bool,
    output: &OutputOptions,
) -> Result<()> {
    let key = match key {
        Some(key) => key,
        None => Password::new().with_prompt("Linear API key").interact()?,
    };

    if validate {
        validate_key(&key).await?;
    }

    let profile = resolve_profile_for_write()?;

    #[cfg(feature = "secure-storage")]
    if secure {
        crate::keyring::set_key(&profile, &key)?;

        if output.is_json() || output.has_template() {
            print_json(
                &json!({
                    "profile": profile,
                    "saved": true,
                    "storage": "keyring"
                }),
                output,
            )?;
            return Ok(());
        }

        println!("API key saved to keyring for profile '{}'", profile);
        return Ok(());
    }

    #[cfg(not(feature = "secure-storage"))]
    if secure {
        anyhow::bail!("Secure storage requires the 'secure-storage' feature. Rebuild with: cargo build --features secure-storage");
    }

    config::set_workspace_key(&profile, &key)?;

    if output.is_json() || output.has_template() {
        print_json(
            &json!({
                "profile": profile,
                "saved": true,
                "storage": "config"
            }),
            output,
        )?;
        return Ok(());
    }

    println!("API key saved for profile '{}'", profile);
    Ok(())
}

async fn logout(force: bool, output: &OutputOptions) -> Result<()> {
    let profile = config::current_profile()?;

    if !force {
        let confirmed = Confirm::new()
            .with_prompt(format!(
                "Remove API key and profile '{}' from config?",
                profile
            ))
            .default(false)
            .interact()?;
        if !confirmed {
            return Ok(());
        }
    }

    // Remove from keyring if feature is enabled
    #[cfg(feature = "secure-storage")]
    {
        let _ = crate::keyring::delete_key(&profile); // Ignore errors (may not exist)
    }

    config::workspace_remove(&profile)?;

    if output.is_json() || output.has_template() {
        print_json(
            &json!({
                "profile": profile,
                "removed": true
            }),
            output,
        )?;
        return Ok(());
    }

    println!("Removed profile '{}'", profile);
    Ok(())
}

async fn status(validate: bool, output: &OutputOptions) -> Result<()> {
    let config_data = config::load_config()?;
    let profile = config::current_profile().ok();
    let env_key = std::env::var("LINEAR_API_KEY")
        .ok()
        .filter(|k| !k.is_empty());
    let env_profile = std::env::var("LINEAR_CLI_PROFILE")
        .ok()
        .filter(|p| !p.is_empty());

    let configured = profile
        .as_ref()
        .and_then(|p| config_data.workspaces.get(p))
        .map(|w| !w.api_key.is_empty())
        .unwrap_or(false);

    // Check keyring storage
    #[cfg(feature = "secure-storage")]
    let keyring_configured = profile
        .as_ref()
        .and_then(|p| crate::keyring::get_key(p).ok())
        .flatten()
        .is_some();
    #[cfg(not(feature = "secure-storage"))]
    let keyring_configured = false;

    #[cfg(feature = "secure-storage")]
    let keyring_available = crate::keyring::is_available();
    #[cfg(not(feature = "secure-storage"))]
    let keyring_available = false;

    let mut validated = None;
    if validate {
        // Try to get key using the priority: env > keyring > config
        let key = env_key.clone().or_else(|| {
            #[cfg(feature = "secure-storage")]
            if let Some(ref p) = profile {
                if let Ok(Some(k)) = crate::keyring::get_key(p) {
                    return Some(k);
                }
            }
            profile
                .as_ref()
                .and_then(|p| config_data.workspaces.get(p))
                .map(|w| w.api_key.clone())
        });
        validated = match key {
            Some(key) => Some(validate_key(&key).await.is_ok()),
            None => Some(false),
        };
    }

    if output.is_json() || output.has_template() {
        print_json(
            &json!({
                "profile": profile,
                "configured": configured,
                "keyring_configured": keyring_configured,
                "keyring_available": keyring_available,
                "env_api_key": env_key.is_some(),
                "env_profile": env_profile,
                "validated": validated,
            }),
            output,
        )?;
        return Ok(());
    }

    println!(
        "Profile: {}",
        profile.clone().unwrap_or_else(|| "none".to_string())
    );
    println!("Config file: {}", if configured { "yes" } else { "no" });
    println!("Keyring: {}", if keyring_configured { "yes" } else { "no" });
    println!(
        "Keyring available: {}",
        if keyring_available { "yes" } else { "no" }
    );
    println!(
        "Env API key override: {}",
        if env_key.is_some() { "yes" } else { "no" }
    );
    if let Some(validated) = validated {
        println!("Validated: {}", if validated { "yes" } else { "no" });
    }

    Ok(())
}

async fn validate_key(key: &str) -> Result<()> {
    let client = LinearClient::with_api_key(key.to_string())?;
    let query = r#"
        query {
            viewer {
                id
            }
        }
    "#;
    let result = client.query(query, None).await?;
    let viewer = &result["data"]["viewer"];
    if viewer.is_null() {
        anyhow::bail!("API key validation failed");
    }
    Ok(())
}

fn resolve_profile_for_write() -> Result<String> {
    if let Ok(profile) = std::env::var("LINEAR_CLI_PROFILE") {
        if !profile.trim().is_empty() {
            return Ok(profile);
        }
    }
    let config_data = config::load_config()?;
    Ok(config_data.current.unwrap_or_else(|| "default".to_string()))
}

#[cfg(feature = "secure-storage")]
async fn migrate(keep_config: bool, force: bool, output: &OutputOptions) -> Result<()> {
    if !crate::keyring::is_available() {
        anyhow::bail!(
            "Keyring is not available on this system. Check that a secret service is running."
        );
    }

    let config_data = config::load_config()?;

    if config_data.workspaces.is_empty() {
        if output.is_json() || output.has_template() {
            print_json(
                &json!({ "migrated": 0, "message": "No workspaces to migrate" }),
                output,
            )?;
            return Ok(());
        }
        println!("No workspaces to migrate.");
        return Ok(());
    }

    let workspace_names: Vec<_> = config_data.workspaces.keys().cloned().collect();

    if !force {
        println!(
            "This will migrate {} workspace(s) to the keyring:",
            workspace_names.len()
        );
        for name in &workspace_names {
            println!("  - {}", name);
        }
        if !keep_config {
            println!("\nAPI keys will be removed from the config file after migration.");
        }
        let confirmed = Confirm::new()
            .with_prompt("Continue?")
            .default(false)
            .interact()?;
        if !confirmed {
            return Ok(());
        }
    }

    let mut migrated = 0;
    let mut failed: Vec<String> = Vec::new();

    for (name, workspace) in &config_data.workspaces {
        match crate::keyring::set_key(name, &workspace.api_key) {
            Ok(()) => {
                migrated += 1;
                if !output.is_json() && !output.has_template() {
                    println!("Migrated: {}", name);
                }
            }
            Err(e) => {
                failed.push(format!("{}: {}", name, e));
            }
        }
    }

    // Remove keys from config if requested and all succeeded
    if !keep_config && failed.is_empty() {
        let mut new_config = config_data;
        for name in &workspace_names {
            if let Some(ws) = new_config.workspaces.get_mut(name) {
                ws.api_key = String::new();
            }
        }
        config::save_config(&new_config)?;
    }

    if output.is_json() || output.has_template() {
        print_json(
            &json!({
                "migrated": migrated,
                "failed": failed,
                "config_cleared": !keep_config && failed.is_empty()
            }),
            output,
        )?;
        return Ok(());
    }

    println!("\nMigrated {} workspace(s) to keyring.", migrated);
    if !failed.is_empty() {
        println!("Failed to migrate:");
        for f in &failed {
            println!("  - {}", f);
        }
    }
    if !keep_config && failed.is_empty() {
        println!("API keys removed from config file.");
    }

    Ok(())
}
