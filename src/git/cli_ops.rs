use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::git_error::{CommandFailedSnafu, CommitNotFoundSnafu, FileNotFoundSnafu};
use crate::error::GitError;
use crate::git::diff::parse_diff;
use crate::git::{CommitInfo, FileDiff, GitOps};

/// Git operations implemented by shelling out to the `git` CLI.
pub struct CliOps {
    pub repo_dir: PathBuf,
    pub notes_ref: String,
}

impl CliOps {
    pub fn new(repo_dir: PathBuf) -> Self {
        Self {
            repo_dir,
            notes_ref: "refs/notes/ultragit".to_string(),
        }
    }

    pub fn with_notes_ref(mut self, notes_ref: String) -> Self {
        self.notes_ref = notes_ref;
        self
    }

    /// Run a git command and return stdout on success, or an error with stderr.
    fn run_git(&self, args: &[&str]) -> Result<String, GitError> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.repo_dir)
            .output()
            .map_err(|e| {
                CommandFailedSnafu {
                    message: format!("failed to run git: {e}"),
                }
                .build()
            })?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(CommandFailedSnafu {
                message: stderr.trim().to_string(),
            }
            .build())
        }
    }

    /// Run git and return (success, stdout, stderr) without failing on non-zero exit.
    fn run_git_raw(&self, args: &[&str]) -> Result<(bool, String, String), GitError> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.repo_dir)
            .output()
            .map_err(|e| {
                CommandFailedSnafu {
                    message: format!("failed to run git: {e}"),
                }
                .build()
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Ok((output.status.success(), stdout, stderr))
    }
}

impl GitOps for CliOps {
    fn diff(&self, commit: &str) -> Result<Vec<FileDiff>, GitError> {
        // Check if this is a root commit (no parents)
        let info = self.commit_info(commit)?;
        let diff_output = if info.parent_shas.is_empty() {
            // Root commit: use --root flag
            self.run_git(&["diff-tree", "--root", "-p", "--no-color", "-M", commit])?
        } else {
            self.run_git(&["diff-tree", "-p", "--no-color", "-M", commit])?
        };

        parse_diff(&diff_output)
    }

    fn note_read(&self, commit: &str) -> Result<Option<String>, GitError> {
        let (success, stdout, _stderr) =
            self.run_git_raw(&["notes", "--ref", &self.notes_ref, "show", commit])?;

        if success {
            Ok(Some(stdout))
        } else {
            Ok(None)
        }
    }

    fn note_write(&self, commit: &str, content: &str) -> Result<(), GitError> {
        // Use a tempfile to avoid shell escaping issues with note content
        let tmp_dir = self.repo_dir.join(".git").join("ultragit");
        std::fs::create_dir_all(&tmp_dir).map_err(|e| {
            CommandFailedSnafu {
                message: format!("failed to create temp dir: {e}"),
            }
            .build()
        })?;

        let tmp_path = tmp_dir.join("note-tmp.json");
        std::fs::write(&tmp_path, content).map_err(|e| {
            CommandFailedSnafu {
                message: format!("failed to write temp file: {e}"),
            }
            .build()
        })?;

        let tmp_path_str = tmp_path.to_string_lossy();

        // Try add first, if that fails (note exists), use add --force
        let result = self.run_git(&[
            "notes",
            "--ref",
            &self.notes_ref,
            "add",
            "-f",
            "-F",
            &tmp_path_str,
            commit,
        ]);

        // Clean up temp file regardless of result
        let _ = std::fs::remove_file(&tmp_path);

        result?;
        Ok(())
    }

    fn note_exists(&self, commit: &str) -> Result<bool, GitError> {
        let (success, _stdout, _stderr) =
            self.run_git_raw(&["notes", "--ref", &self.notes_ref, "show", commit])?;
        Ok(success)
    }

    fn file_at_commit(&self, path: &Path, commit: &str) -> Result<String, GitError> {
        let path_str = path.to_string_lossy();
        let object = format!("{commit}:{path_str}");
        let (success, stdout, stderr) = self.run_git_raw(&["show", &object])?;

        if success {
            Ok(stdout)
        } else {
            if stderr.contains("does not exist") || stderr.contains("fatal: path") {
                return Err(FileNotFoundSnafu {
                    path: path_str.to_string(),
                    commit: commit.to_string(),
                }
                .build());
            }
            Err(CommandFailedSnafu {
                message: stderr.trim().to_string(),
            }
            .build())
        }
    }

    fn commit_info(&self, commit: &str) -> Result<CommitInfo, GitError> {
        // Use a custom format to get all info in one call
        // %H = sha, %s = subject, %an = author name, %ae = author email, %aI = author date ISO, %P = parent hashes
        let (success, stdout, stderr) = self.run_git_raw(&[
            "log",
            "-1",
            "--format=%H%n%s%n%an%n%ae%n%aI%n%P",
            commit,
        ])?;

        if !success {
            if stderr.contains("unknown revision") || stderr.contains("bad object") {
                return Err(CommitNotFoundSnafu {
                    sha: commit.to_string(),
                }
                .build());
            }
            return Err(CommandFailedSnafu {
                message: stderr.trim().to_string(),
            }
            .build());
        }

        let lines: Vec<&str> = stdout.lines().collect();
        if lines.len() < 5 {
            return Err(CommandFailedSnafu {
                message: format!("unexpected git log output for {commit}"),
            }
            .build());
        }

        let parent_shas: Vec<String> = if lines.len() > 5 && !lines[5].is_empty() {
            lines[5].split(' ').map(|s| s.to_string()).collect()
        } else {
            Vec::new()
        };

        Ok(CommitInfo {
            sha: lines[0].to_string(),
            message: lines[1].to_string(),
            author_name: lines[2].to_string(),
            author_email: lines[3].to_string(),
            timestamp: lines[4].to_string(),
            parent_shas,
        })
    }

    fn resolve_ref(&self, refspec: &str) -> Result<String, GitError> {
        let output = self.run_git(&["rev-parse", refspec])?;
        Ok(output.trim().to_string())
    }

    fn config_get(&self, key: &str) -> Result<Option<String>, GitError> {
        let (success, stdout, _stderr) = self.run_git_raw(&["config", "--get", key])?;
        if success {
            let val = stdout.trim().to_string();
            if val.is_empty() {
                Ok(None)
            } else {
                Ok(Some(val))
            }
        } else {
            // git config --get exits with 1 when key is not found, which is not an error
            Ok(None)
        }
    }

    fn config_set(&self, key: &str, value: &str) -> Result<(), GitError> {
        self.run_git(&["config", key, value])?;
        Ok(())
    }

    fn log_for_file(&self, path: &str) -> Result<Vec<String>, GitError> {
        let output = self.run_git(&["log", "--follow", "--format=%H", "--", path])?;
        let shas: Vec<String> = output
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect();
        Ok(shas)
    }
}
