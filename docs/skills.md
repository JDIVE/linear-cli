# Agent Skills

`linear-cli` includes pre-built skills for AI coding assistants. Skills provide contextual documentation that agents can load when performing specific tasks.

## Supported Agents

| Agent | Skills Location | Skill File |
|-------|-----------------|------------|
| Claude Code | `.claude/skills/` | `SKILL.md` |
| OpenAI Codex | `.codex/skills/` | `AGENTS.md` |

## Available Skills

| Skill | Description |
|-------|-------------|
| `linear-issues` | Manage issues - list, create, update, start/stop work |
| `linear-pr` | Create GitHub PRs linked to Linear issues |
| `linear-search` | Search issues and projects |
| `linear-uploads` | Download attachments and images |

## Installation

### Option 1: Clone to Your Project

Copy the skills directories to your project:

```bash
# For Claude Code
cp -r /path/to/linear-cli/.claude/skills/* /your/project/.claude/skills/

# For OpenAI Codex
cp -r /path/to/linear-cli/.codex/skills/* /your/project/.codex/skills/
```

### Option 2: Symlink (Recommended)

Symlink the skills from your linear-cli installation:

```bash
# Find where linear-cli is installed
which linear-cli  # or: where linear-cli on Windows

# Assuming it's installed via cargo
SKILLS_SRC="$HOME/.cargo/git/checkouts/linear-cli-*/master"

# For Claude Code
ln -s "$SKILLS_SRC/.claude/skills" /your/project/.claude/skills

# For OpenAI Codex
ln -s "$SKILLS_SRC/.codex/skills" /your/project/.codex/skills
```

### Option 3: Global Installation

For Claude Code, you can install skills globally:

```bash
# Copy to global Claude config
mkdir -p ~/.claude/skills
cp -r /path/to/linear-cli/.claude/skills/* ~/.claude/skills/
```

## How Skills Work

### Claude Code

Claude Code loads skills based on the `description` field in `SKILL.md`. When you ask about Linear issues, PRs, or uploads, Claude automatically loads the relevant skill.

Example `SKILL.md` structure:
```yaml
---
name: linear-issues
description: Manage Linear issues - list, create, update, start/stop work.
allowed-tools: Bash
---

# Linear Issues
[Documentation content...]
```

### OpenAI Codex

Codex reads `AGENTS.md` files for contextual instructions. Place skills in `.codex/skills/` directories.

## Verifying Installation

### Claude Code

Ask Claude: "What Linear commands are available?"

Claude should reference the skills and show commands like:
- `linear-cli i list`
- `linear-cli i create "Title" -t TEAM`
- etc.

### OpenAI Codex

Ask Codex to work with Linear issues. It should use `linear-cli` commands rather than suggesting MCP tools.

## Creating Custom Skills

You can extend the skills or create new ones:

```bash
# Create a new skill for your workflow
mkdir -p .claude/skills/my-workflow
cat > .claude/skills/my-workflow/SKILL.md << 'EOF'
---
name: my-workflow
description: Custom Linear workflow for my team
allowed-tools: Bash
---

# My Custom Workflow

## Daily Standup
\`\`\`bash
linear-cli i list -s "In Progress" --mine
\`\`\`
EOF
```
