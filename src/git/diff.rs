use serde::{Deserialize, Serialize};

use crate::error::git_error::DiffParseSnafu;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiffStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone)]
pub struct FileDiff {
    pub path: String,
    pub old_path: Option<String>,
    pub status: DiffStatus,
    pub hunks: Vec<Hunk>,
}

#[derive(Debug, Clone)]
pub struct Hunk {
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    pub header: String,
    pub lines: Vec<HunkLine>,
}

#[derive(Debug, Clone)]
pub enum HunkLine {
    Context(String),
    Added(String),
    Removed(String),
}

impl FileDiff {
    pub fn added_line_count(&self) -> usize {
        self.hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter(|l| matches!(l, HunkLine::Added(_)))
            .count()
    }

    pub fn removed_line_count(&self) -> usize {
        self.hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter(|l| matches!(l, HunkLine::Removed(_)))
            .count()
    }

    pub fn changed_line_count(&self) -> usize {
        self.added_line_count() + self.removed_line_count()
    }
}

/// Parse unified diff output into structured FileDiff objects.
pub fn parse_diff(diff_output: &str) -> Result<Vec<FileDiff>, crate::error::GitError> {
    let mut files: Vec<FileDiff> = Vec::new();
    let lines: Vec<&str> = diff_output.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Look for "diff --git a/... b/..."
        if !line.starts_with("diff --git ") {
            i += 1;
            continue;
        }

        // Parse the file paths from the diff header
        let (a_path, b_path) = parse_diff_header(line)?;

        let mut status = DiffStatus::Modified;
        let mut old_path: Option<String> = None;
        let mut new_path = b_path.clone();
        i += 1;

        // Parse extended headers (new file, deleted file, rename, etc.)
        while i < lines.len()
            && !lines[i].starts_with("diff --git ")
            && !lines[i].starts_with("@@")
            && !lines[i].starts_with("--- ")
        {
            let hdr = lines[i];
            if hdr.starts_with("new file mode") {
                status = DiffStatus::Added;
            } else if hdr.starts_with("deleted file mode") {
                status = DiffStatus::Deleted;
            } else if hdr.starts_with("rename from ") {
                old_path = Some(hdr.trim_start_matches("rename from ").to_string());
                status = DiffStatus::Renamed;
            } else if hdr.starts_with("rename to ") {
                new_path = hdr.trim_start_matches("rename to ").to_string();
            }
            i += 1;
        }

        // Parse --- and +++ lines
        if i < lines.len() && lines[i].starts_with("--- ") {
            let minus_path = &lines[i][4..];
            if minus_path == "/dev/null" {
                status = DiffStatus::Added;
            }
            i += 1;
        }
        if i < lines.len() && lines[i].starts_with("+++ ") {
            let plus_path = &lines[i][4..];
            if plus_path == "/dev/null" {
                status = DiffStatus::Deleted;
            }
            i += 1;
        }

        // Parse hunks
        let mut hunks: Vec<Hunk> = Vec::new();
        while i < lines.len() && !lines[i].starts_with("diff --git ") {
            if lines[i].starts_with("@@") {
                let hunk = parse_hunk_header(lines[i])?;
                let header = lines[i].to_string();
                let mut hunk_lines: Vec<HunkLine> = Vec::new();
                i += 1;

                while i < lines.len()
                    && !lines[i].starts_with("@@")
                    && !lines[i].starts_with("diff --git ")
                {
                    let l = lines[i];
                    if let Some(content) = l.strip_prefix('+') {
                        hunk_lines.push(HunkLine::Added(content.to_string()));
                    } else if let Some(content) = l.strip_prefix('-') {
                        hunk_lines.push(HunkLine::Removed(content.to_string()));
                    } else if let Some(content) = l.strip_prefix(' ') {
                        hunk_lines.push(HunkLine::Context(content.to_string()));
                    } else if l == "\\ No newline at end of file" {
                        // skip
                    } else if l.is_empty() {
                        // empty context line (git sometimes omits the leading space)
                        hunk_lines.push(HunkLine::Context(String::new()));
                    } else {
                        // unknown line, skip
                    }
                    i += 1;
                }

                hunks.push(Hunk {
                    old_start: hunk.0,
                    old_count: hunk.1,
                    new_start: hunk.2,
                    new_count: hunk.3,
                    header,
                    lines: hunk_lines,
                });
            } else {
                i += 1;
            }
        }

        // Use the appropriate path
        let final_path = if status == DiffStatus::Renamed {
            new_path
        } else {
            // For non-rename, prefer b_path, but fall back to a_path for deletions
            if status == DiffStatus::Deleted {
                a_path
            } else {
                b_path
            }
        };

        files.push(FileDiff {
            path: final_path,
            old_path,
            status,
            hunks,
        });
    }

    Ok(files)
}

/// Parse "diff --git a/path b/path" header.
/// Returns (a_path, b_path) with the a/ and b/ prefixes stripped.
fn parse_diff_header(line: &str) -> Result<(String, String), crate::error::GitError> {
    // Format: "diff --git a/<path> b/<path>"
    // Tricky: paths can contain spaces. We rely on "a/" and "b/" prefixes.
    let rest = line
        .strip_prefix("diff --git ")
        .ok_or_else(|| DiffParseSnafu { message: format!("invalid diff header: {line}") }.build())?;

    // Find " b/" to split - search from the right side of a/ prefix
    if let Some(a_rest) = rest.strip_prefix("a/") {
        // Find " b/" separator
        if let Some(sep_pos) = a_rest.find(" b/") {
            let a_path = &a_rest[..sep_pos];
            let b_path = &a_rest[sep_pos + 3..];
            return Ok((a_path.to_string(), b_path.to_string()));
        }
    }

    // Fallback for unusual formats (e.g., no prefix)
    let parts: Vec<&str> = rest.splitn(2, ' ').collect();
    if parts.len() == 2 {
        let a = parts[0].strip_prefix("a/").unwrap_or(parts[0]);
        let b = parts[1].strip_prefix("b/").unwrap_or(parts[1]);
        Ok((a.to_string(), b.to_string()))
    } else {
        Err(DiffParseSnafu {
            message: format!("cannot parse diff header: {line}"),
        }
        .build())
    }
}

/// Parse "@@ -old_start,old_count +new_start,new_count @@" header.
/// Returns (old_start, old_count, new_start, new_count).
fn parse_hunk_header(line: &str) -> Result<(u32, u32, u32, u32), crate::error::GitError> {
    // Format: "@@ -1,5 +1,7 @@ optional header text"
    let at_end = line.find(" @@").ok_or_else(|| {
        DiffParseSnafu {
            message: format!("invalid hunk header: {line}"),
        }
        .build()
    })?;
    let range_part = &line[3..at_end]; // skip "@@ "
    let parts: Vec<&str> = range_part.split(' ').collect();
    if parts.len() < 2 {
        return Err(DiffParseSnafu {
            message: format!("invalid hunk header ranges: {line}"),
        }
        .build());
    }

    let (old_start, old_count) = parse_range(parts[0].trim_start_matches('-'))?;
    let (new_start, new_count) = parse_range(parts[1].trim_start_matches('+'))?;

    Ok((old_start, old_count, new_start, new_count))
}

/// Parse "start,count" or just "start" (count defaults to 1).
fn parse_range(s: &str) -> Result<(u32, u32), crate::error::GitError> {
    if let Some((start_s, count_s)) = s.split_once(',') {
        let start: u32 = start_s.parse().map_err(|_| {
            DiffParseSnafu {
                message: format!("invalid range number: {s}"),
            }
            .build()
        })?;
        let count: u32 = count_s.parse().map_err(|_| {
            DiffParseSnafu {
                message: format!("invalid range number: {s}"),
            }
            .build()
        })?;
        Ok((start, count))
    } else {
        let start: u32 = s.parse().map_err(|_| {
            DiffParseSnafu {
                message: format!("invalid range number: {s}"),
            }
            .build()
        })?;
        Ok((start, 1))
    }
}
