use anyhow::{Context, Result};
use clap::Subcommand;
use reqwest::Client;
use serde_json::json;
use std::io::{self, Write};
use std::path::Path;

use crate::api::{resolve_issue_id, LinearClient};

#[derive(Subcommand)]
pub enum UploadCommands {
    /// Fetch an upload from Linear's upload storage
    #[command(alias = "get")]
    Fetch {
        /// The Linear upload URL (e.g., https://uploads.linear.app/...)
        url: String,

        /// Output file path (if not specified, outputs to stdout)
        #[arg(short = 'f', long = "file")]
        file: Option<String>,
    },
    /// Attach a URL to an issue
    #[command(after_help = r#"EXAMPLES:
    linear uploads attach-url LIN-123 https://example.com
    linear up attach-url LIN-123 https://example.com --title "Spec""#)]
    AttachUrl {
        /// Issue ID or identifier
        issue: String,
        /// URL to attach
        url: String,
        /// Optional title (defaults to URL)
        #[arg(short, long)]
        title: Option<String>,
    },
    /// Upload a file and attach it to an issue
    #[command(after_help = r#"EXAMPLES:
    linear uploads upload LIN-123 ./design.png
    linear up upload LIN-123 ./design.png --title "Design" --content-type image/png"#)]
    Upload {
        /// Issue ID or identifier
        issue: String,
        /// File path to upload
        file: String,
        /// Optional title (defaults to filename)
        #[arg(short, long)]
        title: Option<String>,
        /// Override content type (defaults to application/octet-stream)
        #[arg(long)]
        content_type: Option<String>,
    },
}

pub async fn handle(cmd: UploadCommands) -> Result<()> {
    match cmd {
        UploadCommands::Fetch { url, file } => fetch_upload(&url, file).await,
        UploadCommands::AttachUrl { issue, url, title } => attach_url(&issue, &url, title).await,
        UploadCommands::Upload {
            issue,
            file,
            title,
            content_type,
        } => upload_file(&issue, &file, title, content_type).await,
    }
}

async fn fetch_upload(url: &str, file: Option<String>) -> Result<()> {
    // Validate URL is a Linear upload URL
    if !url.starts_with("https://uploads.linear.app/") {
        anyhow::bail!(
            "Invalid URL: expected Linear upload URL starting with 'https://uploads.linear.app/'"
        );
    }

    let client = LinearClient::new()?;
    let bytes = client
        .fetch_bytes(url)
        .await
        .context("Failed to fetch upload from Linear")?;

    if let Some(file_path) = file {
        // Write to file
        std::fs::write(&file_path, &bytes)
            .with_context(|| format!("Failed to write to file: {}", file_path))?;
        eprintln!("Downloaded {} bytes to {}", bytes.len(), file_path);
    } else {
        // Write to stdout
        let mut stdout_handle = io::stdout().lock();
        stdout_handle
            .write_all(&bytes)
            .context("Failed to write to stdout")?;
        stdout_handle.flush()?;
    }

    Ok(())
}

async fn attach_url(issue: &str, url: &str, title: Option<String>) -> Result<()> {
    let client = LinearClient::new()?;
    let issue_id = resolve_issue_id(&client, issue, true).await?;

    let input = json!({
        "issueId": issue_id,
        "title": title.unwrap_or_else(|| url.to_string()),
        "url": url,
    });

    let mutation = r#"
        mutation($input: AttachmentCreateInput!) {
            attachmentCreate(input: $input) {
                success
                attachment {
                    id
                    title
                    url
                }
            }
        }
    "#;

    let result = client
        .mutate(mutation, Some(json!({ "input": input })))
        .await?;
    if result["data"]["attachmentCreate"]["success"].as_bool() == Some(true) {
        let attachment = &result["data"]["attachmentCreate"]["attachment"];
        println!(
            "{} Attached: {}",
            "+",
            attachment["title"].as_str().unwrap_or("")
        );
    } else {
        anyhow::bail!("Failed to attach URL");
    }

    Ok(())
}

async fn upload_file(
    issue: &str,
    file_path: &str,
    title: Option<String>,
    content_type: Option<String>,
) -> Result<()> {
    let client = LinearClient::new()?;
    let issue_id = resolve_issue_id(&client, issue, true).await?;

    let path = Path::new(file_path);
    if !path.exists() {
        anyhow::bail!("File not found: {}", file_path);
    }

    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid file name"))?;

    let data = std::fs::read(path).with_context(|| format!("Failed to read {}", file_path))?;
    let size = data.len() as i32;
    let content_type = content_type.unwrap_or_else(|| "application/octet-stream".to_string());
    let content_type_value = content_type.clone();

    let mutation = r#"
        mutation($filename: String!, $contentType: String!, $size: Int!) {
            fileUpload(filename: $filename, contentType: $contentType, size: $size) {
                success
                uploadFile {
                    uploadUrl
                    assetUrl
                    headers {
                        key
                        value
                    }
                }
            }
        }
    "#;

    let variables = json!({
        "filename": file_name,
        "contentType": content_type_value,
        "size": size,
    });

    let result = client.mutate(mutation, Some(variables)).await?;
    if result["data"]["fileUpload"]["success"].as_bool() != Some(true) {
        anyhow::bail!("Failed to initialize file upload");
    }

    let upload_file = &result["data"]["fileUpload"]["uploadFile"];
    let upload_url = upload_file["uploadUrl"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing upload URL"))?;
    let asset_url = upload_file["assetUrl"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing asset URL"))?;

    let mut has_content_type = false;
    let mut req = Client::new().put(upload_url).body(data);
    if let Some(headers) = upload_file["headers"].as_array() {
        for header in headers {
            if let (Some(key), Some(value)) = (header["key"].as_str(), header["value"].as_str()) {
                if key.eq_ignore_ascii_case("content-type") {
                    has_content_type = true;
                }
                req = req.header(key, value);
            }
        }
    }
    if !has_content_type {
        req = req.header("Content-Type", content_type);
    }

    let response = req.send().await?;
    if !response.status().is_success() {
        anyhow::bail!("Upload failed with status {}", response.status());
    }

    let input = json!({
        "issueId": issue_id,
        "title": title.unwrap_or_else(|| file_name.to_string()),
        "url": asset_url,
    });

    let attachment_mutation = r#"
        mutation($input: AttachmentCreateInput!) {
            attachmentCreate(input: $input) {
                success
                attachment {
                    id
                    title
                    url
                }
            }
        }
    "#;

    let attach_result = client
        .mutate(attachment_mutation, Some(json!({ "input": input })))
        .await?;
    if attach_result["data"]["attachmentCreate"]["success"].as_bool() == Some(true) {
        let attachment = &attach_result["data"]["attachmentCreate"]["attachment"];
        println!(
            "{} Uploaded and attached: {}",
            "+",
            attachment["title"].as_str().unwrap_or("")
        );
    } else {
        anyhow::bail!("Failed to attach uploaded file");
    }

    Ok(())
}
