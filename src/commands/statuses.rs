use anyhow::Result;
use clap::Subcommand;
use colored::Colorize;
use serde_json::json;
use tabled::{Table, Tabled};

use crate::api::{resolve_team_id, LinearClient};

#[derive(Subcommand)]
pub enum StatusCommands {
    /// List all issue statuses for a team
    #[command(alias = "ls")]
    List {
        /// Team name or ID
        #[arg(short, long)]
        team: String,
    },
    /// Get details of a specific status
    Get {
        /// Status name or ID
        id: String,
        /// Team name or ID
        #[arg(short, long)]
        team: String,
    },
}

#[derive(Tabled)]
struct StatusRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Type")]
    status_type: String,
    #[tabled(rename = "Color")]
    color: String,
    #[tabled(rename = "Position")]
    position: String,
    #[tabled(rename = "ID")]
    id: String,
}

pub async fn handle(cmd: StatusCommands) -> Result<()> {
    match cmd {
        StatusCommands::List { team } => list_statuses(&team).await,
        StatusCommands::Get { id, team } => get_status(&id, &team).await,
    }
}

async fn list_statuses(team: &str) -> Result<()> {
    let client = LinearClient::new()?;

    // Resolve team key/name to UUID
    let team_id = resolve_team_id(&client, team).await?;

    let query = r#"
        query($teamId: String!) {
            team(id: $teamId) {
                id
                name
                states {
                    nodes {
                        id
                        name
                        type
                        color
                        position
                        description
                    }
                }
            }
        }
    "#;

    let result = client.query(query, Some(json!({ "teamId": team_id }))).await?;
    let team_data = &result["data"]["team"];

    if team_data.is_null() {
        anyhow::bail!("Team not found: {}", team);
    }

    let team_name = team_data["name"].as_str().unwrap_or("");
    let empty = vec![];
    let states = team_data["states"]["nodes"]
        .as_array()
        .unwrap_or(&empty);

    if states.is_empty() {
        println!("No statuses found for team '{}'.", team_name);
        return Ok(());
    }

    println!("{}", format!("Issue statuses for team '{}'", team_name).bold());
    println!("{}", "-".repeat(50));

    let rows: Vec<StatusRow> = states
        .iter()
        .map(|s| {
            let status_type = s["type"].as_str().unwrap_or("");
            let type_colored = match status_type {
                "completed" => status_type.green().to_string(),
                "started" => status_type.yellow().to_string(),
                "canceled" | "cancelled" => status_type.red().to_string(),
                "backlog" => status_type.dimmed().to_string(),
                "unstarted" => status_type.cyan().to_string(),
                _ => status_type.to_string(),
            };

            StatusRow {
                name: s["name"].as_str().unwrap_or("").to_string(),
                status_type: type_colored,
                color: s["color"].as_str().unwrap_or("").to_string(),
                position: s["position"].as_f64().map(|p| format!("{:.0}", p)).unwrap_or("-".to_string()),
                id: s["id"].as_str().unwrap_or("").to_string(),
            }
        })
        .collect();

    let table = Table::new(rows).to_string();
    println!("{}", table);
    println!("\n{} statuses", states.len());

    Ok(())
}

async fn get_status(id: &str, team: &str) -> Result<()> {
    let client = LinearClient::new()?;

    // Resolve team key/name to UUID
    let team_id = resolve_team_id(&client, team).await?;

    // First get all states for the team and find the matching one
    let query = r#"
        query($teamId: String!) {
            team(id: $teamId) {
                id
                name
                states {
                    nodes {
                        id
                        name
                        type
                        color
                        position
                        description
                    }
                }
            }
        }
    "#;

    let result = client.query(query, Some(json!({ "teamId": team_id }))).await?;
    let team_data = &result["data"]["team"];

    if team_data.is_null() {
        anyhow::bail!("Team not found: {}", team);
    }

    let empty = vec![];
    let states = team_data["states"]["nodes"]
        .as_array()
        .unwrap_or(&empty);

    // Find matching status by ID or name
    let status = states.iter().find(|s| {
        s["id"].as_str() == Some(id) ||
        s["name"].as_str().map(|n| n.to_lowercase()) == Some(id.to_lowercase())
    });

    match status {
        Some(s) => {
            println!("{}", s["name"].as_str().unwrap_or("").bold());
            println!("{}", "-".repeat(40));
            println!("Type: {}", s["type"].as_str().unwrap_or("-"));
            println!("Color: {}", s["color"].as_str().unwrap_or("-"));
            println!("Position: {}", s["position"].as_f64().map(|p| format!("{:.0}", p)).unwrap_or("-".to_string()));
            if let Some(desc) = s["description"].as_str() {
                if !desc.is_empty() {
                    println!("Description: {}", desc);
                }
            }
            println!("ID: {}", s["id"].as_str().unwrap_or("-"));
            Ok(())
        }
        None => {
            anyhow::bail!("Status not found: {}", id);
        }
    }
}
