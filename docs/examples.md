# Usage Examples

## Common Tasks

```bash
linear-cli common
linear-cli tasks
linear-cli agent
```

## Projects

```bash
linear-cli p list                              # List all projects
linear-cli p list --archived                   # Include archived
linear-cli p get PROJECT_ID                    # View project details
linear-cli p create "Q1 Roadmap" -t Engineering
linear-cli p create "Q1 Roadmap" -t Engineering --status started --start-date 2025-01-01
linear-cli p update PROJECT_ID --name "New Name"
linear-cli p update PROJECT_ID --name "New Name" --dry-run
linear-cli p update PROJECT_ID --status completed --target-date 2025-03-31
linear-cli p delete PROJECT_ID --force
linear-cli p archive PROJECT_ID
linear-cli p unarchive PROJECT_ID
linear-cli p add-labels PROJECT_ID LABEL_ID
linear-cli p status list
linear-cli p status create "Blocked" --type started --position 4  # Name max 25 chars
linear-cli p status update STATUS_ID --name "Paused"
linear-cli p updates list PROJECT_ID
linear-cli p updates create PROJECT_ID -b "Status update" --health onTrack
```

## Issues

```bash
linear-cli i list                              # List issues
linear-cli i list -t Engineering -s "In Progress"
linear-cli i list --output json                # Output as JSON
linear-cli i get LIN-123                       # View issue details
linear-cli i get LIN-123 --output json         # JSON output
linear-cli i create "Bug fix" -t Eng -p 1      # Priority: 1=urgent, 4=low
linear-cli i create "Bug fix" -t Eng --project "Q1 Roadmap" --estimate 3 --due 2025-02-10
cat issue.json | linear-cli i create "Bug fix" -t Eng --data -
linear-cli i update LIN-123 -s Done
linear-cli i update LIN-123 --labels Bug --project "Q1 Roadmap"
linear-cli i update LIN-123 -s Done --dry-run
linear-cli i delete LIN-123 --force
linear-cli i start LIN-123                     # Start working: assigns to you, sets In Progress, creates branch
linear-cli i stop LIN-123                      # Stop working: unassigns, resets status
linear-cli i remind LIN-123 --in 2d
linear-cli i subscribe LIN-123
linear-cli i unsubscribe LIN-123
```

## Relations

```bash
linear-cli rel list LIN-123                    # List relations for an issue
linear-cli rel add LIN-123 blocks LIN-456      # Add a relation
linear-cli rel remove LIN-123 blocks LIN-456   # Remove a relation
linear-cli rel children LIN-123                # List child issues
linear-cli rel add LIN-123 relates-to LIN-456 --dry-run
```

## Labels

```bash
linear-cli l list                              # List project labels
linear-cli l list --type issue                 # List issue labels
linear-cli l create "Feature" --color-hex "#10B981"
linear-cli l create "Bug" --type issue --color-hex "#EF4444"
linear-cli l create "Bug" --type issue --team ENG -d "Bug reports"
linear-cli l delete LABEL_ID --force
linear-cli l update LABEL_ID -n "New Name"
```

## Git Integration

```bash
linear-cli g checkout LIN-123                  # Create/checkout branch for issue
linear-cli g branch LIN-123                    # Show branch name for issue
linear-cli g create LIN-123                    # Create branch without checkout
linear-cli g checkout LIN-123 -b custom-branch # Use custom branch name
linear-cli g pr LIN-123                        # Create PR linked to issue
linear-cli g pr LIN-123 --draft                # Create draft PR
linear-cli g pr LIN-123 --base main            # Specify base branch
```

## jj (Jujutsu) Integration

```bash
linear-cli j checkout LIN-123                  # Create bookmark for issue
linear-cli j bookmark LIN-123                  # Show bookmark name for issue
linear-cli j create LIN-123                    # Create bookmark without checkout
linear-cli j pr LIN-123                        # Create PR using jj git push
```

## Sync Local Folders

```bash
linear-cli sy status                           # Compare local folders with Linear
linear-cli sy push -t Engineering              # Create Linear projects for local folders
linear-cli sy push -t Engineering --dry-run    # Preview without creating
```

## Search

```bash
linear-cli s issues "authentication bug"
linear-cli s projects "backend" --limit 10
```

## Uploads

Download attachments and images from Linear issues/comments, or attach new ones:

```bash
# Download to file
linear-cli up fetch "https://uploads.linear.app/..." -f image.png

# Attach a URL to an issue
linear-cli up attach-url LIN-123 https://example.com --title "Spec"

# Upload a file and attach to an issue
linear-cli up upload LIN-123 ./design.png --title "Design" --content-type image/png

# Output to stdout (for piping to other tools)
linear-cli up fetch "https://uploads.linear.app/..." | base64

# Useful for AI agents that need to view images
linear-cli uploads fetch URL -f /tmp/screenshot.png
```

## Other Commands

```bash
# Teams
linear-cli t list
linear-cli t get TEAM_ID
linear-cli t create "Engineering" -k ENG
linear-cli t update ENG -d "Core team"

# Users
linear-cli u list
linear-cli u get me

# Initiatives
linear-cli ini list
linear-cli ini create "Q1 Growth"
linear-cli ini link INITIATIVE_ID PROJECT_ID

# Cycles
linear-cli c list -t Engineering
linear-cli c current -t Engineering
linear-cli c create -t Engineering --starts-at 2025-01-06 --ends-at 2025-01-20

# Comments
linear-cli cm list ISSUE_ID
linear-cli cm list ISSUE_ID --output json      # JSON output for LLMs
linear-cli cm create ISSUE_ID -b "This is a comment"

# Documents
linear-cli d list
linear-cli d get DOC_ID
linear-cli d create "Doc Title" -p PROJECT_ID
linear-cli d update DOC_ID --title "New title" --dry-run
linear-cli d list --output json
linear-cli d delete DOC_ID --force

# Templates
linear-cli tpl list
linear-cli tpl list --output json
linear-cli tpl show bug --output json

# Statuses
linear-cli st list -t Engineering
linear-cli st get "In Progress" -t Engineering
linear-cli st create -t Engineering "Ready" --type unstarted
linear-cli st update STATE_ID -c "#10B981"
linear-cli st archive STATE_ID

# Config
linear-cli config set-key YOUR_API_KEY
linear-cli config show
```

## Interactive Mode

```bash
linear-cli ui                                  # Launch interactive TUI
linear-cli ui --team ENG                       # Launch with preselected team
linear-cli ui issues                           # Browse issues interactively
linear-cli ui projects                         # Browse projects interactively
linear-cli interactive --team Engineering      # Filter by team
```

## Multiple Workspaces

```bash
linear-cli ws list                             # List configured workspaces
linear-cli ws add personal                     # Add a new workspace
linear-cli ws switch personal                  # Switch active workspace
linear-cli ws current                          # Show current workspace
linear-cli ws remove personal                  # Remove a workspace
```

## Bulk Operations

```bash
linear-cli b update-state Done -i LIN-1,LIN-2,LIN-3  # Update multiple issues
linear-cli b assign --user me -i LIN-1,LIN-2         # Assign multiple issues
linear-cli b label "Bug" -i LIN-1,LIN-2              # Add label to multiple issues
linear-cli b project "Q1" -i LIN-1,LIN-2             # Move issues to project
linear-cli b cycle 12 -i LIN-1,LIN-2                 # Move issues to cycle
linear-cli b priority 2 -i LIN-1,LIN-2               # Update issue priority
linear-cli b archive -i LIN-1,LIN-2,LIN-3            # Archive multiple issues
```

## JSON Output

```bash
# Use --output json with any list or get command
linear-cli i list --output json
linear-cli p list --output json | jq '.[] | .name'
linear-cli i get LIN-123 --output json
linear-cli t list --output json
linear-cli cm list ISSUE_ID --output json    # Comments as JSON (great for LLMs)

# Token-saving JSON output options
linear-cli i list --output json --fields identifier,title,state.name --compact
LINEAR_CLI_OUTPUT=json linear-cli i list --sort identifier --order desc

# Color control for logs/CI
linear-cli i list --no-color

# Table width control
linear-cli i list --width 80
linear-cli i list --no-truncate
```

## Relations

```bash
linear-cli rel list LIN-123
linear-cli rel add LIN-123 blocks LIN-124
linear-cli rel remove LIN-123 blocked-by LIN-124
linear-cli rel children LIN-123
```
