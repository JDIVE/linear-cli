use anyhow::Result;
use clap::Subcommand;
use colored::Colorize;
use serde_json::{json, Value};
use std::io::{self, BufRead};
use std::path::Path;
use tabled::{Table, Tabled};

use crate::api::{
    resolve_initiative_id, resolve_project_id, resolve_team_id, resolve_user_id, LinearClient,
};
use crate::display_options;
use crate::input::read_ids_from_stdin;
use crate::output::{ensure_non_empty, filter_values, print_json, sort_values, OutputOptions};
use crate::pagination::paginate_nodes;
use crate::text::truncate;

#[derive(Subcommand)]
pub enum CustomViewCommands {
    /// List custom views
    #[command(alias = "ls")]
    List {
        /// Include archived custom views
        #[arg(short, long)]
        archived: bool,
    },
    /// Get custom view details
    Get {
        /// Custom view ID(s). Use "-" to read from stdin.
        ids: Vec<String>,
    },
    /// Create a custom view
    Create {
        /// Name for the custom view
        name: String,
        /// Description
        #[arg(short, long)]
        description: Option<String>,
        /// Icon emoji/identifier
        #[arg(long)]
        icon: Option<String>,
        /// Color (hex)
        #[arg(short, long = "color-hex")]
        color_hex: Option<String>,
        /// Team key/name/ID
        #[arg(short, long)]
        team: Option<String>,
        /// Project name/ID
        #[arg(long)]
        project: Option<String>,
        /// Initiative name/ID
        #[arg(long)]
        initiative: Option<String>,
        /// Owner (user ID/name/email or "me")
        #[arg(long)]
        owner: Option<String>,
        /// Mark as shared
        #[arg(long)]
        shared: bool,
        /// Issue filter JSON object
        #[arg(long)]
        filter_data: Option<String>,
        /// Project filter JSON object
        #[arg(long)]
        project_filter_data: Option<String>,
        /// Initiative filter JSON object
        #[arg(long)]
        initiative_filter_data: Option<String>,
        /// Feed item filter JSON object
        #[arg(long)]
        feed_item_filter_data: Option<String>,
        /// Full JSON input object (or "-" for stdin)
        #[arg(long)]
        data: Option<String>,
    },
    /// Update a custom view
    Update {
        /// Custom view ID
        id: String,
        /// Updated name
        #[arg(short, long)]
        name: Option<String>,
        /// Updated description
        #[arg(short, long)]
        description: Option<String>,
        /// Updated icon
        #[arg(long)]
        icon: Option<String>,
        /// Updated color (hex)
        #[arg(short, long = "color-hex")]
        color_hex: Option<String>,
        /// Updated team key/name/ID
        #[arg(short, long)]
        team: Option<String>,
        /// Updated project name/ID
        #[arg(long)]
        project: Option<String>,
        /// Updated initiative name/ID
        #[arg(long)]
        initiative: Option<String>,
        /// Updated owner (user ID/name/email or "me")
        #[arg(long)]
        owner: Option<String>,
        /// Shared toggle
        #[arg(long)]
        shared: Option<bool>,
        /// Issue filter JSON object
        #[arg(long)]
        filter_data: Option<String>,
        /// Project filter JSON object
        #[arg(long)]
        project_filter_data: Option<String>,
        /// Initiative filter JSON object
        #[arg(long)]
        initiative_filter_data: Option<String>,
        /// Feed item filter JSON object
        #[arg(long)]
        feed_item_filter_data: Option<String>,
        /// Full JSON input object (or "-" for stdin)
        #[arg(long)]
        data: Option<String>,
    },
    /// Delete a custom view
    Delete {
        /// Custom view ID
        id: String,
    },
}

#[derive(Tabled)]
struct CustomViewRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Owner")]
    owner: String,
    #[tabled(rename = "Team")]
    team: String,
    #[tabled(rename = "Shared")]
    shared: String,
    #[tabled(rename = "Updated")]
    updated_at: String,
    #[tabled(rename = "ID")]
    id: String,
}

pub async fn handle(cmd: CustomViewCommands, output: &OutputOptions) -> Result<()> {
    match cmd {
        CustomViewCommands::List { archived } => list_custom_views(archived, output).await,
        CustomViewCommands::Get { ids } => {
            let final_ids = read_ids_from_stdin(ids);
            if final_ids.is_empty() {
                anyhow::bail!("No custom view IDs provided. Provide IDs or pipe them via stdin.");
            }
            get_custom_views(&final_ids, output).await
        }
        CustomViewCommands::Create {
            name,
            description,
            icon,
            color_hex,
            team,
            project,
            initiative,
            owner,
            shared,
            filter_data,
            project_filter_data,
            initiative_filter_data,
            feed_item_filter_data,
            data,
        } => {
            create_custom_view(
                &name,
                description,
                icon,
                color_hex,
                team,
                project,
                initiative,
                owner,
                shared,
                filter_data,
                project_filter_data,
                initiative_filter_data,
                feed_item_filter_data,
                data,
                output,
            )
            .await
        }
        CustomViewCommands::Update {
            id,
            name,
            description,
            icon,
            color_hex,
            team,
            project,
            initiative,
            owner,
            shared,
            filter_data,
            project_filter_data,
            initiative_filter_data,
            feed_item_filter_data,
            data,
        } => {
            update_custom_view(
                &id,
                name,
                description,
                icon,
                color_hex,
                team,
                project,
                initiative,
                owner,
                shared,
                filter_data,
                project_filter_data,
                initiative_filter_data,
                feed_item_filter_data,
                data,
                output,
            )
            .await
        }
        CustomViewCommands::Delete { id } => delete_custom_view(&id, output).await,
    }
}

async fn list_custom_views(include_archived: bool, output: &OutputOptions) -> Result<()> {
    let client = LinearClient::new()?;

    let query = r#"
        query($includeArchived: Boolean, $first: Int, $after: String, $last: Int, $before: String) {
            customViews(
                includeArchived: $includeArchived,
                first: $first,
                after: $after,
                last: $last,
                before: $before
            ) {
                nodes {
                    id
                    name
                    description
                    icon
                    color
                    shared
                    slugId
                    updatedAt
                    archivedAt
                    team { key name }
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
    let mut views = paginate_nodes(
        &client,
        query,
        vars,
        &["data", "customViews", "nodes"],
        &["data", "customViews", "pageInfo"],
        &pagination,
        50,
    )
    .await?;

    if output.is_json() || output.has_template() {
        print_json(&serde_json::json!(views), output)?;
        return Ok(());
    }

    filter_values(&mut views, &output.filters);
    if let Some(sort_key) = output.json.sort.as_deref() {
        sort_values(&mut views, sort_key, output.json.order);
    }

    ensure_non_empty(&views, output)?;
    if views.is_empty() {
        println!("No custom views found.");
        return Ok(());
    }

    let width = display_options().max_width(36);
    let rows: Vec<CustomViewRow> = views
        .iter()
        .map(|v| CustomViewRow {
            name: truncate(v["name"].as_str().unwrap_or(""), width),
            owner: v["owner"]["name"].as_str().unwrap_or("-").to_string(),
            team: v["team"]["key"].as_str().unwrap_or("-").to_string(),
            shared: if v["shared"].as_bool().unwrap_or(false) {
                "yes".to_string()
            } else {
                "no".to_string()
            },
            updated_at: v["updatedAt"]
                .as_str()
                .unwrap_or("-")
                .chars()
                .take(10)
                .collect(),
            id: v["id"].as_str().unwrap_or("").to_string(),
        })
        .collect();

    let table = Table::new(rows).to_string();
    println!("{}", table);
    println!("\n{} custom views", views.len());

    Ok(())
}

async fn get_custom_views(ids: &[String], output: &OutputOptions) -> Result<()> {
    if ids.len() == 1 {
        return get_custom_view(&ids[0], output).await;
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
                        customView(id: $id) {
                            id
                            name
                            description
                            icon
                            color
                            shared
                            slugId
                            createdAt
                            updatedAt
                            archivedAt
                            owner { id name email }
                            team { id key name }
                            filterData
                            projectFilterData
                            initiativeFilterData
                            feedItemFilterData
                        }
                    }
                "#;

                match client.query(query, Some(json!({ "id": id.clone() }))).await {
                    Ok(result) => {
                        let view = result["data"]["customView"].clone();
                        if view.is_null() {
                            (id, Err(anyhow::anyhow!("Custom view not found")))
                        } else {
                            (id, Ok(view))
                        }
                    }
                    Err(e) => (id, Err(e)),
                }
            }
        })
        .collect();

    let mut results = Vec::new();
    for (id, result) in futures::future::join_all(futures).await {
        match result {
            Ok(view) => results.push(view),
            Err(e) => {
                if !output.is_json() && !output.has_template() {
                    eprintln!("{} Failed to fetch {}: {}", "!".yellow(), id, e);
                }
            }
        }
    }

    if output.is_json() || output.has_template() {
        print_json(&serde_json::json!(results), output)?;
        return Ok(());
    }

    ensure_non_empty(&results, output)?;
    for (idx, view) in results.iter().enumerate() {
        if idx > 0 {
            println!();
        }
        print_custom_view_details(view);
    }

    Ok(())
}

async fn get_custom_view(id: &str, output: &OutputOptions) -> Result<()> {
    let client = LinearClient::new()?;
    let query = r#"
        query($id: String!) {
            customView(id: $id) {
                id
                name
                description
                icon
                color
                shared
                slugId
                createdAt
                updatedAt
                archivedAt
                owner { id name email }
                team { id key name }
                filterData
                projectFilterData
                initiativeFilterData
                feedItemFilterData
            }
        }
    "#;

    let result = client.query(query, Some(json!({ "id": id }))).await?;
    let view = &result["data"]["customView"];

    if view.is_null() {
        anyhow::bail!("Custom view not found: {}", id);
    }

    if output.is_json() || output.has_template() {
        print_json(view, output)?;
        return Ok(());
    }

    print_custom_view_details(view);
    Ok(())
}

fn print_custom_view_details(view: &Value) {
    println!("{}", view["name"].as_str().unwrap_or("").bold());
    println!("{}", "-".repeat(50));

    if let Some(desc) = view["description"].as_str() {
        if !desc.is_empty() {
            println!("Description: {}", desc);
        }
    }

    println!("Shared: {}", view["shared"].as_bool().unwrap_or(false));
    println!("Slug: {}", view["slugId"].as_str().unwrap_or("-"));
    println!("Team: {}", view["team"]["key"].as_str().unwrap_or("-"));
    println!("Owner: {}", view["owner"]["name"].as_str().unwrap_or("-"));
    println!("Color: {}", view["color"].as_str().unwrap_or("-"));
    println!("Icon: {}", view["icon"].as_str().unwrap_or("-"));
    println!("ID: {}", view["id"].as_str().unwrap_or("-"));
}

#[allow(clippy::too_many_arguments)]
async fn create_custom_view(
    name: &str,
    description: Option<String>,
    icon: Option<String>,
    color_hex: Option<String>,
    team: Option<String>,
    project: Option<String>,
    initiative: Option<String>,
    owner: Option<String>,
    shared: bool,
    filter_data: Option<String>,
    project_filter_data: Option<String>,
    initiative_filter_data: Option<String>,
    feed_item_filter_data: Option<String>,
    data: Option<String>,
    output: &OutputOptions,
) -> Result<()> {
    let client = LinearClient::new()?;
    let mut input = if let Some(data) = data {
        parse_json_object(&data)?
    } else {
        json!({})
    };

    input["name"] = json!(name);
    if let Some(description) = description {
        input["description"] = json!(description);
    }
    if let Some(icon) = icon {
        input["icon"] = json!(icon);
    }
    if let Some(color) = color_hex {
        input["color"] = json!(color);
    }
    if shared {
        input["shared"] = json!(true);
    }
    if let Some(filter_data) = filter_data {
        input["filterData"] = parse_json_object(&filter_data)?;
    }
    if let Some(project_filter_data) = project_filter_data {
        input["projectFilterData"] = parse_json_object(&project_filter_data)?;
    }
    if let Some(initiative_filter_data) = initiative_filter_data {
        input["initiativeFilterData"] = parse_json_object(&initiative_filter_data)?;
    }
    if let Some(feed_item_filter_data) = feed_item_filter_data {
        input["feedItemFilterData"] = parse_json_object(&feed_item_filter_data)?;
    }

    if let Some(team) = team {
        input["teamId"] = json!(resolve_team_id(&client, &team, &output.cache).await?);
    }
    if let Some(project) = project {
        input["projectId"] = json!(resolve_project_id(&client, &project, true).await?);
    }
    if let Some(initiative) = initiative {
        input["initiativeId"] = json!(resolve_initiative_id(&client, &initiative, true).await?);
    }
    if let Some(owner) = owner {
        input["ownerId"] = json!(resolve_user_id(&client, &owner, &output.cache).await?);
    }

    let mutation = r#"
        mutation($input: CustomViewCreateInput!) {
            customViewCreate(input: $input) {
                success
                customView {
                    id
                    name
                    shared
                    slugId
                    updatedAt
                    owner { name }
                    team { key }
                }
            }
        }
    "#;

    let result = client
        .mutate(mutation, Some(json!({ "input": input })))
        .await?;

    if result["data"]["customViewCreate"]["success"].as_bool() != Some(true) {
        anyhow::bail!("Failed to create custom view");
    }

    let view = &result["data"]["customViewCreate"]["customView"];
    if output.is_json() || output.has_template() {
        print_json(view, output)?;
        return Ok(());
    }

    println!(
        "{} Created custom view: {}",
        "+".green(),
        view["name"].as_str().unwrap_or("")
    );
    println!("  ID: {}", view["id"].as_str().unwrap_or(""));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn update_custom_view(
    id: &str,
    name: Option<String>,
    description: Option<String>,
    icon: Option<String>,
    color_hex: Option<String>,
    team: Option<String>,
    project: Option<String>,
    initiative: Option<String>,
    owner: Option<String>,
    shared: Option<bool>,
    filter_data: Option<String>,
    project_filter_data: Option<String>,
    initiative_filter_data: Option<String>,
    feed_item_filter_data: Option<String>,
    data: Option<String>,
    output: &OutputOptions,
) -> Result<()> {
    let client = LinearClient::new()?;
    let mut input = if let Some(data) = data {
        parse_json_object(&data)?
    } else {
        json!({})
    };

    if let Some(name) = name {
        input["name"] = json!(name);
    }
    if let Some(description) = description {
        input["description"] = json!(description);
    }
    if let Some(icon) = icon {
        input["icon"] = json!(icon);
    }
    if let Some(color) = color_hex {
        input["color"] = json!(color);
    }
    if let Some(shared) = shared {
        input["shared"] = json!(shared);
    }
    if let Some(filter_data) = filter_data {
        input["filterData"] = parse_json_object(&filter_data)?;
    }
    if let Some(project_filter_data) = project_filter_data {
        input["projectFilterData"] = parse_json_object(&project_filter_data)?;
    }
    if let Some(initiative_filter_data) = initiative_filter_data {
        input["initiativeFilterData"] = parse_json_object(&initiative_filter_data)?;
    }
    if let Some(feed_item_filter_data) = feed_item_filter_data {
        input["feedItemFilterData"] = parse_json_object(&feed_item_filter_data)?;
    }

    if let Some(team) = team {
        input["teamId"] = json!(resolve_team_id(&client, &team, &output.cache).await?);
    }
    if let Some(project) = project {
        input["projectId"] = json!(resolve_project_id(&client, &project, true).await?);
    }
    if let Some(initiative) = initiative {
        input["initiativeId"] = json!(resolve_initiative_id(&client, &initiative, true).await?);
    }
    if let Some(owner) = owner {
        input["ownerId"] = json!(resolve_user_id(&client, &owner, &output.cache).await?);
    }

    let is_empty = input.as_object().map(|m| m.is_empty()).unwrap_or(true);
    if is_empty {
        anyhow::bail!("No changes provided. Use field flags or --data to provide update payload.");
    }

    let mutation = r#"
        mutation($id: String!, $input: CustomViewUpdateInput!) {
            customViewUpdate(id: $id, input: $input) {
                success
                customView {
                    id
                    name
                    shared
                    slugId
                    updatedAt
                    owner { name }
                    team { key }
                }
            }
        }
    "#;

    let result = client
        .mutate(mutation, Some(json!({ "id": id, "input": input })))
        .await?;

    if result["data"]["customViewUpdate"]["success"].as_bool() != Some(true) {
        anyhow::bail!("Failed to update custom view");
    }

    let view = &result["data"]["customViewUpdate"]["customView"];
    if output.is_json() || output.has_template() {
        print_json(view, output)?;
        return Ok(());
    }

    println!(
        "{} Updated custom view: {}",
        "+".green(),
        view["name"].as_str().unwrap_or("")
    );
    println!("  ID: {}", view["id"].as_str().unwrap_or(id));
    Ok(())
}

async fn delete_custom_view(id: &str, output: &OutputOptions) -> Result<()> {
    let client = LinearClient::new()?;

    let mutation = r#"
        mutation($id: String!) {
            customViewDelete(id: $id) {
                success
                entityId
            }
        }
    "#;

    let result = client.mutate(mutation, Some(json!({ "id": id }))).await?;
    if result["data"]["customViewDelete"]["success"].as_bool() != Some(true) {
        anyhow::bail!("Failed to delete custom view");
    }

    if output.is_json() || output.has_template() {
        print_json(&result["data"]["customViewDelete"], output)?;
        return Ok(());
    }

    println!("{} Deleted custom view {}", "+".green(), id.cyan());
    Ok(())
}

fn parse_json_object(input: &str) -> Result<Value> {
    let raw = if input == "-" {
        let stdin = io::stdin();
        let lines: Vec<String> = stdin.lock().lines().map_while(Result::ok).collect();
        lines.join("\n")
    } else if let Some(path) = input.strip_prefix('@') {
        std::fs::read_to_string(path)?
    } else if Path::new(input).exists() {
        std::fs::read_to_string(input)?
    } else {
        input.to_string()
    };

    let value: Value = serde_json::from_str(&raw)
        .map_err(|e| anyhow::anyhow!("Invalid JSON input for '{}': {}", input, e))?;
    if !value.is_object() {
        anyhow::bail!("Expected JSON object payload.");
    }
    Ok(value)
}
