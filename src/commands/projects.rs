use anyhow::Result;
use clap::Subcommand;
use colored::Colorize;
use serde_json::json;
use std::io::{self, BufRead};
use tabled::{Table, Tabled};

use crate::api::{resolve_project_id, resolve_project_status_id, resolve_team_id, LinearClient};
use crate::display_options;
use crate::output::{ensure_non_empty, filter_values, print_json, sort_values, OutputOptions};
use crate::pagination::paginate_nodes;
use crate::text::truncate;

#[derive(Subcommand)]
pub enum ProjectCommands {
    /// List all projects
    #[command(alias = "ls")]
    #[command(after_help = r#"EXAMPLES:
    linear projects list                       # List all projects
    linear p list --archived                   # Include archived projects
    linear p list --output json                # Output as JSON"#)]
    List {
        /// Show archived projects
        #[arg(short, long)]
        archived: bool,
    },
    /// Get project details
    #[command(after_help = r#"EXAMPLES:
    linear projects get PROJECT_ID             # View by ID
    linear p get "Q1 Roadmap"                  # View by name
    linear p get PROJECT_ID --output json      # Output as JSON
    linear p get ID1 ID2 ID3                   # Get multiple projects
    echo "PROJECT_ID" | linear p get -         # Read ID from stdin"#)]
    Get {
        /// Project ID(s) or name(s). Use "-" to read from stdin.
        ids: Vec<String>,
    },
    /// Create a new project
    #[command(after_help = r##"EXAMPLES:
    linear projects create "Q1 Roadmap" -t ENG # Create project
    linear p create "Feature" -t ENG -d "Desc" # With description
    linear p create "UI" -t ENG -c "#FF5733"   # With color"##)]
    Create {
        /// Project name
        name: String,
        /// Team name or ID
        #[arg(short, long)]
        team: String,
        /// Project summary (short description)
        #[arg(short, long)]
        description: Option<String>,
        /// Project content (long-form markdown)
        #[arg(long)]
        content: Option<String>,
        /// Project color (hex)
        #[arg(short, long)]
        color: Option<String>,
        /// Project start date (YYYY-MM-DD)
        #[arg(long)]
        start_date: Option<String>,
        /// Project target date (YYYY-MM-DD)
        #[arg(long)]
        target_date: Option<String>,
        /// Project status name or type (planned, started, paused, completed, canceled)
        #[arg(long)]
        status: Option<String>,
    },
    /// Update a project
    #[command(after_help = r#"EXAMPLES:
    linear projects update ID -n "New Name"    # Rename project
    linear p update ID -d "New description"    # Update description"#)]
    Update {
        /// Project ID or name
        id: String,
        /// New name
        #[arg(short, long)]
        name: Option<String>,
        /// New summary (short description)
        #[arg(short, long)]
        description: Option<String>,
        /// New content (long-form markdown)
        #[arg(long)]
        content: Option<String>,
        /// New color (hex)
        #[arg(short, long)]
        color: Option<String>,
        /// New icon
        #[arg(short, long)]
        icon: Option<String>,
        /// New start date (YYYY-MM-DD)
        #[arg(long)]
        start_date: Option<String>,
        /// New target date (YYYY-MM-DD)
        #[arg(long)]
        target_date: Option<String>,
        /// New project status name or type
        #[arg(long)]
        status: Option<String>,
        /// Preview without updating (dry run)
        #[arg(long)]
        dry_run: bool,
    },
    /// Delete a project
    #[command(after_help = r#"EXAMPLES:
    linear projects delete PROJECT_ID          # Delete with confirmation
    linear p delete PROJECT_ID --force         # Delete without confirmation"#)]
    Delete {
        /// Project ID
        id: String,
        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },
    /// Add labels to a project
    #[command(after_help = r#"EXAMPLES:
    linear projects add-labels ID LABEL_ID     # Add one label
    linear p add-labels ID L1 L2 L3            # Add multiple labels"#)]
    AddLabels {
        /// Project ID
        id: String,
        /// Label IDs to add
        #[arg(required = true)]
        labels: Vec<String>,
    },
    /// Archive a project
    #[command(after_help = r#"EXAMPLE:
    linear projects archive PROJECT_ID"#)]
    Archive {
        /// Project ID or name
        id: String,
    },
    /// Unarchive a project
    #[command(after_help = r#"EXAMPLE:
    linear projects unarchive PROJECT_ID"#)]
    Unarchive {
        /// Project ID or name
        id: String,
    },
    /// Manage project updates (status posts)
    #[command(alias = "updates")]
    Updates {
        #[command(subcommand)]
        action: ProjectUpdateCommands,
    },
}

#[derive(Subcommand)]
pub enum ProjectUpdateCommands {
    /// List project updates
    #[command(alias = "ls")]
    List {
        /// Project ID or name
        project: String,
    },
    /// Create a project update
    Create {
        /// Project ID or name
        project: String,
        /// Update body (Markdown supported). Use "-" to read from stdin.
        #[arg(short, long)]
        body: String,
        /// Health (onTrack, atRisk, offTrack)
        #[arg(long)]
        health: Option<String>,
    },
}

#[derive(Tabled)]
struct ProjectRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Status")]
    status: String,
    #[tabled(rename = "Labels")]
    labels: String,
    #[tabled(rename = "ID")]
    id: String,
}

#[derive(Tabled)]
struct ProjectUpdateRow {
    #[tabled(rename = "Created")]
    created_at: String,
    #[tabled(rename = "Health")]
    health: String,
    #[tabled(rename = "Author")]
    author: String,
    #[tabled(rename = "Body")]
    body: String,
    #[tabled(rename = "ID")]
    id: String,
}

pub async fn handle(cmd: ProjectCommands, output: &OutputOptions) -> Result<()> {
    match cmd {
        ProjectCommands::List { archived } => list_projects(archived, output).await,
        ProjectCommands::Get { ids } => {
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
                anyhow::bail!("No project IDs provided. Provide IDs or pipe them via stdin.");
            }
            get_projects(&final_ids, output).await
        }
        ProjectCommands::Create {
            name,
            team,
            description,
            content,
            color,
            start_date,
            target_date,
            status,
        } => {
            create_project(
                &name,
                &team,
                description,
                content,
                color,
                start_date,
                target_date,
                status,
                output,
            )
            .await
        }
        ProjectCommands::Update {
            id,
            name,
            description,
            content,
            color,
            icon,
            start_date,
            target_date,
            status,
            dry_run,
        } => {
            let dry_run = dry_run || output.dry_run;
            update_project(
                &id,
                name,
                description,
                content,
                color,
                icon,
                start_date,
                target_date,
                status,
                dry_run,
                output,
            )
            .await
        }
        ProjectCommands::Delete { id, force } => delete_project(&id, force).await,
        ProjectCommands::AddLabels { id, labels } => add_labels(&id, labels, output).await,
        ProjectCommands::Archive { id } => archive_project(&id, output).await,
        ProjectCommands::Unarchive { id } => unarchive_project(&id, output).await,
        ProjectCommands::Updates { action } => handle_project_updates(action, output).await,
    }
}

async fn handle_project_updates(cmd: ProjectUpdateCommands, output: &OutputOptions) -> Result<()> {
    match cmd {
        ProjectUpdateCommands::List { project } => list_project_updates(&project, output).await,
        ProjectUpdateCommands::Create {
            project,
            body,
            health,
        } => create_project_update(&project, &body, health, output).await,
    }
}

fn normalize_project_health(value: &str) -> Result<&'static str> {
    match value.to_lowercase().as_str() {
        "ontrack" | "on-track" | "on_track" => Ok("onTrack"),
        "atrisk" | "at-risk" | "at_risk" => Ok("atRisk"),
        "offtrack" | "off-track" | "off_track" => Ok("offTrack"),
        _ => anyhow::bail!(
            "Invalid health '{}'. Use onTrack, atRisk, or offTrack.",
            value
        ),
    }
}

async fn list_project_updates(project: &str, output: &OutputOptions) -> Result<()> {
    let client = LinearClient::new()?;
    let project_id = resolve_project_id(&client, project, true).await?;

    let meta_query = r#"
        query($id: String!) {
            project(id: $id) { id name }
        }
    "#;
    let meta_result = client
        .query(meta_query, Some(json!({ "id": project_id })))
        .await?;
    let project_data = &meta_result["data"]["project"];

    if project_data.is_null() {
        anyhow::bail!("Project not found: {}", project);
    }

    let query = r#"
        query($id: String!, $first: Int, $after: String, $last: Int, $before: String) {
            project(id: $id) {
                projectUpdates(first: $first, after: $after, last: $last, before: $before) {
                    nodes {
                        id
                        body
                        health
                        createdAt
                        updatedAt
                        url
                        user { name }
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
    vars.insert("id".to_string(), json!(project_id));
    let pagination = output.pagination.with_default_limit(50);
    let mut updates = paginate_nodes(
        &client,
        query,
        vars,
        &["data", "project", "projectUpdates", "nodes"],
        &["data", "project", "projectUpdates", "pageInfo"],
        &pagination,
        50,
    )
    .await?;

    if output.is_json() || output.has_template() {
        print_json(
            &json!({
                "project": project_data,
                "updates": updates
            }),
            output,
        )?;
        return Ok(());
    }

    filter_values(&mut updates, &output.filters);
    if let Some(sort_key) = output.json.sort.as_deref() {
        sort_values(&mut updates, sort_key, output.json.order);
    }

    ensure_non_empty(&updates, output)?;
    if updates.is_empty() {
        println!("No updates found for project.");
        return Ok(());
    }

    let width = display_options().max_width(60);
    let rows: Vec<ProjectUpdateRow> = updates
        .iter()
        .map(|u| ProjectUpdateRow {
            created_at: u["createdAt"].as_str().unwrap_or("-").to_string(),
            health: u["health"].as_str().unwrap_or("-").to_string(),
            author: u["user"]["name"].as_str().unwrap_or("-").to_string(),
            body: truncate(u["body"].as_str().unwrap_or(""), width),
            id: u["id"].as_str().unwrap_or("").to_string(),
        })
        .collect();

    println!(
        "{}",
        format!(
            "Project updates for {}",
            project_data["name"].as_str().unwrap_or("")
        )
        .bold()
    );
    println!("{}", "-".repeat(50));
    let table = Table::new(rows).to_string();
    println!("{}", table);
    println!("\n{} updates", updates.len());

    Ok(())
}

async fn create_project_update(
    project: &str,
    body: &str,
    health: Option<String>,
    output: &OutputOptions,
) -> Result<()> {
    let client = LinearClient::new()?;
    let project_id = resolve_project_id(&client, project, true).await?;

    let final_body = if body == "-" {
        let stdin = io::stdin();
        let lines: Vec<String> = stdin.lock().lines().map_while(Result::ok).collect();
        lines.join("\n")
    } else {
        body.to_string()
    };

    let mut input = json!({
        "projectId": project_id,
        "body": final_body
    });

    if let Some(h) = health {
        let normalized = normalize_project_health(&h)?;
        input["health"] = json!(normalized);
    }

    let mutation = r#"
        mutation($input: ProjectUpdateCreateInput!) {
            projectUpdateCreate(input: $input) {
                success
                projectUpdate { id body health url createdAt }
            }
        }
    "#;

    let result = client
        .mutate(mutation, Some(json!({ "input": input })))
        .await?;

    if result["data"]["projectUpdateCreate"]["success"].as_bool() == Some(true) {
        let update = &result["data"]["projectUpdateCreate"]["projectUpdate"];

        if output.is_json() || output.has_template() {
            print_json(update, output)?;
            return Ok(());
        }

        println!("{} Project update created", "+".green());
        println!("  ID: {}", update["id"].as_str().unwrap_or(""));
        println!("  URL: {}", update["url"].as_str().unwrap_or(""));
    } else {
        anyhow::bail!("Failed to create project update");
    }

    Ok(())
}

async fn list_projects(include_archived: bool, output: &OutputOptions) -> Result<()> {
    let client = LinearClient::new()?;

    // Simplified query to reduce GraphQL complexity (was exceeding 10000 limit)
    let query = r#"
        query($includeArchived: Boolean, $first: Int, $after: String, $last: Int, $before: String) {
            projects(first: $first, after: $after, last: $last, before: $before, includeArchived: $includeArchived) {
                nodes {
                    id
                    name
                    state
                    url
                    startDate
                    targetDate
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
    let mut projects = paginate_nodes(
        &client,
        query,
        vars,
        &["data", "projects", "nodes"],
        &["data", "projects", "pageInfo"],
        &pagination,
        50,
    )
    .await?;

    if output.is_json() || output.has_template() {
        print_json(&serde_json::json!(projects), output)?;
        return Ok(());
    }

    filter_values(&mut projects, &output.filters);
    if let Some(sort_key) = output.json.sort.as_deref() {
        sort_values(&mut projects, sort_key, output.json.order);
    }

    ensure_non_empty(&projects, output)?;
    if projects.is_empty() {
        println!("No projects found.");
        return Ok(());
    }

    let width = display_options().max_width(50);
    let rows: Vec<ProjectRow> = projects
        .iter()
        .map(|p| ProjectRow {
            name: truncate(p["name"].as_str().unwrap_or(""), width),
            status: p["state"].as_str().unwrap_or("-").to_string(),
            labels: "-".to_string(),
            id: p["id"].as_str().unwrap_or("").to_string(),
        })
        .collect();

    let table = Table::new(rows).to_string();
    println!("{}", table);
    println!("\n{} projects", projects.len());

    Ok(())
}

async fn get_project(id: &str, output: &OutputOptions) -> Result<()> {
    let client = LinearClient::new()?;

    let query = r#"
        query($id: String!) {
            project(id: $id) {
                id
                name
                description
                icon
                color
                url
                status { name }
                labels { nodes { id name color parent { name } } }
            }
        }
    "#;

    let result = client.query(query, Some(json!({ "id": id }))).await?;
    let project = &result["data"]["project"];

    if project.is_null() {
        anyhow::bail!("Project not found: {}", id);
    }

    // Handle JSON output
    if output.is_json() || output.has_template() {
        print_json(project, output)?;
        return Ok(());
    }

    println!("{}", project["name"].as_str().unwrap_or("").bold());
    println!("{}", "-".repeat(40));

    if let Some(desc) = project["description"].as_str() {
        println!(
            "Description: {}",
            desc.chars().take(100).collect::<String>()
        );
    }

    println!(
        "Status: {}",
        project["status"]["name"].as_str().unwrap_or("-")
    );
    println!("Color: {}", project["color"].as_str().unwrap_or("-"));
    println!("Icon: {}", project["icon"].as_str().unwrap_or("-"));
    println!("URL: {}", project["url"].as_str().unwrap_or("-"));
    println!("ID: {}", project["id"].as_str().unwrap_or("-"));

    let labels = project["labels"]["nodes"].as_array();
    if let Some(labels) = labels {
        if !labels.is_empty() {
            println!("\nLabels:");
            for label in labels {
                let parent = label["parent"]["name"].as_str().unwrap_or("");
                let name = label["name"].as_str().unwrap_or("");
                if parent.is_empty() {
                    println!("  - {}", name);
                } else {
                    println!("  - {} > {}", parent.dimmed(), name);
                }
            }
        }
    }

    Ok(())
}

async fn get_projects(ids: &[String], output: &OutputOptions) -> Result<()> {
    if ids.len() == 1 {
        return get_project(&ids[0], output).await;
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
                        project(id: $id) {
                            id
                            name
                            description
                            status { name }
                            url
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
        let projects: Vec<_> = results
            .iter()
            .filter_map(|(_, r)| {
                r.as_ref().ok().and_then(|data| {
                    let project = &data["data"]["project"];
                    if !project.is_null() {
                        Some(project.clone())
                    } else {
                        None
                    }
                })
            })
            .collect();
        print_json(&serde_json::json!(projects), output)?;
        return Ok(());
    }

    let width = display_options().max_width(50);
    for (id, result) in results {
        match result {
            Ok(data) => {
                let project = &data["data"]["project"];
                if project.is_null() {
                    eprintln!("{} Project not found: {}", "!".yellow(), id);
                } else {
                    let name = truncate(project["name"].as_str().unwrap_or("-"), width);
                    let status = project["status"]["name"].as_str().unwrap_or("-");
                    println!("{} [{}] {}", name.cyan(), status, id);
                }
            }
            Err(e) => {
                eprintln!("{} Error fetching {}: {}", "!".red(), id, e);
            }
        }
    }

    Ok(())
}

async fn create_project(
    name: &str,
    team: &str,
    description: Option<String>,
    content: Option<String>,
    color: Option<String>,
    start_date: Option<String>,
    target_date: Option<String>,
    status: Option<String>,
    output: &OutputOptions,
) -> Result<()> {
    let client = LinearClient::new()?;

    // Resolve team key/name to UUID
    let team_id = resolve_team_id(&client, team).await?;

    let mut input = json!({
        "name": name,
        "teamIds": [team_id]
    });

    if let Some(desc) = description {
        input["description"] = json!(desc);
    }
    if let Some(body) = content {
        input["content"] = json!(body);
    }
    if let Some(c) = color {
        input["color"] = json!(c);
    }
    if let Some(start) = start_date {
        input["startDate"] = json!(start);
    }
    if let Some(target) = target_date {
        input["targetDate"] = json!(target);
    }
    if let Some(status_name) = status {
        let status_id = resolve_project_status_id(&client, &status_name).await?;
        input["statusId"] = json!(status_id);
    }

    let mutation = r#"
        mutation($input: ProjectCreateInput!) {
            projectCreate(input: $input) {
                success
                project { id name url }
            }
        }
    "#;

    let result = client
        .mutate(mutation, Some(json!({ "input": input })))
        .await?;

    if result["data"]["projectCreate"]["success"].as_bool() == Some(true) {
        let project = &result["data"]["projectCreate"]["project"];

        // Handle JSON output
        if output.is_json() || output.has_template() {
            print_json(project, output)?;
            return Ok(());
        }

        println!(
            "{} Created project: {}",
            "+".green(),
            project["name"].as_str().unwrap_or("")
        );
        println!("  ID: {}", project["id"].as_str().unwrap_or(""));
        println!("  URL: {}", project["url"].as_str().unwrap_or(""));
    } else {
        anyhow::bail!("Failed to create project");
    }

    Ok(())
}

async fn update_project(
    id: &str,
    name: Option<String>,
    description: Option<String>,
    content: Option<String>,
    color: Option<String>,
    icon: Option<String>,
    start_date: Option<String>,
    target_date: Option<String>,
    status: Option<String>,
    dry_run: bool,
    output: &OutputOptions,
) -> Result<()> {
    let client = LinearClient::new()?;
    let project_id = resolve_project_id(&client, id, true).await?;

    let mut input = json!({});
    if let Some(n) = name {
        input["name"] = json!(n);
    }
    if let Some(d) = description {
        input["description"] = json!(d);
    }
    if let Some(body) = content {
        input["content"] = json!(body);
    }
    if let Some(c) = color {
        input["color"] = json!(c);
    }
    if let Some(i) = icon {
        input["icon"] = json!(i);
    }
    if let Some(start) = start_date {
        input["startDate"] = json!(start);
    }
    if let Some(target) = target_date {
        input["targetDate"] = json!(target);
    }
    if let Some(status_name) = status {
        let status_id = resolve_project_status_id(&client, &status_name).await?;
        input["statusId"] = json!(status_id);
    }

    if input.as_object().map(|o| o.is_empty()).unwrap_or(true) {
        println!("No updates specified.");
        return Ok(());
    }

    if dry_run {
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
            println!("{}", "[DRY RUN] Would update project:".yellow().bold());
            println!("  ID: {}", id);
        }
        return Ok(());
    }

    let mutation = r#"
        mutation($id: String!, $input: ProjectUpdateInput!) {
            projectUpdate(id: $id, input: $input) {
                success
                project { id name }
            }
        }
    "#;

    let result = client
        .mutate(mutation, Some(json!({ "id": project_id, "input": input })))
        .await?;

    if result["data"]["projectUpdate"]["success"].as_bool() == Some(true) {
        let project = &result["data"]["projectUpdate"]["project"];

        // Handle JSON output
        if output.is_json() || output.has_template() {
            print_json(project, output)?;
            return Ok(());
        }

        println!("{} Project updated", "+".green());
    } else {
        anyhow::bail!("Failed to update project");
    }

    Ok(())
}

async fn archive_project(id: &str, output: &OutputOptions) -> Result<()> {
    let client = LinearClient::new()?;
    let project_id = resolve_project_id(&client, id, true).await?;

    let mutation = r#"
        mutation($id: String!) {
            projectArchive(id: $id) {
                success
            }
        }
    "#;

    let result = client
        .mutate(mutation, Some(json!({ "id": project_id })))
        .await?;

    if result["data"]["projectArchive"]["success"].as_bool() == Some(true) {
        if output.is_json() || output.has_template() {
            print_json(&json!({ "archived": true, "id": project_id }), output)?;
            return Ok(());
        }

        println!("{} Project archived", "+".green());
    } else {
        anyhow::bail!("Failed to archive project");
    }

    Ok(())
}

async fn unarchive_project(id: &str, output: &OutputOptions) -> Result<()> {
    let client = LinearClient::new()?;
    let project_id = resolve_project_id(&client, id, true).await?;

    let mutation = r#"
        mutation($id: String!) {
            projectUnarchive(id: $id) {
                success
            }
        }
    "#;

    let result = client
        .mutate(mutation, Some(json!({ "id": project_id })))
        .await?;

    if result["data"]["projectUnarchive"]["success"].as_bool() == Some(true) {
        if output.is_json() || output.has_template() {
            print_json(&json!({ "archived": false, "id": project_id }), output)?;
            return Ok(());
        }

        println!("{} Project unarchived", "+".green());
    } else {
        anyhow::bail!("Failed to unarchive project");
    }

    Ok(())
}

async fn delete_project(id: &str, force: bool) -> Result<()> {
    if !force {
        println!("Are you sure you want to delete project {}?", id);
        println!("This action cannot be undone. Use --force to skip this prompt.");
        return Ok(());
    }

    let client = LinearClient::new()?;
    let project_id = resolve_project_id(&client, id, true).await?;

    let mutation = r#"
        mutation($id: String!) {
            projectDelete(id: $id) {
                success
            }
        }
    "#;

    let result = client
        .mutate(mutation, Some(json!({ "id": project_id })))
        .await?;

    if result["data"]["projectDelete"]["success"].as_bool() == Some(true) {
        println!("{} Project deleted", "+".green());
    } else {
        anyhow::bail!("Failed to delete project");
    }

    Ok(())
}

async fn add_labels(id: &str, label_ids: Vec<String>, output: &OutputOptions) -> Result<()> {
    let client = LinearClient::new()?;
    let project_id = resolve_project_id(&client, id, true).await?;

    let mutation = r#"
        mutation($id: String!, $input: ProjectUpdateInput!) {
            projectUpdate(id: $id, input: $input) {
                success
                project {
                    name
                    labels { nodes { name } }
                }
            }
        }
    "#;

    let input = json!({ "labelIds": label_ids });
    let result = client
        .mutate(mutation, Some(json!({ "id": project_id, "input": input })))
        .await?;

    if result["data"]["projectUpdate"]["success"].as_bool() == Some(true) {
        let project = &result["data"]["projectUpdate"]["project"];

        // Handle JSON output
        if output.is_json() || output.has_template() {
            print_json(project, output)?;
            return Ok(());
        }

        let empty = vec![];
        let labels: Vec<&str> = project["labels"]["nodes"]
            .as_array()
            .unwrap_or(&empty)
            .iter()
            .filter_map(|l| l["name"].as_str())
            .collect();
        println!("{} Labels updated: {}", "+".green(), labels.join(", "));
    } else {
        anyhow::bail!("Failed to add labels");
    }

    Ok(())
}
