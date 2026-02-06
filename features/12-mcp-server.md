# Feature 12: MCP Server

## Overview

The MCP (Model Context Protocol) server exposes Ultragit's read operations as tools that any MCP-connected agent can call directly. Instead of an agent shelling out to the `ultragit` CLI and parsing stdout, the MCP server provides a native tool interface: the agent calls `ultragit_read` as a tool, receives structured JSON, and incorporates the annotations into its reasoning.

This is the cleanest integration path for agents that support MCP. The agent's framework handles tool discovery, invocation, and result parsing. Ultragit appears as just another tool in the agent's toolbox, alongside file reading, code search, and terminal access.

The MCP server is a thin wrapper around the existing read pipeline (Feature 07) and advanced query modules (Feature 08). It translates MCP tool invocations into read pipeline calls and formats the results as MCP tool responses. No new annotation logic — just a protocol adapter.

---

## Dependencies

| Feature | Reason |
|---------|--------|
| 07 Read Pipeline | All tool implementations delegate to the read pipeline for blame, note retrieval, filtering, scoring, and output assembly |
| 08 Advanced Queries | `ultragit_deps`, `ultragit_history`, and `ultragit_summary` tools delegate to the advanced query modules |
| 01 CLI Framework & Config | Server reads repository configuration (notes ref, filter settings) |
| 02 Git Operations Layer | All read operations are git operations |

---

## Public API

### CLI Commands

#### `ultragit mcp start`

Starts the MCP server as a long-lived process.

```
ultragit mcp start [OPTIONS]
```

**Flags:**
- `--repo <PATH>` — repository root. Default: current working directory.
- `--noteref <REF>` — notes ref to read. Default: `refs/notes/ultragit`.

**Behavior:** The server runs as a stdio-based JSON-RPC process. It reads MCP requests from stdin and writes MCP responses to stdout. It stays alive until stdin is closed or the process is signaled.

This is not a command users run directly. It is invoked by the MCP client (e.g., Claude Desktop, Claude Code, or another MCP host) based on the server configuration.

#### `ultragit mcp install`

Registers the Ultragit MCP server in the agent's MCP configuration.

```
ultragit mcp install [OPTIONS]
```

**Flags:**
- `--config <PATH>` — path to MCP config file. If omitted, searches standard locations:
  - `.mcp.json` (repository root, for Claude Code)
  - `~/.config/claude/claude_desktop_config.json` (for Claude Desktop)
- `--global` — install globally (Claude Desktop config) rather than per-repository.

**Effect:** Adds or updates the Ultragit server entry in the MCP configuration:

```json
{
  "mcpServers": {
    "ultragit": {
      "command": "ultragit",
      "args": ["mcp", "start"],
      "cwd": "/path/to/repo"
    }
  }
}
```

If the entry already exists with the same command, do nothing (idempotent). If it exists with different args, update it.

### MCP Tool Definitions

The server exposes four tools, mirroring the CLI read commands.

#### `ultragit_read`

Retrieves annotations for a code region.

```json
{
  "name": "ultragit_read",
  "description": "Retrieve Ultragit annotations for a file or code region. Returns intent, reasoning, constraints, semantic dependencies, and risk notes captured at commit time. Use before modifying existing code.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "path": {
        "type": "string",
        "description": "File path relative to repository root"
      },
      "anchor": {
        "type": "string",
        "description": "Function or type name to scope the query (e.g., 'MqttClient::connect')"
      },
      "lines": {
        "type": "string",
        "description": "Line range in START:END format (e.g., '42:67')"
      },
      "max_tokens": {
        "type": "integer",
        "description": "Maximum token budget for the response. Output is trimmed to fit."
      }
    },
    "required": ["path"]
  }
}
```

#### `ultragit_deps`

Returns semantic dependencies on a code region — what other code assumes about this code.

```json
{
  "name": "ultragit_deps",
  "description": "Find code that depends on behavioral assumptions about the specified function or region. Critical before modifying any function's behavior or signature.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "path": {
        "type": "string",
        "description": "File path relative to repository root"
      },
      "anchor": {
        "type": "string",
        "description": "Function or type name to check dependencies for"
      }
    },
    "required": ["path"]
  }
}
```

#### `ultragit_history`

Returns the annotation timeline for a code region.

```json
{
  "name": "ultragit_history",
  "description": "Show the reasoning timeline for a code region: what changed and why at each step. Use when debugging surprising behavior.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "path": {
        "type": "string",
        "description": "File path relative to repository root"
      },
      "anchor": {
        "type": "string",
        "description": "Function or type name"
      },
      "limit": {
        "type": "integer",
        "description": "Maximum number of history entries to return. Default: 10."
      }
    },
    "required": ["path"]
  }
}
```

#### `ultragit_summary`

Returns a condensed overview of a file or module.

```json
{
  "name": "ultragit_summary",
  "description": "Get a condensed view of intent and constraints for all annotated regions in a file or directory. Use for broad orientation on an unfamiliar module.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "path": {
        "type": "string",
        "description": "File or directory path relative to repository root"
      },
      "anchor": {
        "type": "string",
        "description": "Optional function or type name to scope the summary"
      }
    },
    "required": ["path"]
  }
}
```

---

## Internal Design

### MCP Protocol

The MCP server implements the Model Context Protocol over stdio transport. The protocol uses JSON-RPC 2.0 as its message format.

**Key protocol messages:**

- `initialize` — handshake; server declares capabilities (tools).
- `tools/list` — client requests available tools; server responds with tool definitions.
- `tools/call` — client invokes a tool; server executes and responds with the result.

The server does not implement resources, prompts, or sampling — only tools.

```rust
/// MCP server state
pub struct McpServer {
    /// Repository root for git operations
    repo_root: PathBuf,

    /// Notes ref to read annotations from
    notes_ref: String,

    /// Read pipeline (shared across tool calls)
    read_pipeline: ReadPipeline,
}

impl McpServer {
    pub fn new(repo_root: PathBuf, notes_ref: String) -> Result<Self>;

    /// Main event loop: read requests from stdin, dispatch, write responses to stdout
    pub async fn run(&self) -> Result<()>;
}
```

### Message Flow

```
MCP Client (stdin)                    Ultragit MCP Server
     │                                       │
     │── initialize ──────────────────────>  │
     │<── initialize result ──────────────── │
     │                                       │
     │── tools/list ──────────────────────>  │
     │<── tool definitions ───────────────── │
     │                                       │
     │── tools/call (ultragit_read) ──────>  │
     │      { path, anchor, max_tokens }     │
     │                                       │
     │           ┌─── ReadPipeline ───┐      │
     │           │ blame → notes →    │      │
     │           │ filter → score →   │      │
     │           │ assemble JSON      │      │
     │           └────────────────────┘      │
     │                                       │
     │<── tool result (annotation JSON) ──── │
     │                                       │
     │── tools/call (ultragit_deps) ──────>  │
     │           ...                         │
```

### JSON-RPC Message Format

**Request (tools/call):**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "ultragit_read",
    "arguments": {
      "path": "src/mqtt/client.rs",
      "anchor": "MqttClient::connect",
      "max_tokens": 2000
    }
  }
}
```

**Success response:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "{\"$schema\":\"ultragit-read/v1\",\"query\":{...},\"regions\":[...],\"stats\":{...}}"
      }
    ]
  }
}
```

**Error response:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "No annotations found for src/mqtt/client.rs"
      }
    ],
    "isError": true
  }
}
```

### Tool Dispatch

Each MCP tool maps directly to a read pipeline function:

```rust
/// Dispatch a tool call to the appropriate read pipeline function
async fn dispatch_tool(
    &self,
    tool_name: &str,
    arguments: serde_json::Value,
) -> Result<ToolResult> {
    match tool_name {
        "ultragit_read" => self.handle_read(arguments).await,
        "ultragit_deps" => self.handle_deps(arguments).await,
        "ultragit_history" => self.handle_history(arguments).await,
        "ultragit_summary" => self.handle_summary(arguments).await,
        _ => Err(McpError::UnknownTool(tool_name.to_string())),
    }
}
```

Each handler:

1. Parses the `arguments` JSON into the expected struct.
2. Validates the arguments (path exists, anchor is valid, etc.).
3. Calls the corresponding read pipeline function.
4. Serializes the result as JSON.
5. Wraps it in an MCP tool result.

```rust
async fn handle_read(&self, args: serde_json::Value) -> Result<ToolResult> {
    let params: ReadParams = serde_json::from_value(args)?;

    let query = ReadQuery {
        file: params.path,
        anchor: params.anchor,
        lines: params.lines.map(|l| parse_line_range(&l)).transpose()?,
        max_tokens: params.max_tokens,
        depth: 1,           // default
        max_regions: 20,    // default
        ..Default::default()
    };

    let result = self.read_pipeline.execute(query).await?;
    let json = serde_json::to_string(&result)?;

    Ok(ToolResult::text(json))
}
```

### Server Lifecycle

The MCP server is a single-threaded async process. It:

1. Reads initialization parameters from the `initialize` request.
2. Opens the git repository at the configured working directory.
3. Initializes the read pipeline (loads config, prepares tree-sitter parsers).
4. Enters the event loop: read a JSON-RPC message from stdin, dispatch, write the response to stdout.
5. Exits when stdin is closed (client disconnected) or on SIGTERM/SIGINT.

The server is stateless across tool calls — each call is independent. The read pipeline caches tree-sitter parsers and git state as needed, but no state from one tool call affects another.

```rust
pub async fn run(&self) -> Result<()> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut writer = BufWriter::new(stdout);

    loop {
        let message = match read_message(&mut reader).await {
            Ok(msg) => msg,
            Err(e) if e.is_eof() => break,
            Err(e) => {
                eprintln!("Error reading message: {e}");
                continue;
            }
        };

        let response = self.handle_message(message).await;
        write_message(&mut writer, &response).await?;
        writer.flush().await?;
    }

    Ok(())
}
```

### Server Registration

`ultragit mcp install` writes the server configuration to the appropriate MCP config file.

```rust
pub struct McpServerConfig {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
}

/// Install MCP server configuration
pub fn install_mcp_config(
    config_path: &Path,
    repo_root: &Path,
) -> Result<()> {
    let mut config = read_mcp_config(config_path)?;

    config.servers.insert("ultragit".to_string(), McpServerConfig {
        command: "ultragit".to_string(),
        args: vec!["mcp".to_string(), "start".to_string()],
        cwd: Some(repo_root.to_path_buf()),
    });

    write_mcp_config(config_path, &config)?;
    Ok(())
}
```

**Config file locations:**

| Platform | Per-repository | Global (Claude Desktop) |
|----------|---------------|------------------------|
| All | `.mcp.json` in repo root | — |
| macOS | — | `~/Library/Application Support/Claude/claude_desktop_config.json` |
| Linux | — | `~/.config/claude/claude_desktop_config.json` |
| Windows | — | `%APPDATA%\Claude\claude_desktop_config.json` |

The install command creates the config file if it doesn't exist. If it exists, it reads the JSON, adds or updates the `ultragit` entry under `mcpServers`, and writes back. Other server entries are preserved.

### Stdin/Stdout Protocol Details

MCP over stdio uses a line-delimited JSON-RPC protocol. Each message is a single line of JSON followed by a newline. The server reads lines from stdin and writes lines to stdout.

Diagnostic output (logging, warnings) goes to stderr, never stdout. This is critical — stdout is the protocol channel and must contain only valid JSON-RPC messages.

```rust
/// Read a single JSON-RPC message from the reader
async fn read_message(reader: &mut BufReader<Stdin>) -> Result<JsonRpcMessage> {
    let mut line = String::new();
    let bytes_read = reader.read_line(&mut line).await?;
    if bytes_read == 0 {
        return Err(McpError::Eof);
    }
    let message: JsonRpcMessage = serde_json::from_str(line.trim())?;
    Ok(message)
}

/// Write a JSON-RPC response to the writer
async fn write_message(writer: &mut BufWriter<Stdout>, response: &JsonRpcResponse) -> Result<()> {
    let json = serde_json::to_string(response)?;
    writer.write_all(json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    Ok(())
}
```

---

## Error Handling

The MCP server must remain alive across individual tool failures. A bad tool call should return an error response, not crash the server.

| Failure Mode | Handling |
|---|---|
| Tool call with unknown tool name | Return MCP error: `"Unknown tool: <name>"`. Server stays alive. |
| Missing required argument (`path`) | Return MCP tool error with message: `"Missing required argument: path"`. Server stays alive. |
| Path doesn't exist in repository | Return MCP tool result with `isError: true` and message: `"File not found: <path>"`. Server stays alive. |
| Anchor doesn't resolve | Return tool result with no matching regions and a note in the output: `"Anchor '<name>' not found in <path>. Returning file-level annotations."` Fall back to file-level query. |
| No annotations found | Return a valid tool result with empty regions and stats showing 0 annotations. Not an error — the absence of annotations is useful information. |
| Git operations fail (corrupt repo, missing ref) | Return MCP tool error with the git error message. Server stays alive. |
| JSON-RPC parse error on input | Write a JSON-RPC error response (`-32700 Parse error`). Continue reading next message. |
| Invalid JSON-RPC method | Write a JSON-RPC error response (`-32601 Method not found`). Continue reading. |
| Stdin closed (EOF) | Exit cleanly with code 0. |
| SIGTERM/SIGINT | Exit cleanly with code 0. |
| Panic in tool handler | Catch panics at the dispatch level (using `std::panic::catch_unwind` or tokio's panic handling). Return an internal error response. Server stays alive. |

---

## Configuration

The MCP server reads configuration from the repository's `.git/config` and `.ultragit-config.toml`:

| Source | Key | Effect |
|--------|-----|--------|
| CLI arg | `--repo` | Repository root path |
| CLI arg | `--noteref` | Notes ref to read |
| `.git/config` | `ultragit.noteref` | Default notes ref |
| `.git/config` | `ultragit.include` | File include patterns (applied to queries) |
| `.git/config` | `ultragit.exclude` | File exclude patterns (applied to queries) |

The server does not have its own configuration section. It reuses the existing Ultragit configuration.

---

## Implementation Steps

### Step 1: JSON-RPC Message Types
**Scope:** `src/mcp/mod.rs`

- Define JSON-RPC request and response structs with serde.
- Define MCP-specific message types: `initialize`, `tools/list`, `tools/call`.
- Define `ToolResult` struct with `content` and `isError` fields.
- Tests: serialize/deserialize round-trip for all message types.

### Step 2: Stdio Transport
**Scope:** `src/mcp/server.rs`

- Implement `read_message` and `write_message` for line-delimited JSON-RPC over stdin/stdout.
- Implement the main event loop with EOF detection.
- Handle JSON parse errors without crashing.
- Tests: feed valid and invalid JSON lines, verify responses.

### Step 3: Initialize and Tools/List Handlers
**Scope:** `src/mcp/server.rs`

- Implement `initialize` handler: return server info and capabilities (tools).
- Implement `tools/list` handler: return the four tool definitions.
- Tests: verify tool definitions match the expected schema.

### Step 4: ultragit_read Tool
**Scope:** `src/mcp/tools.rs`

- Parse `ultragit_read` arguments.
- Map to `ReadQuery` and call the read pipeline.
- Serialize result as JSON string in MCP tool result format.
- Handle errors (missing path, no annotations, etc.).
- Tests: mock the read pipeline, verify tool input/output mapping.

### Step 5: ultragit_deps Tool
**Scope:** `src/mcp/tools.rs`

- Parse `ultragit_deps` arguments.
- Call the deps query module (Feature 08).
- Serialize result.
- Tests: mock the deps pipeline, verify tool output.

### Step 6: ultragit_history Tool
**Scope:** `src/mcp/tools.rs`

- Parse `ultragit_history` arguments.
- Call the history query module (Feature 08).
- Serialize result.
- Tests: mock the history pipeline, verify tool output.

### Step 7: ultragit_summary Tool
**Scope:** `src/mcp/tools.rs`

- Parse `ultragit_summary` arguments.
- Call the summary query module (Feature 08).
- Serialize result.
- Tests: mock the summary pipeline, verify tool output.

### Step 8: CLI Integration (mcp start, mcp install)
**Scope:** `src/cli/mcp.rs`

- Implement `ultragit mcp start` subcommand that initializes the `McpServer` and calls `run()`.
- Implement `ultragit mcp install` that writes MCP config to the appropriate file.
- Config file discovery logic (per-repo, global, platform-specific paths).
- Idempotent install (don't duplicate entries).
- Tests: install into new config file, install into existing config with other servers, idempotent reinstall.

### Step 9: Error Resilience
**Scope:** `src/mcp/server.rs`

- Add panic catching around tool dispatch.
- Verify server stays alive after tool errors.
- Test with rapid-fire requests including malformed ones.
- Tests: stress test with mixed valid/invalid requests.

---

## Test Plan

### Unit Tests

- **JSON-RPC message parsing:** Parse valid `initialize`, `tools/list`, and `tools/call` messages. Verify error on malformed JSON.
- **Tool argument parsing:** Parse each tool's arguments. Verify error on missing required fields. Verify optional fields default correctly.
- **Tool result serialization:** Verify MCP tool result format with `content` array and `isError` flag.
- **Config file manipulation:** Read existing MCP config, add server entry, write back. Verify other entries preserved. Verify idempotent.
- **Config path discovery:** Verify platform-specific config paths resolve correctly.

### Integration Tests

- **Full protocol handshake:**
  1. Spawn `ultragit mcp start` as a subprocess.
  2. Send `initialize` request, verify response.
  3. Send `tools/list`, verify four tools returned.
  4. Send `tools/call` for each tool with valid arguments.
  5. Verify responses contain expected annotation data.
  6. Close stdin, verify process exits cleanly.

- **ultragit_read via MCP:**
  1. Create a test repo with a commit and annotation.
  2. Start MCP server.
  3. Call `ultragit_read` with the file path.
  4. Verify the response contains the annotation data matching what `ultragit read` CLI would return.

- **ultragit_deps via MCP:**
  1. Create a test repo with annotations containing semantic dependencies.
  2. Call `ultragit_deps` for the target function.
  3. Verify dependencies are returned.

- **ultragit_history via MCP:**
  1. Create a test repo with multiple commits touching the same function.
  2. Call `ultragit_history`.
  3. Verify the timeline is returned in chronological order.

- **ultragit_summary via MCP:**
  1. Create a test repo with multiple annotated functions.
  2. Call `ultragit_summary` for the file.
  3. Verify condensed output with intent and constraints for each region.

- **Error handling end-to-end:**
  1. Call `ultragit_read` with a non-existent file.
  2. Verify error response with `isError: true`.
  3. Call another tool after the error — verify server is still responsive.

- **MCP install round-trip:**
  1. Create a temp directory with no `.mcp.json`.
  2. Run `ultragit mcp install`.
  3. Verify `.mcp.json` was created with correct content.
  4. Run `ultragit mcp install` again.
  5. Verify file unchanged (idempotent).

### Protocol Conformance Tests

- **JSON-RPC error codes:** Verify `-32700` for parse errors, `-32601` for unknown methods, `-32602` for invalid params.
- **Message ordering:** Send multiple requests without waiting for responses (pipelining). Verify responses arrive with correct `id` matching.
- **Notification handling:** Send a JSON-RPC notification (no `id`). Verify server doesn't respond (per spec) and doesn't crash.
- **Large responses:** Query a file with many annotations. Verify the response is valid JSON (no truncation in the transport layer).
- **Binary/non-UTF8 in stdin:** Send invalid bytes. Verify server recovers and continues processing.

### Edge Cases

- Start MCP server in a non-git directory (error on initialize or first tool call).
- Start MCP server in a repo with no annotations (tools return empty results, not errors).
- Call tool with extra unknown arguments (ignored per MCP spec).
- Call tool with empty string path (error).
- MCP server with very large annotation output (verify no stdout buffer overflow).
- Rapid sequential tool calls (verify no state leakage between calls).
- Server process killed with SIGKILL (no graceful cleanup — verify no corruption of git state).

---

## Acceptance Criteria

1. `ultragit mcp start` runs as a long-lived stdio process that speaks JSON-RPC.
2. The server responds to `initialize` with its capabilities (tools list).
3. `tools/list` returns exactly four tools: `ultragit_read`, `ultragit_deps`, `ultragit_history`, `ultragit_summary`.
4. Each tool's `inputSchema` accurately describes its required and optional parameters.
5. `ultragit_read` returns the same annotation data as `ultragit read` CLI for the same query.
6. `ultragit_deps` returns the same dependency data as `ultragit deps` CLI.
7. `ultragit_history` returns the same timeline data as `ultragit history` CLI.
8. `ultragit_summary` returns the same summary data as `ultragit summary` CLI.
9. Tool errors return MCP error responses with `isError: true` and descriptive messages. The server remains alive after errors.
10. The server handles malformed JSON-RPC input without crashing.
11. The server exits cleanly when stdin is closed.
12. `ultragit mcp install` creates or updates MCP configuration with the correct server entry. Installation is idempotent.
13. The server produces no output on stdout except valid JSON-RPC messages. Diagnostic output goes to stderr.
14. The server has sub-second latency for typical single-file queries (same as CLI read path — no protocol overhead beyond JSON serialization).
15. An MCP-connected agent (e.g., Claude Desktop or Claude Code) can discover and invoke Ultragit tools after running `ultragit mcp install`.
