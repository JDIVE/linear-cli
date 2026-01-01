use anyhow::Result;
use clap::Subcommand;
use colored::Colorize;
use serde_json::json;
use tabled::{Table, Tabled};

use crate::api::{resolve_team_id, LinearClient};
use crate::OutputFormat;

#[derive(Subcommand)]
pub enum ProjectCommands {
    /// List all projects
    #[command(alias = "ls")]
    List {
        /// Show archived projects
        #[arg(short, long)]
        archived: bool,
    },
    /// Get project details
    Get {
        /// Project ID or name
        id: String,
    },
    /// Create a new project
    Create {
        /// Project name
        name: String,
        /// Team name or ID
        #[arg(short, long)]
        team: String,
        /// Project description
        #[arg(short, long)]
        description: Option<String>,
        /// Project color (hex)
        #[arg(short, long)]
        color: Option<String>,
    },
    /// Update a project
    Update {
        /// Project ID
        id: String,
        /// New name
        #[arg(short, long)]
        name: Option<String>,
        /// New description
        #[arg(short, long)]
        description: Option<String>,
        /// New color (hex)
        #[arg(short, long)]
        color: Option<String>,
        /// New icon
        #[arg(short, long)]
        icon: Option<String>,
    },
    /// Delete a project
    Delete {
        /// Project ID
        id: String,
        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },
    /// Add labels to a project
    AddLabels {
        /// Project ID
        id: String,
        /// Label IDs to add
        #[arg(required = true)]
        labels: Vec<String>,
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

pub async fn handle(cmd: ProjectCommands, output: OutputFormat) -> Result<()> {
    match cmd {
        ProjectCommands::List { archived } => list_projects(archived, output).await,
        ProjectCommands::Get { id } => get_project(&id, output).await,
        ProjectCommands::Create { name, team, description, color } => {
            create_project(&name, &team, description, color, output).await
        }
        ProjectCommands::Update { id, name, description, color, icon } => {
            update_project(&id, name, description, color, icon, output).await
        }
        ProjectCommands::Delete { id, force } => delete_project(&id, force).await,
        ProjectCommands::AddLabels { id, labels } => add_labels(&id, labels, output).await,
    }
}

async fn list_projects(include_archived: bool, output: OutputFormat) -> Result<()> {
    let client = LinearClient::new()?;

    // Simplified query to reduce GraphQL complexity (was exceeding 10000 limit)
    let query = r#"
        query($includeArchived: Boolean) {
            projects(first: 50, includeArchived: $includeArchived) {
                nodes {
                    id
                    name
                    state
                    url
                    startDate
                    targetDate
                }
            }
        }
    "#;

    let result = client.query(query, Some(json!({ "includeArchived": include_archived }))).await?;

    // Handle JSON output
    if matches!(output, OutputFormat::Json) {
        println!("{}", serde_json::to_string_pretty(&result["data"]["projects"]["nodes"])?);
        return Ok(());
    }

    let empty = vec![];
    let projects = result["data"]["projects"]["nodes"]
        .as_array()
        .unwrap_or(&empty);

    if projects.is_empty() {
        println!("No projects found.");
        return Ok(());
    }

    let rows: Vec<ProjectRow> = projects
        .iter()
        .map(|p| {
            ProjectRow {
                name: p["name"].as_str().unwrap_or("").to_string(),
                status: p["state"].as_str().unwrap_or("-").to_string(),
                labels: "-".to_string(),
                id: p["id"].as_str().unwrap_or("").to_string(),
            }
        })
        .collect();

    let table = Table::new(rows).to_string();
    println!("{}", table);
    println!("\n{} projects", projects.len());

    Ok(())
}

async fn get_project(id: &str, output: OutputFormat) -> Result<()> {
    let client = LinearClient::new()?;

    let query = r#"
        query($id: String!) {
            project(id: $id) {
                id
                name
                description
                summary
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
    if matches!(output, OutputFormat::Json) {
        println!("{}", serde_json::to_string_pretty(project)?);
        return Ok(());
    }

    println!("{}", project["name"].as_str().unwrap_or("").bold());
    println!("{}", "-".repeat(40));

    if let Some(summary) = project["summary"].as_str() {
        println!("Summary: {}", summary);
    }
    if let Some(desc) = project["description"].as_str() {
        println!("Description: {}", desc.chars().take(100).collect::<String>());
    }

    println!("Status: {}", project["status"]["name"].as_str().unwrap_or("-"));
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

async fn create_project(name: &str, team: &str, description: Option<String>, color: Option<String>, output: OutputFormat) -> Result<()> {
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
    if let Some(c) = color {
        input["color"] = json!(c);
    }

    let mutation = r#"
        mutation($input: ProjectCreateInput!) {
            projectCreate(input: $input) {
                success
                project { id name url }
            }
        }
    "#;

    let result = client.mutate(mutation, Some(json!({ "input": input }))).await?;

    if result["data"]["projectCreate"]["success"].as_bool() == Some(true) {
        let project = &result["data"]["projectCreate"]["project"];

        // Handle JSON output
        if matches!(output, OutputFormat::Json) {
            println!("{}", serde_json::to_string_pretty(project)?);
            return Ok(());
        }

        println!("{} Created project: {}", "+".green(), project["name"].as_str().unwrap_or(""));
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
    color: Option<String>,
    icon: Option<String>,
    output: OutputFormat,
) -> Result<()> {
    let client = LinearClient::new()?;

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

    let mutation = r#"
        mutation($id: String!, $input: ProjectUpdateInput!) {
            projectUpdate(id: $id, input: $input) {
                success
                project { id name }
            }
        }
    "#;

    let result = client.mutate(mutation, Some(json!({ "id": id, "input": input }))).await?;

    if result["data"]["projectUpdate"]["success"].as_bool() == Some(true) {
        let project = &result["data"]["projectUpdate"]["project"];

        // Handle JSON output
        if matches!(output, OutputFormat::Json) {
            println!("{}", serde_json::to_string_pretty(project)?);
            return Ok(());
        }

        println!("{} Project updated", "+".green());
    } else {
        anyhow::bail!("Failed to update project");
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

    let mutation = r#"
        mutation($id: String!) {
            projectDelete(id: $id) {
                success
            }
        }
    "#;

    let result = client.mutate(mutation, Some(json!({ "id": id }))).await?;

    if result["data"]["projectDelete"]["success"].as_bool() == Some(true) {
        println!("{} Project deleted", "+".green());
    } else {
        anyhow::bail!("Failed to delete project");
    }

    Ok(())
}

async fn add_labels(id: &str, label_ids: Vec<String>, output: OutputFormat) -> Result<()> {
    let client = LinearClient::new()?;

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
    let result = client.mutate(mutation, Some(json!({ "id": id, "input": input }))).await?;

    if result["data"]["projectUpdate"]["success"].as_bool() == Some(true) {
        let project = &result["data"]["projectUpdate"]["project"];

        // Handle JSON output
        if matches!(output, OutputFormat::Json) {
            println!("{}", serde_json::to_string_pretty(project)?);
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
