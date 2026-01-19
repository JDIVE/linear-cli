use anyhow::Result;
use chrono::NaiveDate;
use clap::Subcommand;
use colored::Colorize;
use serde_json::json;
use tabled::{Table, Tabled};

use crate::api::{resolve_team_id, LinearClient};
use crate::display_options;
use crate::output::{ensure_non_empty, filter_values, print_json, sort_values, OutputOptions};
use crate::pagination::paginate_nodes;
use crate::text::truncate;

#[derive(Subcommand)]
pub enum CycleCommands {
    /// List cycles for a team
    #[command(alias = "ls")]
    List {
        /// Team ID or name
        #[arg(short, long)]
        team: String,
        /// Include completed cycles
        #[arg(short, long)]
        all: bool,
    },
    /// Show the current active cycle
    Current {
        /// Team ID or name
        #[arg(short, long)]
        team: String,
    },
    /// Create a new cycle
    #[command(after_help = r#"EXAMPLES:
    linear cycles create -t ENG --starts-at 2025-01-06 --ends-at 2025-01-20
    linear c create -t ENG --name "Sprint 12" --starts-at 2025-01-06 --ends-at 2025-01-20"#)]
    Create {
        /// Team ID or name
        #[arg(short, long)]
        team: String,
        /// Cycle name (optional)
        #[arg(short, long)]
        name: Option<String>,
        /// Cycle description (optional)
        #[arg(short, long)]
        description: Option<String>,
        /// Start date (YYYY-MM-DD or full ISO datetime)
        #[arg(long)]
        starts_at: String,
        /// End date (YYYY-MM-DD or full ISO datetime)
        #[arg(long)]
        ends_at: String,
    },
}

#[derive(Tabled)]
struct CycleRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Number")]
    number: String,
    #[tabled(rename = "Status")]
    status: String,
    #[tabled(rename = "Start Date")]
    start_date: String,
    #[tabled(rename = "End Date")]
    end_date: String,
    #[tabled(rename = "Progress")]
    progress: String,
    #[tabled(rename = "ID")]
    id: String,
}

pub async fn handle(cmd: CycleCommands, output: &OutputOptions) -> Result<()> {
    match cmd {
        CycleCommands::List { team, all } => list_cycles(&team, all, output).await,
        CycleCommands::Current { team } => current_cycle(&team, output).await,
        CycleCommands::Create {
            team,
            name,
            description,
            starts_at,
            ends_at,
        } => create_cycle(&team, name, description, &starts_at, &ends_at, output).await,
    }
}

fn normalize_datetime(value: &str, end_of_day: bool) -> Result<String> {
    if value.len() == 10 {
        if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
            let suffix = if end_of_day {
                "T23:59:59.000Z"
            } else {
                "T00:00:00.000Z"
            };
            return Ok(format!("{}{}", date.format("%Y-%m-%d"), suffix));
        }
    }
    Ok(value.to_string())
}

async fn list_cycles(team: &str, include_all: bool, output: &OutputOptions) -> Result<()> {
    let client = LinearClient::new()?;

    // Resolve team key/name to UUID
    let team_id = resolve_team_id(&client, team).await?;

    let team_query = r#"
        query($teamId: String!) {
            team(id: $teamId) {
                id
                name
            }
        }
    "#;

    let result = client
        .query(team_query, Some(json!({ "teamId": team_id })))
        .await?;
    let team_data = &result["data"]["team"];

    if team_data.is_null() {
        anyhow::bail!("Team not found: {}", team);
    }

    let team_name = team_data["name"].as_str().unwrap_or("");

    let cycles_query = r#"
        query($teamId: String!, $first: Int, $after: String, $last: Int, $before: String) {
            team(id: $teamId) {
                cycles(first: $first, after: $after, last: $last, before: $before) {
                    nodes {
                        id
                        name
                        number
                        startsAt
                        endsAt
                        completedAt
                        progress
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
    let pagination = output.pagination.with_default_limit(50);
    let cycles = paginate_nodes(
        &client,
        cycles_query,
        vars,
        &["data", "team", "cycles", "nodes"],
        &["data", "team", "cycles", "pageInfo"],
        &pagination,
        50,
    )
    .await?;

    let cycles: Vec<_> = cycles
        .into_iter()
        .filter(|c| include_all || c["completedAt"].is_null())
        .collect();

    if output.is_json() || output.has_template() {
        print_json(
            &json!({
                "team": team_name,
                "cycles": cycles
            }),
            output,
        )?;
        return Ok(());
    }

    if cycles.is_empty() {
        println!("No cycles found for team '{}'.", team_name);
        return Ok(());
    }

    let mut filtered = cycles;
    filter_values(&mut filtered, &output.filters);

    if let Some(sort_key) = output.json.sort.as_deref() {
        sort_values(&mut filtered, sort_key, output.json.order);
    }

    let width = display_options().max_width(30);
    let rows: Vec<CycleRow> = filtered
        .iter()
        .map(|c| {
            let progress = c["progress"].as_f64().unwrap_or(0.0);

            let status = if !c["completedAt"].is_null() {
                "Completed".to_string()
            } else {
                "Active".to_string()
            };

            CycleRow {
                name: truncate(c["name"].as_str().unwrap_or("-"), width),
                number: c["number"]
                    .as_i64()
                    .map(|n| n.to_string())
                    .unwrap_or("-".to_string()),
                status,
                start_date: c["startsAt"]
                    .as_str()
                    .map(|s| s.chars().take(10).collect())
                    .unwrap_or("-".to_string()),
                end_date: c["endsAt"]
                    .as_str()
                    .map(|s| s.chars().take(10).collect())
                    .unwrap_or("-".to_string()),
                progress: format!("{:.0}%", progress * 100.0),
                id: c["id"].as_str().unwrap_or("").to_string(),
            }
        })
        .collect();

    ensure_non_empty(&filtered, output)?;
    if rows.is_empty() {
        println!(
            "No active cycles found for team '{}'. Use --all to see completed cycles.",
            team_name
        );
        return Ok(());
    }

    println!("{}", format!("Cycles for team '{}'", team_name).bold());
    println!("{}", "-".repeat(40));

    let rows_len = rows.len();
    let table = Table::new(rows).to_string();
    println!("{}", table);
    println!("\n{} cycles shown", rows_len);

    Ok(())
}

async fn current_cycle(team: &str, output: &OutputOptions) -> Result<()> {
    let client = LinearClient::new()?;

    // Resolve team key/name to UUID
    let team_id = resolve_team_id(&client, team).await?;

    let query = r#"
        query($teamId: String!) {
            team(id: $teamId) {
                id
                name
                activeCycle {
                    id
                    name
                    number
                    startsAt
                    endsAt
                    progress
                    issues(first: 50) {
                        nodes {
                            id
                            identifier
                            title
                            state { name type }
                        }
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

    if output.is_json() || output.has_template() {
        print_json(team_data, output)?;
        return Ok(());
    }

    let team_name = team_data["name"].as_str().unwrap_or("");
    let cycle = &team_data["activeCycle"];

    if cycle.is_null() {
        println!("No active cycle for team '{}'.", team_name);
        return Ok(());
    }

    let progress = cycle["progress"].as_f64().unwrap_or(0.0);
    let cycle_number = cycle["number"].as_i64().unwrap_or(0);
    let default_name = format!("Cycle {}", cycle_number);
    let cycle_name = cycle["name"].as_str().unwrap_or(&default_name);

    println!("{}", format!("Current Cycle: {}", cycle_name).bold());
    println!("{}", "-".repeat(40));

    println!("Team: {}", team_name);
    println!("Cycle Number: {}", cycle_number);
    println!(
        "Start Date: {}",
        cycle["startsAt"].as_str().map(|s| &s[..10]).unwrap_or("-")
    );
    println!(
        "End Date: {}",
        cycle["endsAt"].as_str().map(|s| &s[..10]).unwrap_or("-")
    );
    println!("Progress: {:.0}%", progress * 100.0);
    println!("ID: {}", cycle["id"].as_str().unwrap_or("-"));

    // Show issues in the cycle
    let issues = cycle["issues"]["nodes"].as_array();
    if let Some(issues) = issues {
        if !issues.is_empty() {
            println!("\n{}", "Issues in this cycle:".bold());
            for issue in issues {
                let identifier = issue["identifier"].as_str().unwrap_or("");
                let title = truncate(
                    issue["title"].as_str().unwrap_or(""),
                    display_options().max_width(50),
                );
                let state = issue["state"]["name"].as_str().unwrap_or("");
                let state_type = issue["state"]["type"].as_str().unwrap_or("");

                let state_colored = match state_type {
                    "completed" => state.green().to_string(),
                    "started" => state.yellow().to_string(),
                    "canceled" | "cancelled" => state.red().to_string(),
                    _ => state.dimmed().to_string(),
                };

                println!("  {} {} [{}]", identifier.cyan(), title, state_colored);
            }
        }
    }

    Ok(())
}

async fn create_cycle(
    team: &str,
    name: Option<String>,
    description: Option<String>,
    starts_at: &str,
    ends_at: &str,
    output: &OutputOptions,
) -> Result<()> {
    let client = LinearClient::new()?;
    let team_id = resolve_team_id(&client, team).await?;

    let input = json!({
        "teamId": team_id,
        "name": name,
        "description": description,
        "startsAt": normalize_datetime(starts_at, false)?,
        "endsAt": normalize_datetime(ends_at, true)?,
    });

    let mutation = r#"
        mutation($input: CycleCreateInput!) {
            cycleCreate(input: $input) {
                success
                cycle {
                    id
                    name
                    number
                    startsAt
                    endsAt
                }
            }
        }
    "#;

    let result = client.mutate(mutation, Some(json!({ "input": input }))).await?;
    if result["data"]["cycleCreate"]["success"].as_bool() == Some(true) {
        let cycle = &result["data"]["cycleCreate"]["cycle"];
        if output.is_json() || output.has_template() {
            print_json(cycle, output)?;
            return Ok(());
        }
        println!(
            "{} Created cycle: {}",
            "+".green(),
            cycle["name"].as_str().unwrap_or("Cycle")
        );
    } else {
        anyhow::bail!("Failed to create cycle");
    }

    Ok(())
}
