use anyhow::Result;
use clap::Subcommand;
use colored::Colorize;
use serde_json::{json, Value};
use tabled::{Table, Tabled};

use crate::api::{resolve_team_id, LinearClient};
use crate::cache::{Cache, CacheType};
use crate::display_options;
use crate::input::read_ids_from_stdin;
use crate::output::{ensure_non_empty, filter_values, print_json, sort_values, OutputOptions};
use crate::pagination::paginate_nodes;
use crate::text::truncate;

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
        /// Status name(s) or ID(s). Use "-" to read from stdin.
        ids: Vec<String>,
        /// Team name or ID
        #[arg(short, long)]
        team: String,
    },
    /// Create a new workflow state
    #[command(after_help = r##"EXAMPLES:
    linear statuses create -t ENG "Ready" --type unstarted
    linear st create -t ENG "In QA" --type started -c "#F59E0B""##)]
    Create {
        /// Team name or ID
        #[arg(short, long)]
        team: String,
        /// State name
        name: String,
        /// State type (backlog, unstarted, started, completed, canceled)
        #[arg(long, default_value = "started")]
        r#type: String,
        /// State color (hex)
        #[arg(short, long = "color-hex")]
        color_hex: Option<String>,
        /// Position in the workflow
        #[arg(long)]
        position: Option<f64>,
    },
    /// Update an existing workflow state
    #[command(after_help = r##"EXAMPLES:
    linear statuses update STATE_ID -n "Ready for QA"
    linear st update STATE_ID -c "#10B981" --position 4"##)]
    Update {
        /// State ID
        id: String,
        /// New name
        #[arg(short, long)]
        name: Option<String>,
        /// New color (hex)
        #[arg(short, long = "color-hex")]
        color_hex: Option<String>,
        /// New position
        #[arg(long)]
        position: Option<f64>,
    },
    /// Archive a workflow state
    #[command(after_help = r#"EXAMPLE:
    linear statuses archive STATE_ID"#)]
    Archive {
        /// State ID
        id: String,
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

pub async fn handle(cmd: StatusCommands, output: &OutputOptions) -> Result<()> {
    match cmd {
        StatusCommands::List { team } => list_statuses(&team, output).await,
        StatusCommands::Get { ids, team } => {
            let final_ids = read_ids_from_stdin(ids);
            if final_ids.is_empty() {
                anyhow::bail!("No status IDs provided. Provide IDs or pipe them via stdin.");
            }
            get_statuses(&final_ids, &team, output).await
        }
        StatusCommands::Create {
            team,
            name,
            r#type,
            color_hex,
            position,
        } => create_status(&team, &name, &r#type, color_hex, position, output).await,
        StatusCommands::Update {
            id,
            name,
            color_hex,
            position,
        } => update_status(&id, name, color_hex, position, output).await,
        StatusCommands::Archive { id } => archive_status(&id, output).await,
    }
}

async fn list_statuses(team: &str, output: &OutputOptions) -> Result<()> {
    let client = LinearClient::new()?;

    // Resolve team key/name to UUID
    let team_id = resolve_team_id(&client, team, &output.cache).await?;

    let can_use_cache = !output.cache.no_cache
        && output.pagination.after.is_none()
        && output.pagination.before.is_none()
        && !output.pagination.all
        && output.pagination.page_size.is_none()
        && output.pagination.limit.is_none();

    let (team_name, states): (String, Vec<Value>) = if can_use_cache {
        let cache = Cache::new()?;
        if let Some(cached) = cache.get_keyed(CacheType::Statuses, &team_id) {
            let name = cached["team_name"].as_str().unwrap_or("").to_string();
            let states_data = cached["states"].as_array().cloned().unwrap_or_default();
            (name, states_data)
        } else {
            (String::new(), Vec::new())
        }
    } else {
        (String::new(), Vec::new())
    };

    let (team_name, states) = if !states.is_empty() {
        (team_name, states)
    } else {
        let team_query = r#"
            query($teamId: String!) {
                team(id: $teamId) {
                    id
                    name
                }
            }
        "#;
        let team_result = client
            .query(team_query, Some(json!({ "teamId": team_id })))
            .await?;
        let team_data = &team_result["data"]["team"];

        if team_data.is_null() {
            anyhow::bail!("Team not found: {}", team);
        }

        let name = team_data["name"].as_str().unwrap_or("").to_string();

        let states_query = r#"
            query($teamId: String!, $first: Int, $after: String, $last: Int, $before: String) {
                team(id: $teamId) {
                    states(first: $first, after: $after, last: $last, before: $before) {
                        nodes {
                            id
                            name
                            type
                            color
                            position
                            description
                        }
                        pageInfo {
                            hasNextPage
                            endCursor
                            hasPreviousPage
                            startCursor
                        }
                    }
                }
            }
        "#;

        let mut vars = serde_json::Map::new();
        vars.insert("teamId".to_string(), json!(team_id));
        let pagination = output.pagination.with_default_limit(100);
        let states = paginate_nodes(
            &client,
            states_query,
            vars,
            &["data", "team", "states", "nodes"],
            &["data", "team", "states", "pageInfo"],
            &pagination,
            100,
        )
        .await?;

        if !output.cache.no_cache {
            let cache = Cache::with_ttl(output.cache.effective_ttl_seconds())?;
            let cache_data = json!({
                "team_name": name,
                "states": states,
            });
            let _ = cache.set_keyed(CacheType::Statuses, &team_id, cache_data);
        }

        (name, states)
    };

    if output.is_json() || output.has_template() {
        print_json(
            &json!({
                "team": team_name,
                "statuses": states
            }),
            output,
        )?;
        return Ok(());
    }

    if states.is_empty() {
        println!("No statuses found for team '{}'.", team_name);
        return Ok(());
    }

    println!(
        "{}",
        format!("Issue statuses for team '{}'", team_name).bold()
    );
    println!("{}", "-".repeat(50));

    let width = display_options().max_width(30);
    let mut states = states;
    filter_values(&mut states, &output.filters);
    if let Some(sort_key) = output.json.sort.as_deref() {
        sort_values(&mut states, sort_key, output.json.order);
    }

    ensure_non_empty(&states, output)?;
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
                name: truncate(s["name"].as_str().unwrap_or(""), width),
                status_type: type_colored,
                color: s["color"].as_str().unwrap_or("").to_string(),
                position: s["position"]
                    .as_f64()
                    .map(|p| format!("{:.0}", p))
                    .unwrap_or("-".to_string()),
                id: s["id"].as_str().unwrap_or("").to_string(),
            }
        })
        .collect();

    let table = Table::new(rows).to_string();
    println!("{}", table);
    println!("\n{} statuses", states.len());

    Ok(())
}

async fn get_statuses(ids: &[String], team: &str, output: &OutputOptions) -> Result<()> {
    let client = LinearClient::new()?;

    // Resolve team key/name to UUID
    let team_id = resolve_team_id(&client, team, &output.cache).await?;

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

    let result = client
        .query(query, Some(json!({ "teamId": team_id })))
        .await?;
    let team_data = &result["data"]["team"];

    if team_data.is_null() {
        anyhow::bail!("Team not found: {}", team);
    }

    let empty = vec![];
    let states = team_data["states"]["nodes"].as_array().unwrap_or(&empty);

    let mut found: Vec<serde_json::Value> = Vec::new();
    for id in ids {
        let status = states.iter().find(|s| {
            s["id"].as_str() == Some(id.as_str())
                || s["name"].as_str().map(|n| n.to_lowercase()) == Some(id.to_lowercase())
        });

        if let Some(s) = status {
            found.push(s.clone());
        } else if !output.is_json() && !output.has_template() {
            eprintln!("{} Status not found: {}", "!".yellow(), id);
        }
    }

    if output.is_json() || output.has_template() {
        print_json(&serde_json::json!(found), output)?;
        return Ok(());
    }

    for (idx, status) in found.iter().enumerate() {
        if idx > 0 {
            println!();
        }
        println!("{}", status["name"].as_str().unwrap_or("").bold());
        println!("{}", "-".repeat(40));
        println!("Type: {}", status["type"].as_str().unwrap_or("-"));
        println!("Color: {}", status["color"].as_str().unwrap_or("-"));
        println!(
            "Position: {}",
            status["position"]
                .as_f64()
                .map(|p| format!("{:.0}", p))
                .unwrap_or("-".to_string())
        );
        if let Some(desc) = status["description"].as_str() {
            if !desc.is_empty() {
                println!("Description: {}", desc);
            }
        }
        println!("ID: {}", status["id"].as_str().unwrap_or("-"));
    }

    Ok(())
}

fn normalize_state_type(value: &str) -> Result<&'static str> {
    match value.to_lowercase().as_str() {
        "backlog" => Ok("backlog"),
        "unstarted" => Ok("unstarted"),
        "started" => Ok("started"),
        "completed" => Ok("completed"),
        "canceled" | "cancelled" => Ok("canceled"),
        _ => anyhow::bail!(
            "Invalid state type '{}'. Use backlog, unstarted, started, completed, or canceled.",
            value
        ),
    }
}

async fn create_status(
    team: &str,
    name: &str,
    state_type: &str,
    color: Option<String>,
    position: Option<f64>,
    output: &OutputOptions,
) -> Result<()> {
    let client = LinearClient::new()?;
    let team_id = resolve_team_id(&client, team, &output.cache).await?;
    let normalized_type = normalize_state_type(state_type)?;

    let mut input = json!({
        "name": name,
        "type": normalized_type,
        "teamId": team_id
    });

    if let Some(c) = color {
        input["color"] = json!(c);
    }
    if let Some(p) = position {
        input["position"] = json!(p);
    }

    let mutation = r#"
        mutation($input: WorkflowStateCreateInput!) {
            workflowStateCreate(input: $input) {
                success
                workflowState { id name type color position }
            }
        }
    "#;

    let result = client
        .mutate(mutation, Some(json!({ "input": input })))
        .await?;

    if result["data"]["workflowStateCreate"]["success"].as_bool() == Some(true) {
        let state = &result["data"]["workflowStateCreate"]["workflowState"];

        if output.is_json() || output.has_template() {
            print_json(state, output)?;
            return Ok(());
        }

        println!(
            "{} Created status: {}",
            "+".green(),
            state["name"].as_str().unwrap_or("")
        );
        println!("  ID: {}", state["id"].as_str().unwrap_or(""));
    } else {
        anyhow::bail!("Failed to create status");
    }

    Ok(())
}

async fn update_status(
    id: &str,
    name: Option<String>,
    color: Option<String>,
    position: Option<f64>,
    output: &OutputOptions,
) -> Result<()> {
    let client = LinearClient::new()?;

    let mut input = json!({});
    if let Some(n) = name {
        input["name"] = json!(n);
    }
    if let Some(c) = color {
        input["color"] = json!(c);
    }
    if let Some(p) = position {
        input["position"] = json!(p);
    }

    if input.as_object().map(|o| o.is_empty()).unwrap_or(true) {
        println!("No updates specified.");
        return Ok(());
    }

    let mutation = r#"
        mutation($id: String!, $input: WorkflowStateUpdateInput!) {
            workflowStateUpdate(id: $id, input: $input) {
                success
                workflowState { id name type color position }
            }
        }
    "#;

    let result = client
        .mutate(mutation, Some(json!({ "id": id, "input": input })))
        .await?;

    if result["data"]["workflowStateUpdate"]["success"].as_bool() == Some(true) {
        let state = &result["data"]["workflowStateUpdate"]["workflowState"];

        if output.is_json() || output.has_template() {
            print_json(state, output)?;
            return Ok(());
        }

        println!(
            "{} Updated status: {}",
            "+".green(),
            state["name"].as_str().unwrap_or("")
        );
    } else {
        anyhow::bail!("Failed to update status");
    }

    Ok(())
}

async fn archive_status(id: &str, output: &OutputOptions) -> Result<()> {
    let client = LinearClient::new()?;

    if output.dry_run {
        if output.is_json() || output.has_template() {
            print_json(
                &json!({
                    "dry_run": true,
                    "would_archive": true,
                    "id": id,
                }),
                output,
            )?;
        } else {
            println!("{}", "[DRY RUN] Would archive status:".yellow().bold());
            println!("  ID: {}", id);
        }
        return Ok(());
    }

    let mutation = r#"
        mutation($id: String!) {
            workflowStateArchive(id: $id) {
                success
            }
        }
    "#;

    let result = client.mutate(mutation, Some(json!({ "id": id }))).await?;

    if result["data"]["workflowStateArchive"]["success"].as_bool() == Some(true) {
        if output.is_json() || output.has_template() {
            print_json(&json!({ "archived": true, "id": id }), output)?;
            return Ok(());
        }

        println!("{} Status archived", "+".green());
    } else {
        anyhow::bail!("Failed to archive status");
    }

    Ok(())
}
