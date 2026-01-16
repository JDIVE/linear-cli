# JSON Output Samples

These files are sample outputs for common commands. They are intended to help agent
parsers and tests understand the shape of JSON responses. Field presence may vary
depending on Linear data and permissions.

Commands that produce these shapes:
- `linear-cli i list --output json`
- `linear-cli i get LIN-123 --output json`
- `linear-cli p list --output json`
- `linear-cli t list --output json`
- `linear-cli cm list ISSUE_ID --output json`
- `linear-cli context --output json`

Schema version:
- `1.0` (see `docs/json/schema.json`)

Notes:
- Use `--output ndjson` for streaming lists (one JSON object per line).
- Use `--fields`, `--sort`, and `--filter` to shape outputs.
- Use `--schema` to print the current schema version.
- Errors are returned as a JSON object with `error: true`, optional `details`, and `retry_after`.
