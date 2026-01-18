use anyhow::Result;
use clap::Subcommand;
use colored::Colorize;
use serde_json::{json, Value};
use std::io::{self, BufRead};
use tabled::{Table, Tabled};

use crate::api::{resolve_team_id, LinearClient};
use crate::cache::{Cache, CacheType};
use crate::display_options;
use crate::output::{ensure_non_empty, filter_values, print_json, sort_values, OutputOptions};
use crate::pagination::paginate_nodes;
use crate::text::truncate;

#[derive(Subcommand)]
pub enum TeamCommands {
    /// List all teams
    #[command(alias = "ls")]
    List,
    /// Get team details
    Get {
        /// Team ID(s), key(s), or name(s). Use "-" to read from stdin.
        ids: Vec<String>,
    },
    /// Create a new team
    #[command(after_help = r##"EXAMPLES:
    linear teams create "Platform" -k PLT
    linear t create "Engineering" -k ENG -d "Core dev team"
    linear t create "Design" -k DSG -c "#FF5733" --icon "palette""##)]
    Create {
        /// Team name
        name: String,
        /// Team key (e.g., ENG)
        #[arg(short, long)]
        key: String,
        /// Team description
        #[arg(short, long)]
        description: Option<String>,
        /// Team color (hex)
        #[arg(short, long = "color-hex", id = "team_color")]
        color: Option<String>,
        /// Team icon
        #[arg(long)]
        icon: Option<String>,
    },
    /// Update an existing team
    #[command(after_help = r##"EXAMPLES:
    linear teams update ENG -n "Engineering"
    linear t update ENG -d "Updated description" -c "#10B981""##)]
    Update {
        /// Team ID, key, or name
        id: String,
        /// New team name
        #[arg(short, long)]
        name: Option<String>,
        /// New description
        #[arg(short, long)]
        description: Option<String>,
        /// New color (hex)
        #[arg(short, long = "color-hex", id = "team_color")]
        color: Option<String>,
        /// New icon
        #[arg(long)]
        icon: Option<String>,
    },
}

#[derive(Tabled)]
struct TeamRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Key")]
    key: String,
    #[tabled(rename = "ID")]
    id: String,
}

pub async fn handle(cmd: TeamCommands, output: &OutputOptions) -> Result<()> {
    match cmd {
        TeamCommands::List => list_teams(output).await,
        TeamCommands::Get { ids } => {
            let final_ids: Vec<String> = if ids.is_empty() || (ids.len() == 1 && ids[0] == "-") {
                let stdin = io::stdin();
                stdin
                    .lock()
                    .lines()
                    .map_while(Result::ok)
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| l.trim().to_string())
                    .collect()
            } else {
                ids
            };
            if final_ids.is_empty() {
                anyhow::bail!("No team IDs provided. Provide IDs or pipe them via stdin.");
            }
            get_teams(&final_ids, output).await
        }
        TeamCommands::Create {
            name,
            key,
            description,
            color,
            icon,
        } => create_team(&name, &key, description, color, icon, output).await,
        TeamCommands::Update {
            id,
            name,
            description,
            color,
            icon,
        } => update_team(&id, name, description, color, icon, output).await,
    }
}

async fn list_teams(output: &OutputOptions) -> Result<()> {
    let can_use_cache = !output.cache.no_cache
        && output.pagination.after.is_none()
        && output.pagination.before.is_none()
        && !output.pagination.all
        && output.pagination.page_size.is_none()
        && output.pagination.limit.is_none();

    let cached: Vec<Value> = if can_use_cache {
        let cache = Cache::new()?;
        cache
            .get(CacheType::Teams)
            .and_then(|data| data.as_array().cloned())
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let teams = if !cached.is_empty() {
        cached
    } else {
        let client = LinearClient::new()?;
        let pagination = output.pagination.with_default_limit(100);
        let query = r#"
            query($first: Int, $after: String, $last: Int, $before: String) {
                teams(first: $first, after: $after, last: $last, before: $before) {
                    nodes {
                        id
                        name
                        key
                    }
                    pageInfo {
                        hasNextPage
                        endCursor
                        hasPreviousPage
                        startCursor
                    }
                }
            }
        "#;

        let teams = paginate_nodes(
            &client,
            query,
            serde_json::Map::new(),
            &["data", "teams", "nodes"],
            &["data", "teams", "pageInfo"],
            &pagination,
            100,
        )
        .await?;

        if !output.cache.no_cache {
            let cache = Cache::with_ttl(output.cache.effective_ttl_seconds())?;
            let _ = cache.set(CacheType::Teams, serde_json::json!(teams.clone()));
        }

        teams
    };

    if output.is_json() || output.has_template() {
        print_json(&serde_json::json!(teams), output)?;
        return Ok(());
    }

    let mut teams = teams;
    filter_values(&mut teams, &output.filters);

    if let Some(sort_key) = output.json.sort.as_deref() {
        sort_values(&mut teams, sort_key, output.json.order);
    }

    ensure_non_empty(&teams, output)?;
    if teams.is_empty() {
        println!("No teams found.");
        return Ok(());
    }

    let width = display_options().max_width(30);
    let rows: Vec<TeamRow> = teams
        .iter()
        .map(|t| TeamRow {
            name: truncate(t["name"].as_str().unwrap_or(""), width),
            key: t["key"].as_str().unwrap_or("").to_string(),
            id: t["id"].as_str().unwrap_or("").to_string(),
        })
        .collect();

    let table = Table::new(rows).to_string();
    println!("{}", table);
    println!("\n{} teams", teams.len());

    Ok(())
}

async fn get_team(id: &str, output: &OutputOptions) -> Result<()> {
    let client = LinearClient::new()?;

    let query = r#"
        query($id: String!) {
            team(id: $id) {
                id
                name
                key
                description
                icon
                color
                private
                timezone
                issueCount
                createdAt
                updatedAt
            }
        }
    "#;

    let result = client.query(query, Some(json!({ "id": id }))).await?;
    let team = &result["data"]["team"];

    if team.is_null() {
        anyhow::bail!("Team not found: {}", id);
    }

    if output.is_json() || output.has_template() {
        print_json(team, output)?;
        return Ok(());
    }

    println!("{}", team["name"].as_str().unwrap_or("").bold());
    println!("{}", "-".repeat(40));

    println!("Key: {}", team["key"].as_str().unwrap_or("-"));

    if let Some(desc) = team["description"].as_str() {
        if !desc.is_empty() {
            println!("Description: {}", desc);
        }
    }

    println!("Private: {}", team["private"].as_bool().unwrap_or(false));

    if let Some(timezone) = team["timezone"].as_str() {
        println!("Timezone: {}", timezone);
    }

    if let Some(issue_count) = team["issueCount"].as_i64() {
        println!("Issue Count: {}", issue_count);
    }

    if let Some(color) = team["color"].as_str() {
        println!("Color: {}", color);
    }

    if let Some(icon) = team["icon"].as_str() {
        println!("Icon: {}", icon);
    }

    println!("ID: {}", team["id"].as_str().unwrap_or("-"));

    if let Some(created_at) = team["createdAt"].as_str() {
        println!("Created: {}", created_at);
    }

    if let Some(updated_at) = team["updatedAt"].as_str() {
        println!("Updated: {}", updated_at);
    }

    Ok(())
}

async fn get_teams(ids: &[String], output: &OutputOptions) -> Result<()> {
    if ids.len() == 1 {
        return get_team(&ids[0], output).await;
    }

    let client = LinearClient::new()?;

    let futures: Vec<_> = ids
        .iter()
        .map(|id| {
            let client = client.clone();
            let id = id.clone();
            async move {
                let query = r#"
                    query($id: String!) {
                        team(id: $id) {
                            id
                            name
                            key
                            private
                        }
                    }
                "#;
                let result = client.query(query, Some(json!({ "id": id }))).await;
                (id, result)
            }
        })
        .collect();

    let results = futures::future::join_all(futures).await;

    if output.is_json() || output.has_template() {
        let teams: Vec<_> = results
            .iter()
            .filter_map(|(_, r)| {
                r.as_ref().ok().and_then(|data| {
                    let team = &data["data"]["team"];
                    if !team.is_null() {
                        Some(team.clone())
                    } else {
                        None
                    }
                })
            })
            .collect();
        print_json(&serde_json::json!(teams), output)?;
        return Ok(());
    }

    let width = display_options().max_width(30);
    for (id, result) in results {
        match result {
            Ok(data) => {
                let team = &data["data"]["team"];
                if team.is_null() {
                    eprintln!("{} Team not found: {}", "!".yellow(), id);
                } else {
                    let name = truncate(team["name"].as_str().unwrap_or("-"), width);
                    let key = team["key"].as_str().unwrap_or("-");
                    let private = team["private"].as_bool().unwrap_or(false);
                    println!("{} ({}) private={} id={}", name.cyan(), key, private, id);
                }
            }
            Err(e) => {
                eprintln!("{} Error fetching {}: {}", "!".red(), id, e);
            }
        }
    }

    Ok(())
}

async fn create_team(
    name: &str,
    key: &str,
    description: Option<String>,
    color: Option<String>,
    icon: Option<String>,
    output: &OutputOptions,
) -> Result<()> {
    let client = LinearClient::new()?;

    let mut input = json!({
        "name": name,
        "key": key.to_uppercase(),
    });

    if let Some(desc) = description {
        input["description"] = json!(desc);
    }
    if let Some(c) = color {
        input["color"] = json!(c);
    }
    if let Some(i) = icon {
        input["icon"] = json!(i);
    }

    let mutation = r#"
        mutation($input: TeamCreateInput!) {
            teamCreate(input: $input) {
                success
                team { id key name description color icon }
            }
        }
    "#;

    let result = client
        .mutate(mutation, Some(json!({ "input": input })))
        .await?;

    if result["data"]["teamCreate"]["success"].as_bool() == Some(true) {
        let team = &result["data"]["teamCreate"]["team"];

        if output.is_json() || output.has_template() {
            print_json(team, output)?;
            return Ok(());
        }

        println!(
            "{} Created team: {} ({})",
            "+".green(),
            team["name"].as_str().unwrap_or(""),
            team["key"].as_str().unwrap_or("")
        );
        println!("  ID: {}", team["id"].as_str().unwrap_or(""));
    } else {
        anyhow::bail!("Failed to create team");
    }

    Ok(())
}

async fn update_team(
    id: &str,
    name: Option<String>,
    description: Option<String>,
    color: Option<String>,
    icon: Option<String>,
    output: &OutputOptions,
) -> Result<()> {
    let client = LinearClient::new()?;
    let team_id = resolve_team_id(&client, id).await?;

    let mut input = json!({});
    if let Some(n) = name {
        input["name"] = json!(n);
    }
    if let Some(d) = description {
        input["description"] = json!(d);
    }
    if let Some(c) = color {
        input["color"] = json!(c);
    }
    if let Some(i) = icon {
        input["icon"] = json!(i);
    }

    if input.as_object().map(|o| o.is_empty()).unwrap_or(true) {
        println!("No updates specified.");
        return Ok(());
    }

    let mutation = r#"
        mutation($id: String!, $input: TeamUpdateInput!) {
            teamUpdate(id: $id, input: $input) {
                success
                team { id key name description color icon }
            }
        }
    "#;

    let result = client
        .mutate(mutation, Some(json!({ "id": team_id, "input": input })))
        .await?;

    if result["data"]["teamUpdate"]["success"].as_bool() == Some(true) {
        let team = &result["data"]["teamUpdate"]["team"];

        if output.is_json() || output.has_template() {
            print_json(team, output)?;
            return Ok(());
        }

        println!(
            "{} Updated team: {} ({})",
            "+".green(),
            team["name"].as_str().unwrap_or(""),
            team["key"].as_str().unwrap_or("")
        );
    } else {
        anyhow::bail!("Failed to update team");
    }

    Ok(())
}
