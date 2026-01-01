use anyhow::Result;
use clap::Subcommand;
use colored::Colorize;
use serde_json::json;
use tabled::{Table, Tabled};

use crate::api::{resolve_team_id, LinearClient};

#[derive(Subcommand)]
pub enum UserCommands {
    /// List all users in the workspace
    #[command(alias = "ls")]
    List {
        /// Filter users by team name or ID
        #[arg(short, long)]
        team: Option<String>,
    },
    /// Show current user details
    Me,
}

#[derive(Tabled)]
struct UserRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Email")]
    email: String,
    #[tabled(rename = "ID")]
    id: String,
}

pub async fn handle(cmd: UserCommands) -> Result<()> {
    match cmd {
        UserCommands::List { team } => list_users(team).await,
        UserCommands::Me => get_me().await,
    }
}

async fn list_users(team: Option<String>) -> Result<()> {
    let client = LinearClient::new()?;

    // Resolve team key/name to UUID if provided
    let resolved_team = if let Some(ref t) = team {
        Some(resolve_team_id(&client, t).await?)
    } else {
        None
    };

    let query = if resolved_team.is_some() {
        r#"
            query($teamId: String!) {
                team(id: $teamId) {
                    members {
                        nodes {
                            id
                            name
                            email
                        }
                    }
                }
            }
        "#
    } else {
        r#"
            query {
                users(first: 100) {
                    nodes {
                        id
                        name
                        email
                    }
                }
            }
        "#
    };

    let result = if let Some(ref team_id) = resolved_team {
        client.query(query, Some(json!({ "teamId": team_id }))).await?
    } else {
        client.query(query, None).await?
    };

    let users = if resolved_team.is_some() {
        result["data"]["team"]["members"]["nodes"]
            .as_array()
            .unwrap_or(&vec![])
            .clone()
    } else {
        result["data"]["users"]["nodes"]
            .as_array()
            .unwrap_or(&vec![])
            .clone()
    };

    if users.is_empty() {
        println!("No users found.");
        return Ok(());
    }

    let rows: Vec<UserRow> = users
        .iter()
        .map(|u| UserRow {
            name: u["name"].as_str().unwrap_or("").to_string(),
            email: u["email"].as_str().unwrap_or("").to_string(),
            id: u["id"].as_str().unwrap_or("").to_string(),
        })
        .collect();

    let table = Table::new(rows).to_string();
    println!("{}", table);
    println!("\n{} users", users.len());

    Ok(())
}

async fn get_me() -> Result<()> {
    let client = LinearClient::new()?;

    let query = r#"
        query {
            viewer {
                id
                name
                email
                displayName
                avatarUrl
                admin
                active
                createdAt
                url
            }
        }
    "#;

    let result = client.query(query, None).await?;
    let user = &result["data"]["viewer"];

    if user.is_null() {
        anyhow::bail!("Could not fetch current user");
    }

    println!("{}", user["name"].as_str().unwrap_or("").bold());
    println!("{}", "-".repeat(40));

    if let Some(display_name) = user["displayName"].as_str() {
        if !display_name.is_empty() {
            println!("Display Name: {}", display_name);
        }
    }

    println!("Email: {}", user["email"].as_str().unwrap_or("-"));
    println!("Admin: {}", user["admin"].as_bool().map(|b| if b { "Yes" } else { "No" }).unwrap_or("-"));
    println!("Active: {}", user["active"].as_bool().map(|b| if b { "Yes" } else { "No" }).unwrap_or("-"));

    if let Some(created) = user["createdAt"].as_str() {
        println!("Created: {}", created);
    }

    println!("URL: {}", user["url"].as_str().unwrap_or("-"));
    println!("ID: {}", user["id"].as_str().unwrap_or("-"));

    Ok(())
}
