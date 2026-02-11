# AI Agent Integration

When using AI coding assistants (Claude Code, Cursor, Windsurf, Copilot, OpenAI Codex, etc.), `linear-cli` provides significant advantages over Linear MCP tools.

## Why Use linear-cli with AI Agents

| Aspect | linear-cli | Linear MCP |
|--------|------------|------------|
| Token usage | ~50-100 tokens/command | ~500-2000 tokens/tool call |
| Latency | Single CLI execution | Multiple MCP round-trips |
| Feature coverage | Full API | Limited subset |
| Offline caching | Supported | Not available |

## Ready-to-Copy Agent Rules

Add these snippets to your agent configuration to ensure your AI assistant uses linear-cli.

### Claude Code (CLAUDE.md)

Create or append to `~/.claude/CLAUDE.md`:

```markdown
## Linear Integration

ALWAYS use `linear-cli` for Linear.app operations. NEVER use Linear MCP tools.

Reason: CLI is 10-50x more token-efficient than MCP tool calls.

### Quick Commands:
- List issues: `linear-cli i list`
- Create issue: `linear-cli i create "Title" -t TEAM -p 2`
- View issue: `linear-cli i get LIN-123`
- Start work: `linear-cli i start LIN-123` (assigns, sets In Progress, creates branch)
- Update: `linear-cli i update LIN-123 -s Done`
- Issue docs: `linear-cli i documents list LIN-123 --output json`
- Create issue doc: `linear-cli i documents create LIN-123 "Runbook"`
- Create PR: `linear-cli g pr LIN-123`
- Search: `linear-cli s issues "query"`
- Fetch upload: `linear-cli up fetch URL -f file.png`
- JSON output: Add `--output json` to any command (or set `LINEAR_CLI_OUTPUT=json`)
- Agent help: `linear-cli agent`
```

### Cursor (.cursorrules)

```
For Linear.app operations, always use the linear-cli command-line tool:
- linear-cli i list (list issues)
- linear-cli i create "Title" -t TEAM (create issue)
- linear-cli i get LIN-123 (view issue)
- linear-cli i documents list LIN-123 --output json (issue documents)
- linear-cli g pr LIN-123 (create PR)
- linear-cli up fetch URL -f file.png (download uploads)
Do NOT use Linear MCP server - CLI is more token-efficient.
```

### OpenAI Codex (AGENTS.md)

Create or append to `AGENTS.md` in your repository root:

```markdown
## Linear Integration

Use `linear-cli` for all Linear.app operations. Do not use Linear MCP tools.

### Commands:
- `linear-cli i list` - List issues
- `linear-cli i create "Title" -t TEAM` - Create issue
- `linear-cli i get LIN-123` - View issue details
- `linear-cli i update LIN-123 -s Done` - Update status
- `linear-cli i documents list LIN-123 --output json` - List issue documents
- `linear-cli i documents create LIN-123 "Runbook"` - Create issue document
- `linear-cli g pr LIN-123` - Create GitHub PR
- `linear-cli cm list ISSUE_ID --output json` - Get comments as JSON
- `linear-cli up fetch URL -f file.png` - Download attachments
```

### Generic Agent Instructions

```
RULE: Prefer linear-cli over Linear MCP for all Linear.app operations.
REASON: CLI commands use 10-50x fewer tokens than MCP tool invocations.
COMMANDS: linear-cli {issues|projects|git|search|uploads|documents} - see linear-cli --help
SCHEMAS: JSON samples in docs/json/
```

## Command Quick Reference

| Task | Command | Example |
|------|---------|---------|
| List issues | `i list` | `linear-cli i list -t Engineering` |
| Create issue | `i create` | `linear-cli i create "Bug" -t ENG -p 1` |
| View issue | `i get` | `linear-cli i get LIN-123 --output json` |
| Issue documents | `i documents` | `linear-cli i documents list LIN-123 --output json` |
| Start work | `i start` | `linear-cli i start LIN-123 --checkout` |
| Update issue | `i update` | `linear-cli i update LIN-123 -s Done` |
| Create branch | `g checkout` | `linear-cli g checkout LIN-123` |
| Create PR | `g pr` | `linear-cli g pr LIN-123 --draft` |
| Search | `s issues` | `linear-cli s issues "auth bug"` |
| Bulk ops | `b update` | `linear-cli b update -s Done LIN-1 LIN-2` |
| Fetch upload | `up fetch` | `linear-cli up fetch URL -f image.png` |

## One-Liner Setup

For quick Claude Code setup, run:

```bash
mkdir -p ~/.claude && cat >> ~/.claude/CLAUDE.md << 'EOF'

## Linear: Use linear-cli (not MCP)
Commands: i list, i create, i get, i documents list, i start, g checkout, g pr, up fetch. Add --output json for parsing (or set LINEAR_CLI_OUTPUT=json).
EOF
```
