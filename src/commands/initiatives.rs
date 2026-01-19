use anyhow::Result;
use clap::Subcommand;
use colored::Colorize;
use serde_json::json;
use std::io::{self, BufRead};
use tabled::{Table, Tabled};

use crate::api::{
    resolve_initiative_id, resolve_project_id, resolve_user_id, LinearClient,
};
use crate::display_options;
use crate::output::{ensure_non_empty, filter_values, print_json, sort_values, OutputOptions};
use crate::pagination::{paginate_nodes, PaginationOptions};
use crate::text::truncate;

#[derive(Subcommand)]
pub enum InitiativeCommands {
    /// List initiatives
    #[command(alias = "ls")]
    #[command(after_help = r#"EXAMPLES:
    linear initiatives list                   # List all initiatives
    linear ini list --archived                # Include archived initiatives
    linear ini list --output json             # Output as JSON"#)]
    List {
        /// Include archived initiatives
        #[arg(short, long)]
        archived: bool,
    },
    /// Get initiative details
    #[command(after_help = r#"EXAMPLES:
    linear initiatives get INITIATIVE_ID      # View by ID
    linear ini get "Q1 Growth"                # View by name
    linear ini get ID1 ID2 ID3                # Get multiple initiatives
    echo "INITIATIVE_ID" | linear ini get -   # Read ID from stdin"#)]
    Get {
        /// Initiative ID(s) or name(s). Use "-" to read from stdin.
        ids: Vec<String>,
    },
    /// Create a new initiative
    #[command(after_help = r#"EXAMPLES:
    linear initiatives create "Q1 Growth"     # Create an initiative
    linear ini create "Modernize" -d "Scope"  # With description
    linear ini create "Platform" --owner me  # With owner"#)]
    Create {
        /// Initiative name
        name: String,
        /// Initiative description
        #[arg(short, long)]
        description: Option<String>,
        /// Initiative content (markdown)
        #[arg(long)]
        content: Option<String>,
        /// Owner (user ID, name, email, or "me")
        #[arg(long)]
        owner: Option<String>,
        /// Initiative status (planned, active, completed)
        #[arg(long)]
        status: Option<String>,
        /// Target date (YYYY-MM-DD)
        #[arg(long)]
        target_date: Option<String>,
        /// Initiative color (hex)
        #[arg(long = "color-hex", id = "initiative_color")]
        color: Option<String>,
        /// Initiative icon
        #[arg(long)]
        icon: Option<String>,
    },
    /// Update an initiative
    #[command(after_help = r#"EXAMPLES:
    linear initiatives update ID -n "New name"
    linear ini update ID --status active --dry-run"#)]
    Update {
        /// Initiative ID or name
        id: String,
        /// New name
        #[arg(short, long)]
        name: Option<String>,
        /// New description
        #[arg(short, long)]
        description: Option<String>,
        /// New content (markdown)
        #[arg(long)]
        content: Option<String>,
        /// New owner
        #[arg(long)]
        owner: Option<String>,
        /// New status (planned, active, completed)
        #[arg(long)]
        status: Option<String>,
        /// New target date (YYYY-MM-DD)
        #[arg(long)]
        target_date: Option<String>,
        /// New color (hex)
        #[arg(long = "color-hex", id = "initiative_color")]
        color: Option<String>,
        /// New icon
        #[arg(long)]
        icon: Option<String>,
        /// Preview without updating (dry run)
        #[arg(long)]
        dry_run: bool,
    },
    /// Archive an initiative
    #[command(after_help = r#"EXAMPLE:
    linear initiatives archive INITIATIVE_ID"#)]
    Archive {
        /// Initiative ID or name
        id: String,
    },
    /// Unarchive an initiative
    #[command(after_help = r#"EXAMPLE:
    linear initiatives unarchive INITIATIVE_ID"#)]
    Unarchive {
        /// Initiative ID or name
        id: String,
    },
    /// Link a project to an initiative
    #[command(after_help = r#"EXAMPLE:
    linear initiatives link INITIATIVE_ID PROJECT_ID"#)]
    Link {
        /// Initiative ID or name
        initiative: String,
        /// Project ID or name
        project: String,
    },
    /// Unlink a project from an initiative
    #[command(after_help = r#"EXAMPLE:
    linear initiatives unlink INITIATIVE_ID PROJECT_ID"#)]
    Unlink {
        /// Initiative ID or name
        initiative: String,
        /// Project ID or name
        project: String,
    },
}

#[derive(Tabled)]
struct InitiativeRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Status")]
    status: String,
    #[tabled(rename = "Target Date")]
    target_date: String,
    #[tabled(rename = "Owner")]
    owner: String,
    #[tabled(rename = "ID")]
    id: String,
}

pub async fn handle(cmd: InitiativeCommands, output: &OutputOptions) -> Result<()> {
    match cmd {
        InitiativeCommands::List { archived } => list_initiatives(archived, output).await,
        InitiativeCommands::Get { ids } => {
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
                anyhow::bail!("No initiative IDs provided. Provide IDs or pipe them via stdin.");
            }
            get_initiatives(&final_ids, output).await
        }
        InitiativeCommands::Create {
            name,
            description,
            content,
            owner,
            status,
            target_date,
            color,
            icon,
        } => {
            create_initiative(
                &name,
                description,
                content,
                owner,
                status,
                target_date,
                color,
                icon,
                output,
            )
            .await
        }
        InitiativeCommands::Update {
            id,
            name,
            description,
            content,
            owner,
            status,
            target_date,
            color,
            icon,
            dry_run,
        } => {
            update_initiative(
                &id,
                name,
                description,
                content,
                owner,
                status,
                target_date,
                color,
                icon,
                dry_run,
                output,
            )
            .await
        }
        InitiativeCommands::Archive { id } => archive_initiative(&id, output).await,
        InitiativeCommands::Unarchive { id } => unarchive_initiative(&id, output).await,
        InitiativeCommands::Link { initiative, project } => {
            link_project(&initiative, &project, output).await
        }
        InitiativeCommands::Unlink { initiative, project } => {
            unlink_project(&initiative, &project, output).await
        }
    }
}

fn normalize_initiative_status(status: &str) -> Result<String> {
    match status.to_lowercase().as_str() {
        "planned" => Ok("Planned".to_string()),
        "active" => Ok("Active".to_string()),
        "completed" => Ok("Completed".to_string()),
        _ => anyhow::bail!(
            "Invalid status '{}'. Use planned, active, or completed.",
            status
        ),
    }
}

async fn list_initiatives(include_archived: bool, output: &OutputOptions) -> Result<()> {
    let client = LinearClient::new()?;

    let query = r#"
        query($includeArchived: Boolean, $first: Int, $after: String, $last: Int, $before: String) {
            initiatives(
                first: $first,
                after: $after,
                last: $last,
                before: $before,
                includeArchived: $includeArchived
            ) {
                nodes {
                    id
                    name
                    status
                    targetDate
                    owner { name }
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

    let mut vars = serde_json::Map::new();
    vars.insert("includeArchived".to_string(), json!(include_archived));
    let pagination = output.pagination.with_default_limit(50);
    let mut initiatives = paginate_nodes(
        &client,
        query,
        vars,
        &["data", "initiatives", "nodes"],
        &["data", "initiatives", "pageInfo"],
        &pagination,
        50,
    )
    .await?;

    if output.is_json() || output.has_template() {
        print_json(&serde_json::json!(initiatives), output)?;
        return Ok(());
    }

    filter_values(&mut initiatives, &output.filters);
    if let Some(sort_key) = output.json.sort.as_deref() {
        sort_values(&mut initiatives, sort_key, output.json.order);
    }

    ensure_non_empty(&initiatives, output)?;
    if initiatives.is_empty() {
        println!("No initiatives found.");
        return Ok(());
    }

    let width = display_options().max_width(40);
    let rows: Vec<InitiativeRow> = initiatives
        .iter()
        .map(|i| InitiativeRow {
            name: truncate(i["name"].as_str().unwrap_or(""), width),
            status: i["status"].as_str().unwrap_or("-").to_string(),
            target_date: i["targetDate"]
                .as_str()
                .unwrap_or("-")
                .to_string(),
            owner: i["owner"]["name"].as_str().unwrap_or("-").to_string(),
            id: i["id"].as_str().unwrap_or("").to_string(),
        })
        .collect();

    let table = Table::new(rows).to_string();
    println!("{}", table);
    println!("\n{} initiatives", initiatives.len());

    Ok(())
}

async fn get_initiatives(ids: &[String], output: &OutputOptions) -> Result<()> {
    if ids.len() == 1 {
        return get_initiative(&ids[0], output).await;
    }

    let client = LinearClient::new()?;
    let futures: Vec<_> = ids
        .iter()
        .map(|id| {
            let client = client.clone();
            let id = id.clone();
            async move {
                let resolved = resolve_initiative_id(&client, &id, true).await;
                let resolved = match resolved {
                    Ok(r) => r,
                    Err(e) => return (id, Err(e)),
                };
                let query = r#"
                    query($id: String!) {
                        initiative(id: $id) {
                            id
                            name
                            description
                            status
                            targetDate
                            url
                        }
                    }
                "#;
                let result = client.query(query, Some(json!({ "id": resolved }))).await;
                (id, result)
            }
        })
        .collect();

    let results = futures::future::join_all(futures).await;

    if output.is_json() || output.has_template() {
        let initiatives: Vec<_> = results
            .iter()
            .filter_map(|(_, r)| {
                r.as_ref().ok().and_then(|data| {
                    let initiative = &data["data"]["initiative"];
                    if initiative.is_null() {
                        None
                    } else {
                        Some(initiative.clone())
                    }
                })
            })
            .collect();
        print_json(&serde_json::json!(initiatives), output)?;
        return Ok(());
    }

    for (id, result) in results {
        match result {
            Ok(data) => {
                let initiative = &data["data"]["initiative"];
                if initiative.is_null() {
                    eprintln!("{} Initiative not found: {}", "!".yellow(), id);
                    continue;
                }
                println!("{}", initiative["name"].as_str().unwrap_or("").bold());
                println!("{}", "-".repeat(40));
                if let Some(desc) = initiative["description"].as_str() {
                    println!(
                        "Description: {}",
                        desc.chars().take(100).collect::<String>()
                    );
                }
                println!(
                    "Status: {}",
                    initiative["status"].as_str().unwrap_or("-")
                );
                println!(
                    "Target Date: {}",
                    initiative["targetDate"].as_str().unwrap_or("-")
                );
                println!("URL: {}", initiative["url"].as_str().unwrap_or("-"));
                println!("ID: {}", initiative["id"].as_str().unwrap_or("-"));
                println!();
            }
            Err(e) => {
                eprintln!("{} Failed to fetch {}: {}", "!".yellow(), id, e);
            }
        }
    }

    Ok(())
}

async fn get_initiative(id: &str, output: &OutputOptions) -> Result<()> {
    let client = LinearClient::new()?;
    let resolved = resolve_initiative_id(&client, id, true).await?;

    let query = r#"
        query($id: String!) {
            initiative(id: $id) {
                id
                name
                description
                content
                status
                targetDate
                url
                owner { name }
                projects(first: 50) {
                    nodes { id name }
                }
            }
        }
    "#;

    let result = client.query(query, Some(json!({ "id": resolved }))).await?;
    let initiative = &result["data"]["initiative"];

    if initiative.is_null() {
        anyhow::bail!("Initiative not found: {}", id);
    }

    if output.is_json() || output.has_template() {
        print_json(initiative, output)?;
        return Ok(());
    }

    println!("{}", initiative["name"].as_str().unwrap_or("").bold());
    println!("{}", "-".repeat(40));

    if let Some(desc) = initiative["description"].as_str() {
        println!(
            "Description: {}",
            desc.chars().take(120).collect::<String>()
        );
    }
    println!(
        "Status: {}",
        initiative["status"].as_str().unwrap_or("-")
    );
    println!(
        "Target Date: {}",
        initiative["targetDate"].as_str().unwrap_or("-")
    );
    println!(
        "Owner: {}",
        initiative["owner"]["name"].as_str().unwrap_or("-")
    );
    println!("URL: {}", initiative["url"].as_str().unwrap_or("-"));
    println!("ID: {}", initiative["id"].as_str().unwrap_or("-"));

    let projects = initiative["projects"]["nodes"].as_array();
    if let Some(projects) = projects {
        if !projects.is_empty() {
            println!("\nProjects:");
            for project in projects {
                println!("  - {}", project["name"].as_str().unwrap_or("-"));
            }
        }
    }

    Ok(())
}

async fn create_initiative(
    name: &str,
    description: Option<String>,
    content: Option<String>,
    owner: Option<String>,
    status: Option<String>,
    target_date: Option<String>,
    color: Option<String>,
    icon: Option<String>,
    output: &OutputOptions,
) -> Result<()> {
    let client = LinearClient::new()?;

    let mut input = json!({ "name": name });
    if let Some(desc) = description {
        input["description"] = json!(desc);
    }
    if let Some(body) = content {
        input["content"] = json!(body);
    }
    if let Some(owner) = owner {
        let owner_id = resolve_user_id(&client, &owner).await?;
        input["ownerId"] = json!(owner_id);
    }
    if let Some(status) = status {
        input["status"] = json!(normalize_initiative_status(&status)?);
    }
    if let Some(target) = target_date {
        input["targetDate"] = json!(target);
    }
    if let Some(color) = color {
        input["color"] = json!(color);
    }
    if let Some(icon) = icon {
        input["icon"] = json!(icon);
    }

    let mutation = r#"
        mutation($input: InitiativeCreateInput!) {
            initiativeCreate(input: $input) {
                success
                initiative {
                    id
                    name
                    status
                }
            }
        }
    "#;

    let result = client.mutate(mutation, Some(json!({ "input": input }))).await?;

    if result["data"]["initiativeCreate"]["success"].as_bool() == Some(true) {
        let initiative = &result["data"]["initiativeCreate"]["initiative"];
        if output.is_json() || output.has_template() {
            print_json(initiative, output)?;
            return Ok(());
        }
        println!(
            "{} Created initiative: {}",
            "+".green(),
            initiative["name"].as_str().unwrap_or("")
        );
    } else {
        anyhow::bail!("Failed to create initiative");
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn update_initiative(
    id: &str,
    name: Option<String>,
    description: Option<String>,
    content: Option<String>,
    owner: Option<String>,
    status: Option<String>,
    target_date: Option<String>,
    color: Option<String>,
    icon: Option<String>,
    dry_run: bool,
    output: &OutputOptions,
) -> Result<()> {
    let client = LinearClient::new()?;
    let initiative_id = resolve_initiative_id(&client, id, true).await?;

    let mut input = json!({});
    if let Some(name) = name {
        input["name"] = json!(name);
    }
    if let Some(desc) = description {
        input["description"] = json!(desc);
    }
    if let Some(body) = content {
        input["content"] = json!(body);
    }
    if let Some(owner) = owner {
        let owner_id = resolve_user_id(&client, &owner).await?;
        input["ownerId"] = json!(owner_id);
    }
    if let Some(status) = status {
        input["status"] = json!(normalize_initiative_status(&status)?);
    }
    if let Some(target) = target_date {
        input["targetDate"] = json!(target);
    }
    if let Some(color) = color {
        input["color"] = json!(color);
    }
    if let Some(icon) = icon {
        input["icon"] = json!(icon);
    }

    if input.as_object().map(|o| o.is_empty()).unwrap_or(true) {
        println!("No updates specified.");
        return Ok(());
    }

    if dry_run || output.dry_run {
        if output.is_json() || output.has_template() {
            print_json(
                &json!({
                    "dry_run": true,
                    "would_update": {
                        "id": id,
                        "input": input,
                    }
                }),
                output,
            )?;
        } else {
            println!("{}", "[DRY RUN] Would update initiative:".yellow().bold());
            println!("  ID: {}", id);
        }
        return Ok(());
    }

    let mutation = r#"
        mutation($id: String!, $input: InitiativeUpdateInput!) {
            initiativeUpdate(id: $id, input: $input) {
                success
                initiative {
                    id
                    name
                    status
                }
            }
        }
    "#;

    let result = client
        .mutate(
            mutation,
            Some(json!({ "id": initiative_id, "input": input })),
        )
        .await?;

    if result["data"]["initiativeUpdate"]["success"].as_bool() == Some(true) {
        let initiative = &result["data"]["initiativeUpdate"]["initiative"];
        if output.is_json() || output.has_template() {
            print_json(initiative, output)?;
            return Ok(());
        }
        println!(
            "{} Updated initiative: {}",
            "+".green(),
            initiative["name"].as_str().unwrap_or("")
        );
    } else {
        anyhow::bail!("Failed to update initiative");
    }

    Ok(())
}

async fn archive_initiative(id: &str, output: &OutputOptions) -> Result<()> {
    let client = LinearClient::new()?;
    let initiative_id = resolve_initiative_id(&client, id, true).await?;

    if output.dry_run {
        if output.is_json() || output.has_template() {
            print_json(
                &json!({
                    "dry_run": true,
                    "would_archive": true,
                    "id": initiative_id,
                }),
                output,
            )?;
        } else {
            println!("{}", "[DRY RUN] Would archive initiative:".yellow().bold());
            println!("  ID: {}", id);
        }
        return Ok(());
    }

    let mutation = r#"
        mutation($id: String!) {
            initiativeArchive(id: $id) {
                success
                entity {
                    id
                    name
                }
            }
        }
    "#;

    let result = client.mutate(mutation, Some(json!({ "id": initiative_id }))).await?;
    if result["data"]["initiativeArchive"]["success"].as_bool() == Some(true) {
        let entity = &result["data"]["initiativeArchive"]["entity"];
        if output.is_json() || output.has_template() {
            print_json(entity, output)?;
            return Ok(());
        }
        println!(
            "{} Initiative archived: {}",
            "+".green(),
            entity["name"].as_str().unwrap_or("")
        );
    } else {
        anyhow::bail!("Failed to archive initiative");
    }

    Ok(())
}

async fn unarchive_initiative(id: &str, output: &OutputOptions) -> Result<()> {
    let client = LinearClient::new()?;
    let initiative_id = resolve_initiative_id(&client, id, true).await?;

    if output.dry_run {
        if output.is_json() || output.has_template() {
            print_json(
                &json!({
                    "dry_run": true,
                    "would_archive": false,
                    "id": initiative_id,
                }),
                output,
            )?;
        } else {
            println!("{}", "[DRY RUN] Would unarchive initiative:".yellow().bold());
            println!("  ID: {}", id);
        }
        return Ok(());
    }

    let mutation = r#"
        mutation($id: String!) {
            initiativeUnarchive(id: $id) {
                success
                entity {
                    id
                    name
                }
            }
        }
    "#;

    let result = client
        .mutate(mutation, Some(json!({ "id": initiative_id })))
        .await?;
    if result["data"]["initiativeUnarchive"]["success"].as_bool() == Some(true) {
        let entity = &result["data"]["initiativeUnarchive"]["entity"];
        if output.is_json() || output.has_template() {
            print_json(entity, output)?;
            return Ok(());
        }
        println!(
            "{} Initiative unarchived: {}",
            "+".green(),
            entity["name"].as_str().unwrap_or("")
        );
    } else {
        anyhow::bail!("Failed to unarchive initiative");
    }

    Ok(())
}

async fn link_project(initiative: &str, project: &str, output: &OutputOptions) -> Result<()> {
    let client = LinearClient::new()?;
    let initiative_id = resolve_initiative_id(&client, initiative, true).await?;
    let project_id = resolve_project_id(&client, project, true).await?;

    let mutation = r#"
        mutation($input: InitiativeToProjectCreateInput!) {
            initiativeToProjectCreate(input: $input) {
                success
                initiativeToProject {
                    id
                    initiative { id }
                    project { id }
                }
            }
        }
    "#;

    let input = json!({
        "initiativeId": initiative_id,
        "projectId": project_id
    });

    let result = client.mutate(mutation, Some(json!({ "input": input }))).await?;
    if result["data"]["initiativeToProjectCreate"]["success"].as_bool() == Some(true) {
        let link = &result["data"]["initiativeToProjectCreate"]["initiativeToProject"];
        if output.is_json() || output.has_template() {
            print_json(link, output)?;
            return Ok(());
        }
        println!("{} Project linked to initiative", "+".green());
    } else {
        anyhow::bail!("Failed to link project to initiative");
    }

    Ok(())
}

async fn unlink_project(initiative: &str, project: &str, output: &OutputOptions) -> Result<()> {
    let client = LinearClient::new()?;
    let initiative_id = resolve_initiative_id(&client, initiative, true).await?;
    let project_id = resolve_project_id(&client, project, true).await?;
    let link_id = resolve_initiative_to_project_id(&client, &initiative_id, &project_id).await?;

    let mutation = r#"
        mutation($id: String!) {
            initiativeToProjectDelete(id: $id) {
                success
            }
        }
    "#;

    let result = client.mutate(mutation, Some(json!({ "id": link_id }))).await?;
    if result["data"]["initiativeToProjectDelete"]["success"].as_bool() == Some(true) {
        if output.is_json() || output.has_template() {
            print_json(&json!({ "unlinked": true, "initiativeId": initiative_id, "projectId": project_id }), output)?;
            return Ok(());
        }
        println!("{} Project unlinked from initiative", "+".green());
    } else {
        anyhow::bail!("Failed to unlink project from initiative");
    }

    Ok(())
}

async fn resolve_initiative_to_project_id(
    client: &LinearClient,
    initiative_id: &str,
    project_id: &str,
) -> Result<String> {
    let query = r#"
        query($projectId: String!, $first: Int, $after: String, $last: Int, $before: String) {
            project(id: $projectId) {
                initiativeToProjects(first: $first, after: $after, last: $last, before: $before) {
                    nodes {
                        id
                        initiative { id }
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
    vars.insert("projectId".to_string(), json!(project_id));
    let pagination = PaginationOptions {
        limit: Some(200),
        ..Default::default()
    };
    let nodes = paginate_nodes(
        client,
        query,
        vars,
        &["data", "project", "initiativeToProjects", "nodes"],
        &["data", "project", "initiativeToProjects", "pageInfo"],
        &pagination,
        100,
    )
    .await?;

    for node in nodes {
        if node["initiative"]["id"].as_str() == Some(initiative_id) {
            if let Some(id) = node["id"].as_str() {
                return Ok(id.to_string());
            }
        }
    }

    anyhow::bail!("Project is not linked to the specified initiative");
}
