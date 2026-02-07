use snafu::Snafu;
use std::path::PathBuf;

#[derive(Debug, Snafu)]
#[snafu(visibility(pub), module(chronicle_error))]
pub enum ChronicleError {
    #[snafu(display("not a git repository: {}", path.display()))]
    NotARepository {
        path: PathBuf,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("chronicle not initialized (run `git chronicle init` first)"))]
    NotInitialized {
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("git error: {source}"))]
    Git {
        source: GitError,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("provider error: {source}"))]
    Provider {
        source: ProviderError,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("agent error: {source}"))]
    Agent {
        source: AgentError,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("AST error: {source}"))]
    Ast {
        source: AstError,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("config error: {message}"))]
    Config {
        message: String,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("IO error: {source}"))]
    Io {
        source: std::io::Error,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("JSON error: {source}"))]
    Json {
        source: serde_json::Error,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("annotation validation error: {message}"))]
    Validation {
        message: String,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("setup error: {source}"))]
    Setup {
        source: SetupError,
        #[snafu(implicit)]
        location: snafu::Location,
    },
}

#[derive(Debug, Snafu)]
#[snafu(visibility(pub), module(git_error))]
pub enum GitError {
    #[snafu(display("git command failed: {message}"))]
    CommandFailed {
        message: String,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("commit not found: {sha}"))]
    CommitNotFound {
        sha: String,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("file not found: {path} at {commit}"))]
    FileNotFound {
        path: String,
        commit: String,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("notes ref missing: {refname}"))]
    NotesRefMissing {
        refname: String,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("diff parse error: {message}"))]
    DiffParse {
        message: String,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("IO error: {source}"))]
    Io {
        source: std::io::Error,
        #[snafu(implicit)]
        location: snafu::Location,
    },
}

#[derive(Debug, Snafu)]
#[snafu(visibility(pub), module(provider_error))]
pub enum ProviderError {
    #[snafu(display("no credentials found for any provider"))]
    NoCredentials {
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("authentication failed: {message}"))]
    AuthFailed {
        message: String,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("rate limited, retry after {retry_after_secs}s"))]
    RateLimited {
        retry_after_secs: u64,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("request timeout"))]
    Timeout {
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("API error: {message}"))]
    Api {
        message: String,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("failed to parse response: {message}"))]
    ParseResponse {
        message: String,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("HTTP error: {source}, at {location}"))]
    Http {
        source: Box<ureq::Transport>,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("retries exhausted after {attempts} attempts"))]
    RetriesExhausted {
        attempts: u32,
        #[snafu(implicit)]
        location: snafu::Location,
    },
}

#[derive(Debug, Snafu)]
#[snafu(visibility(pub), module(agent_error))]
pub enum AgentError {
    #[snafu(display("provider error: {source}"))]
    Provider {
        source: ProviderError,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("no annotations emitted by agent"))]
    NoAnnotations {
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("max turns exceeded ({turns})"))]
    MaxTurnsExceeded {
        turns: u32,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("invalid annotation: {message}"))]
    InvalidAnnotation {
        message: String,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("git error: {source}"))]
    Git {
        source: GitError,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("JSON error: {source}"))]
    Json {
        source: serde_json::Error,
        #[snafu(implicit)]
        location: snafu::Location,
    },
}

#[derive(Debug, Snafu)]
#[snafu(visibility(pub), module(ast_error))]
pub enum AstError {
    #[snafu(display("unsupported language: {extension}"))]
    UnsupportedLanguage {
        extension: String,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("parse failed for {path}: {message}"))]
    ParseFailed {
        path: String,
        message: String,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("tree-sitter error: {message}"))]
    TreeSitter {
        message: String,
        #[snafu(implicit)]
        location: snafu::Location,
    },
}

#[derive(Debug, Snafu)]
#[snafu(visibility(pub), module(setup_error))]
pub enum SetupError {
    #[snafu(display("home directory not found, at {location}"))]
    NoHomeDirectory {
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("git-chronicle binary not found on PATH, at {location}"))]
    BinaryNotFound {
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("failed to write {path}: {source}, at {location}"))]
    WriteFile {
        path: String,
        #[snafu(source)]
        source: std::io::Error,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("failed to read file {path}: {source}, at {location}"))]
    ReadFile {
        path: String,
        #[snafu(source)]
        source: std::io::Error,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("failed to read user config: {source}, at {location}"))]
    ReadConfig {
        #[snafu(source)]
        source: toml::de::Error,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("failed to write user config: {source}, at {location}"))]
    WriteConfig {
        #[snafu(source)]
        source: toml::ser::Error,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display(
        "Claude CLI not found — install Claude Code or select a different provider, at {location}"
    ))]
    ClaudeCliNotFound {
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("ANTHROPIC_API_KEY environment variable not set, at {location}"))]
    ApiKeyNotSet {
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("interactive input error: {source}, at {location}"))]
    InteractiveInput {
        #[snafu(source)]
        source: std::io::Error,
        #[snafu(implicit)]
        location: snafu::Location,
    },
}

pub type Result<T, E = ChronicleError> = std::result::Result<T, E>;
