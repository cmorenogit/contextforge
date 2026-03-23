use std::path::Path;

use git2::Repository;
use regex::Regex;

use crate::error::{ContextForgeError, Result};

/// Parsed commit information.
#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub hash: String,
    pub message: String,
    pub author: String,
    pub committed_at: String,
    pub conventional: Option<ConventionalCommit>,
}

/// Conventional commit components.
#[derive(Debug, Clone)]
pub struct ConventionalCommit {
    pub commit_type: String,
    pub scope: Option<String>,
    pub breaking: bool,
    pub description: String,
}

/// Reads git history from a repository using libgit2.
pub struct GitAnalyzer {
    repo: Repository,
}

impl GitAnalyzer {
    /// Open a git repository at the given path (discovers .git upward).
    pub fn open(path: &Path) -> Result<Self> {
        let repo = Repository::discover(path)
            .map_err(|e| ContextForgeError::Git(format!("Failed to discover repo: {e}")))?;
        Ok(Self { repo })
    }

    /// Walk commits from HEAD, up to `limit`.
    pub fn walk_commits(&self, limit: usize) -> Result<Vec<CommitInfo>> {
        let head = self
            .repo
            .head()
            .map_err(|e| ContextForgeError::Git(format!("Failed to get HEAD: {e}")))?;

        let head_oid = head
            .target()
            .ok_or_else(|| ContextForgeError::Git("HEAD has no target".into()))?;

        let mut revwalk = self
            .repo
            .revwalk()
            .map_err(|e| ContextForgeError::Git(format!("Revwalk init: {e}")))?;

        revwalk
            .push(head_oid)
            .map_err(|e| ContextForgeError::Git(format!("Revwalk push: {e}")))?;

        let cc_regex = Regex::new(
            r"^(feat|fix|refactor|docs|test|chore|perf|ci|build|style)(\(([^)]+)\))?(!)?\s*:\s*(.+)",
        )
        .expect("valid regex");

        let mut commits = Vec::new();
        for oid_result in revwalk {
            if commits.len() >= limit {
                break;
            }

            let oid =
                oid_result.map_err(|e| ContextForgeError::Git(format!("Revwalk error: {e}")))?;

            let commit = self
                .repo
                .find_commit(oid)
                .map_err(|e| ContextForgeError::Git(format!("Find commit: {e}")))?;

            let message = commit
                .message()
                .unwrap_or("")
                .lines()
                .next()
                .unwrap_or("")
                .to_string();

            let author = commit.author().name().unwrap_or("").to_string();

            let time = commit.time();
            let committed_at = chrono::DateTime::from_timestamp(time.seconds(), 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default();

            let conventional = cc_regex.captures(&message).map(|caps| ConventionalCommit {
                commit_type: caps
                    .get(1)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default(),
                scope: caps.get(3).map(|m| m.as_str().to_string()),
                breaking: caps.get(4).is_some(),
                description: caps
                    .get(5)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default(),
            });

            commits.push(CommitInfo {
                hash: oid.to_string(),
                message,
                author,
                committed_at,
                conventional,
            });
        }

        Ok(commits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conventional_commit_regex() {
        let re = Regex::new(
            r"^(feat|fix|refactor|docs|test|chore|perf|ci|build|style)(\(([^)]+)\))?(!)?\s*:\s*(.+)",
        )
        .unwrap();

        let caps = re.captures("feat(auth): add login").unwrap();
        assert_eq!(&caps[1], "feat");
        assert_eq!(&caps[3], "auth");
        assert!(caps.get(4).is_none());
        assert_eq!(&caps[5], "add login");

        let caps = re.captures("fix: typo").unwrap();
        assert_eq!(&caps[1], "fix");
        assert!(caps.get(3).is_none());
        assert_eq!(&caps[5], "typo");

        let caps = re.captures("refactor!: breaking change").unwrap();
        assert_eq!(&caps[1], "refactor");
        assert!(caps.get(4).is_some());
        assert_eq!(&caps[5], "breaking change");

        assert!(re.captures("not a conventional commit").is_none());
    }

    #[test]
    fn test_open_contextforge_repo() {
        let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let analyzer = GitAnalyzer::open(&repo_root).unwrap();
        let commits = analyzer.walk_commits(5).unwrap();
        assert!(!commits.is_empty());
        assert!(!commits[0].hash.is_empty());
        assert!(!commits[0].message.is_empty());
    }
}
