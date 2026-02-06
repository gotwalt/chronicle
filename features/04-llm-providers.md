# Feature 04: LLM Provider Abstraction

## Overview

The LLM provider layer is the HTTP interface between Ultragit and the language models that power annotation. It abstracts over four providers — Anthropic, OpenAI, Gemini, and OpenRouter — behind a single `LlmProvider` trait, normalizing request/response formats, tool-use message types, credential discovery, retry logic, and error handling.

This layer exists so that the writing agent (Feature 05) and any future LLM-calling code can work against a single interface without caring which provider is active. The provider is selected once at startup based on available credentials and configuration, then used throughout the session.

No third-party SDK dependencies. Each provider is a thin HTTP client built on `reqwest` + `tokio` + `serde`. The APIs are simple enough that a bespoke client is more maintainable than tracking SDK version churn across four providers.

---

## Dependencies

| Feature | What it provides |
|---------|-----------------|
| 01 CLI & Config | `UltragitConfig` for reading `provider`, `model`, API key overrides from `.git/config` and `.ultragit-config.toml` |

No dependency on 02 (Git Operations) or 03 (AST Parsing). This feature is purely about HTTP + LLM protocol normalization.

---

## Public API

### Core Trait

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Send a completion request and return the model's response.
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, ProviderError>;

    /// Whether this provider supports native tool use (function calling).
    fn supports_tool_use(&self) -> bool;

    /// Provider name for logging and diagnostics.
    fn name(&self) -> &str;

    /// Model identifier currently configured.
    fn model(&self) -> &str;

    /// Validate that credentials are working. Used by `ultragit auth check`.
    async fn check_auth(&self) -> Result<AuthStatus, ProviderError>;
}
```

### Message Types

```rust
/// Role in the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// A single content block within a message.
/// Models produce text and tool-use blocks; tool results come back as ToolResult blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: bool,
    },
}

/// A message in the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

/// A tool the model can call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value, // JSON Schema object
}
```

### Request/Response

```rust
/// Normalized completion request sent to any provider.
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub system: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    pub max_tokens: u32,
    pub temperature: Option<f32>,
    /// If true and the provider doesn't support tool use,
    /// fall back to structured-output mode.
    pub structured_fallback: bool,
}

/// Normalized completion response from any provider.
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    /// The content blocks the model produced.
    pub content: Vec<ContentBlock>,
    /// Why the model stopped generating.
    pub stop_reason: StopReason,
    /// Token usage for budget tracking.
    pub usage: TokenUsage,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StopReason {
    /// Model finished naturally.
    EndTurn,
    /// Model wants to call one or more tools.
    ToolUse,
    /// Hit the max_tokens limit.
    MaxTokens,
    /// Unknown/provider-specific reason.
    Other(String),
}

#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct AuthStatus {
    pub provider: String,
    pub model: String,
    pub authenticated: bool,
    pub message: String,
}
```

### Provider Construction

```rust
/// Discover credentials and construct the appropriate provider.
/// Returns the first provider with valid credentials, following the priority chain.
pub async fn discover_provider(config: &UltragitConfig) -> Result<Box<dyn LlmProvider>, ProviderError>;

/// Construct a specific provider by name. Used when the user pins a provider in config.
pub fn build_provider(
    provider_name: &str,
    api_key: &str,
    model: &str,
) -> Result<Box<dyn LlmProvider>, ProviderError>;
```

### Error Types

```rust
use snafu::{Snafu, ResultExt, Location};

#[derive(Debug, Snafu)]
pub enum ProviderError {
    #[snafu(display("No LLM credentials found, at {location}"))]
    NoCredentials {
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Authentication failed for {provider}: {message}, at {location}"))]
    AuthFailed {
        provider: String,
        message: String,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Rate limited by {provider}, retry after {retry_after_secs}s, at {location}"))]
    RateLimited {
        provider: String,
        retry_after_secs: u64,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Request to {provider} timed out after {timeout_secs}s, at {location}"))]
    Timeout {
        provider: String,
        timeout_secs: u64,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Provider {provider} returned error: {status} {body}, at {location}"))]
    ApiError {
        provider: String,
        status: u16,
        body: String,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Failed to parse response from {provider}: {message}, at {location}"))]
    ParseResponse {
        provider: String,
        message: String,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("HTTP error communicating with {provider}, at {location}"))]
    Http {
        provider: String,
        #[snafu(source)]
        source: reqwest::Error,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Provider {provider} does not support tool use and structured fallback is disabled, at {location}"))]
    NoToolUseSupport {
        provider: String,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Max retries ({max_retries}) exceeded for {provider}, at {location}"))]
    RetriesExhausted {
        provider: String,
        max_retries: u32,
        #[snafu(implicit)]
        location: Location,
    },
}
```

### CLI: `ultragit auth check`

```
ultragit auth check

Discovers credentials using the priority chain and validates them by making
a minimal API call to the selected provider.

Output on success:
  ✓ Anthropic API key found (ANTHROPIC_API_KEY)
    Model: claude-sonnet-4-5-20250929
    Testing connection... ✓ OK

Output on failure:
  ✗ No LLM credentials found.

    Set one of the following:
      export ANTHROPIC_API_KEY=sk-ant-...    (preferred)
      export OPENAI_API_KEY=sk-...
      export OPENROUTER_API_KEY=...

    Or install Claude Code to use subscription credentials:
      https://docs.anthropic.com/claude-code
```

---

## Internal Design

### Credential Discovery Chain

The `discover_provider` function walks the following chain, returning the first provider with valid credentials:

| Priority | Check | Provider | Default Model |
|----------|-------|----------|--------------|
| 1 | `ANTHROPIC_API_KEY` env var | Anthropic | `claude-sonnet-4-5-20250929` |
| 2 | Claude CLI credentials at `~/.config/claude/` | Anthropic | `claude-sonnet-4-5-20250929` |
| 3 | `OPENAI_API_KEY` env var | OpenAI | `gpt-4o` |
| 4 | `GOOGLE_API_KEY` or `GEMINI_API_KEY` env var | Gemini | `gemini-2.0-flash` |
| 5 | `OPENROUTER_API_KEY` env var | OpenRouter | `anthropic/claude-sonnet-4-5-20250929` |
| 6 | `ULTRAGIT_API_KEY` + `ULTRAGIT_PROVIDER` env vars | Explicit | Per provider |

If `config.provider` is set (user pinned a provider), skip the chain and construct that provider directly, failing if credentials for it are missing.

**Claude CLI credential discovery:** Check for `~/.config/claude/credentials.json` (or platform equivalent). The exact format needs verification — it may contain an OAuth token, a session key, or a standard API key. Parse what's there and attempt to use it as a bearer token against the Anthropic API. If the file doesn't exist or parsing fails, skip to the next entry in the chain.

### Provider Implementations

Each provider struct holds a `reqwest::Client`, the API key, the model name, and provider-specific configuration.

#### Anthropic (`AnthropicProvider`)

**Endpoint:** `https://api.anthropic.com/v1/messages`

**Headers:**
- `x-api-key: <key>`
- `anthropic-version: 2023-06-01`
- `content-type: application/json`

**Request mapping:**
- `system` → top-level `system` field (string or content blocks)
- `messages` → `messages` array, each with `role` and `content`
- `tools` → `tools` array; each tool has `name`, `description`, `input_schema`
- Tool use responses from the model arrive as `content` blocks with `type: "tool_use"`, containing `id`, `name`, `input`
- Tool results are sent back as a user message with `content` blocks of `type: "tool_result"`, containing `tool_use_id`, `content`

**Response mapping:**
- `content` blocks → `Vec<ContentBlock>` (text blocks become `ContentBlock::Text`, tool_use blocks become `ContentBlock::ToolUse`)
- `stop_reason: "end_turn"` → `StopReason::EndTurn`
- `stop_reason: "tool_use"` → `StopReason::ToolUse`
- `stop_reason: "max_tokens"` → `StopReason::MaxTokens`
- `usage.input_tokens`, `usage.output_tokens` → `TokenUsage`

#### OpenAI (`OpenAiProvider`)

**Endpoint:** `https://api.openai.com/v1/chat/completions`

**Headers:**
- `Authorization: Bearer <key>`
- `content-type: application/json`

**Request mapping:**
- `system` → first message with `role: "system"`
- `messages` → `messages` array
- `tools` → `tools` array; each tool is `{ type: "function", function: { name, description, parameters } }` where `parameters` is the `input_schema`
- Tool use responses from the model arrive in `message.tool_calls`, each with `id`, `function.name`, `function.arguments` (JSON string)
- Tool results are sent as messages with `role: "tool"`, `tool_call_id`, `content`

**Response mapping:**
- `choices[0].message.content` → `ContentBlock::Text` (if present)
- `choices[0].message.tool_calls` → `Vec<ContentBlock::ToolUse>`, parsing `function.arguments` from JSON string to `serde_json::Value`
- `choices[0].finish_reason: "stop"` → `StopReason::EndTurn`
- `choices[0].finish_reason: "tool_calls"` → `StopReason::ToolUse`
- `choices[0].finish_reason: "length"` → `StopReason::MaxTokens`
- `usage.prompt_tokens`, `usage.completion_tokens` → `TokenUsage`

#### Gemini (`GeminiProvider`)

**Endpoint:** `https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent`

**Headers:**
- `x-goog-api-key: <key>` (query param `key=` also works)
- `content-type: application/json`

**Request mapping:**
- `system` → `system_instruction.parts[0].text`
- `messages` → `contents` array; each with `role` ("user" or "model") and `parts`
- `tools` → `tools[0].function_declarations` array; each has `name`, `description`, `parameters` (OpenAPI-style schema)
- Tool use responses arrive as `parts` with `functionCall: { name, args }`
- Tool results are sent as user content with `parts` containing `functionResponse: { name, response }`

**Response mapping:**
- `candidates[0].content.parts` → iterate: text parts become `ContentBlock::Text`, functionCall parts become `ContentBlock::ToolUse` (generate a UUID for the `id` since Gemini doesn't provide one; track the mapping internally for tool result correlation)
- `candidates[0].finishReason: "STOP"` → `StopReason::EndTurn`
- functionCall presence → `StopReason::ToolUse`
- `candidates[0].finishReason: "MAX_TOKENS"` → `StopReason::MaxTokens`
- `usageMetadata.promptTokenCount`, `usageMetadata.candidatesTokenCount` → `TokenUsage`

#### OpenRouter (`OpenRouterProvider`)

**Endpoint:** `https://openrouter.ai/api/v1/chat/completions`

OpenRouter is OpenAI-compatible. The `OpenRouterProvider` wraps `OpenAiProvider` with:
- Different base URL
- `Authorization: Bearer <OPENROUTER_API_KEY>`
- `HTTP-Referer` and `X-Title` headers for OpenRouter attribution
- Model names use provider prefixes: `anthropic/claude-sonnet-4-5-20250929`

Internally, `OpenRouterProvider` contains an `OpenAiProvider` and delegates to it after setting headers. This avoids duplicating the OpenAI request/response mapping.

### Structured-Output Fallback

When `CompletionRequest::structured_fallback` is true and the provider either doesn't support tool use or tool use fails with an unsupported error:

1. Remove `tools` from the request.
2. Append to the system prompt: instructions to emit JSON in a specific schema instead of calling tools.
3. Parse the model's text response as JSON.
4. Wrap parsed tool calls as synthetic `ContentBlock::ToolUse` blocks so the agent loop doesn't need to know the difference.

This is a degraded path — the model may produce malformed JSON or ignore the schema. The caller should validate and retry once before failing.

### Retry and Backoff

All providers share a common retry wrapper:

```rust
struct RetryConfig {
    /// Maximum number of retry attempts.
    max_retries: u32,         // default: 3
    /// Initial backoff duration.
    initial_backoff: Duration, // default: 1s
    /// Backoff multiplier per retry.
    backoff_multiplier: f64,   // default: 2.0
    /// Maximum backoff duration.
    max_backoff: Duration,     // default: 30s
    /// Request timeout per attempt.
    request_timeout: Duration, // default: 60s
    /// Add jitter to backoff to avoid thundering herd.
    jitter: bool,              // default: true
}
```

Retry conditions:
- HTTP 429 (rate limited): retry after `Retry-After` header value, or backoff.
- HTTP 500, 502, 503, 529 (server errors): retry with backoff.
- Network errors (connection refused, timeout): retry with backoff.
- HTTP 401, 403 (auth errors): do NOT retry. Return `ProviderError::AuthFailed` immediately.
- HTTP 400 (bad request): do NOT retry. Return `ProviderError::ApiError` immediately.

The retry wrapper is generic and wraps the HTTP call, not the entire `complete` method. This means each individual HTTP request within a tool-use conversation loop gets its own retry budget.

### Rate Limiting

In addition to reactive retry-on-429, implement a simple token-bucket rate limiter per provider:

```rust
struct RateLimiter {
    requests_per_minute: u32,  // default: 50 for Anthropic, 60 for OpenAI
    tokens: AtomicU32,
    last_refill: AtomicU64,
}
```

This prevents hitting rate limits in the first place during high-throughput operations like backfill. The limiter is configurable:

```ini
[ultragit]
    rateLimit = 50  # requests per minute
```

### Timeout Handling

Each HTTP request has a per-attempt timeout (`request_timeout`, default 60s). The entire `complete` call has an outer timeout equal to `request_timeout * (max_retries + 1)` to bound the total wall time. If the outer timeout fires, return `ProviderError::Timeout`.

For the writing agent's tool-use loop, the agent orchestrator (Feature 05) manages an overall budget. The provider layer is unaware of the multi-turn conversation; it just handles individual request/response pairs.

### reqwest Client Configuration

A single `reqwest::Client` is shared per provider instance (connection pooling, keep-alive). Configuration:

```rust
let client = reqwest::Client::builder()
    .timeout(Duration::from_secs(60))
    .connect_timeout(Duration::from_secs(10))
    .pool_max_idle_per_host(2)
    .user_agent(format!("ultragit/{}", env!("CARGO_PKG_VERSION")))
    .build()?;
```

Use `rustls` for TLS (no OpenSSL dependency, simplifies static linking).

---

## Error Handling

| Failure Mode | Handling |
|-------------|----------|
| No credentials found | `discover_provider` returns `ProviderError::NoCredentials`. Caller (hook) logs warning and exits without annotating. |
| Invalid API key | First API call returns 401. `ProviderError::AuthFailed`. No retry. Hook logs error to `.git/ultragit/failed.log`. |
| Rate limited | Retry with backoff, respecting `Retry-After`. After `max_retries`, return `ProviderError::RetriesExhausted`. |
| Server error (5xx) | Retry with exponential backoff + jitter. After `max_retries`, return `ProviderError::RetriesExhausted`. |
| Network timeout | Retry with backoff. After `max_retries`, return `ProviderError::Timeout`. |
| Malformed response | `ProviderError::ParseResponse`. Do not retry (likely a provider bug, not transient). |
| Provider doesn't support tool use | If `structured_fallback` is enabled, use fallback mode. Otherwise, `ProviderError::NoToolUseSupport`. |
| Claude CLI credentials expired/invalid | Skip to next credential in chain. If no other credentials, `ProviderError::NoCredentials`. |

All errors include the provider name for diagnostics. The hook's responsibility is to catch `ProviderError` and decide whether to log-and-continue (for async annotation) or surface to the user (for `ultragit auth check` or `ultragit annotate --sync`).

---

## Configuration

### Git Config (`[ultragit]` section)

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `provider` | string | (auto-discover) | Pin a specific provider: `anthropic`, `openai`, `gemini`, `openrouter` |
| `model` | string | (per-provider default) | Model identifier |
| `rateLimit` | integer | 50 | Max requests per minute |
| `timeout` | integer | 60 | Per-request timeout in seconds |
| `maxRetries` | integer | 3 | Maximum retry attempts |

### Environment Variables

| Variable | Description |
|----------|-------------|
| `ANTHROPIC_API_KEY` | Anthropic API key |
| `OPENAI_API_KEY` | OpenAI API key |
| `GOOGLE_API_KEY` | Google/Gemini API key |
| `GEMINI_API_KEY` | Alias for `GOOGLE_API_KEY` |
| `OPENROUTER_API_KEY` | OpenRouter API key |
| `ULTRAGIT_API_KEY` | Explicit override API key |
| `ULTRAGIT_PROVIDER` | Explicit override provider name |
| `ULTRAGIT_MODEL` | Explicit override model (used with `ULTRAGIT_PROVIDER`) |

---

## Implementation Steps

### Step 1: Core types and trait definition
- Define `LlmProvider` trait, `CompletionRequest`, `CompletionResponse`, `Message`, `ContentBlock`, `ToolDefinition`, `StopReason`, `TokenUsage`, `AuthStatus`, `ProviderError`.
- Define `RetryConfig` and `RateLimiter` structs.
- Add to `src/provider/mod.rs`.
- **PR scope:** Types only, no implementations.

### Step 2: Anthropic provider implementation
- Implement `AnthropicProvider` with request/response serialization.
- Implement `complete()`, `check_auth()`, `supports_tool_use()` (returns `true`).
- Define Anthropic-specific serde types for request/response JSON.
- Unit tests with recorded HTTP responses.
- **PR scope:** `src/provider/anthropic.rs` + tests.

### Step 3: Retry wrapper and rate limiter
- Implement generic async retry wrapper with exponential backoff + jitter.
- Implement `RateLimiter` with token-bucket algorithm.
- Integrate retry wrapper into `AnthropicProvider::complete()`.
- Unit tests: verify retry on 429, 500, 503; no retry on 401, 400; backoff timing; jitter.
- **PR scope:** retry logic in `src/provider/mod.rs`, integration into anthropic.rs.

### Step 4: OpenAI provider implementation
- Implement `OpenAiProvider` with request/response serialization.
- Map tool_calls (JSON string arguments) to `ContentBlock::ToolUse`.
- Map `role: "tool"` messages for tool results.
- Unit tests with recorded HTTP responses.
- **PR scope:** `src/provider/openai.rs` + tests.

### Step 5: Gemini provider implementation
- Implement `GeminiProvider` with request/response serialization.
- Handle the function_call/functionResponse parts format.
- Generate synthetic tool-use IDs (Gemini doesn't provide them).
- Unit tests with recorded HTTP responses.
- **PR scope:** `src/provider/gemini.rs` + tests.

### Step 6: OpenRouter provider implementation
- Implement `OpenRouterProvider` wrapping `OpenAiProvider` with different base URL and headers.
- Unit tests verifying correct header injection and model name handling.
- **PR scope:** `src/provider/openrouter.rs` + tests.

### Step 7: Credential discovery
- Implement `discover_provider()` — walk the chain, check env vars, check Claude CLI credentials file, construct the first valid provider.
- Implement `build_provider()` for explicit construction.
- Handle config overrides (pinned provider/model).
- Unit tests: mock env vars, verify chain priority, verify config override behavior.
- Integration test: verify `discover_provider` works end-to-end with a real env var set.
- **PR scope:** `discover_provider` and `build_provider` in `src/provider/mod.rs`.

### Step 8: Structured-output fallback
- Implement fallback mode: strip tools, inject JSON schema instructions into system prompt, parse text response as JSON, synthesize `ContentBlock::ToolUse` blocks.
- Unit tests: verify prompt injection, JSON parsing, error handling on malformed JSON.
- **PR scope:** `src/provider/mod.rs` (or `src/agent/structured.rs` if cleaner).

### Step 9: `ultragit auth check` CLI command
- Wire up the `auth check` subcommand to call `discover_provider()` then `provider.check_auth()`.
- Format output (success/failure, provider name, model, connection test result).
- **PR scope:** `src/cli/auth.rs`.

### Step 10: Integration tests
- Create integration tests that exercise the full provider pipeline with recorded HTTP responses (using `wiremock` or similar).
- Test: Anthropic tool-use conversation (multi-turn), OpenAI tool-use conversation, Gemini tool-use conversation, retry on transient failure, credential discovery priority.
- **PR scope:** `tests/integration/provider_test.rs`.

---

## Test Plan

### Unit Tests

**Per provider (Anthropic, OpenAI, Gemini, OpenRouter):**
- Request serialization: verify the outgoing JSON matches the provider's API spec.
- Response deserialization: verify parsing of text responses, tool-use responses, error responses.
- Tool-use round-trip: serialize a tool-use response from the model, then serialize the tool result back.
- Edge cases: empty content, multiple tool calls in one response, very long text content.

**Retry wrapper:**
- Retries on 429 with `Retry-After` header.
- Retries on 500, 502, 503 with exponential backoff.
- Does not retry on 401, 400.
- Respects `max_retries` limit.
- Jitter produces non-deterministic but bounded delays.
- Outer timeout fires when total time exceeds budget.

**Rate limiter:**
- Permits requests within budget.
- Blocks (async wait) when budget exhausted.
- Refills over time.

**Credential discovery:**
- Returns Anthropic when `ANTHROPIC_API_KEY` is set.
- Skips Anthropic, returns OpenAI when only `OPENAI_API_KEY` is set.
- Returns explicit override when `ULTRAGIT_PROVIDER` + `ULTRAGIT_API_KEY` are set, even if `ANTHROPIC_API_KEY` is also set.
- Returns `NoCredentials` when nothing is set.
- Config-pinned provider overrides the chain.

**Structured fallback:**
- Strips tools and injects schema instructions.
- Parses valid JSON response into synthetic tool-use blocks.
- Returns error on malformed JSON.

### Integration Tests

- End-to-end provider round-trip with `wiremock` recorded responses.
- Multi-turn tool-use conversation: model calls tool, tool result returned, model calls another tool, model emits final text.
- Retry integration: first request returns 429, second succeeds.
- Auth check command output verification.

### Edge Cases

- API key with trailing whitespace or newline (trim it).
- Claude CLI credentials file exists but is empty or corrupted.
- Provider returns valid JSON but unexpected schema (missing fields).
- Provider returns 200 with an error body (Gemini does this for some errors).
- Network partition mid-response (partial body).
- Very large response (>1MB) — verify no OOM or parse failure.
- Concurrent `complete` calls from backfill — verify rate limiter works across tasks.

---

## Acceptance Criteria

1. `discover_provider()` correctly walks the credential chain and returns the first available provider.
2. All four providers (Anthropic, OpenAI, Gemini, OpenRouter) correctly serialize requests and deserialize responses for both text-only and tool-use conversations.
3. Tool-use content blocks are normalized: regardless of provider, the agent loop receives `ContentBlock::ToolUse` with `id`, `name`, `input` and sends back `ContentBlock::ToolResult` with `tool_use_id`, `content`.
4. Retry logic handles 429, 5xx, and network errors with exponential backoff + jitter, respecting `max_retries`.
5. Auth errors (401/403) fail immediately without retry.
6. Rate limiter prevents exceeding configured requests-per-minute.
7. `ultragit auth check` discovers credentials, identifies the provider and model, makes a test API call, and reports success or failure with actionable guidance.
8. Structured-output fallback works for providers without tool use.
9. All provider tests pass with recorded HTTP responses (no live API calls in CI).
10. Request timeout and outer timeout both function correctly.
11. `ProviderError` variants carry enough context (provider name, status code, body) for actionable error messages.
