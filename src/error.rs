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
}

pub type Result<T, E = ChronicleError> = std::result::Result<T, E>;
