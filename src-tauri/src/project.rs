//! Project identification for reply history.
//!
//! The "current project" is derived from the CLI invocation's working directory: walk up from
//! `cwd` to the first ancestor containing a `.git` entry (the repo root); fall back to `cwd` when
//! no repo is found. The canonicalized absolute path is the project key; its basename is the
//! display name. This is computed by the CLI (and standalone GUI processes) and carried through to
//! the recording point so history can be filtered per project.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Detect the current project key (absolute path). Returns an empty string only when the working
/// directory can't be determined.
pub fn detect() -> String {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(_) => return String::new(),
    };
    detect_from(&cwd)
}

/// Project key for an explicit directory (e.g. the cwd a hook reported on stdin): its git root,
/// falling back to the directory itself.
pub fn detect_from(dir: &Path) -> String {
    let root = git_root(dir).unwrap_or_else(|| dir.to_path_buf());
    canonical_string(&root)
}

/// Walk up from `start` to the first ancestor that contains a `.git` entry (file or dir).
pub fn git_root(start: &Path) -> Option<PathBuf> {
    let mut cur: Option<&Path> = Some(start);
    while let Some(dir) = cur {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        cur = dir.parent();
    }
    None
}

/// Canonicalize when possible (resolves symlinks); fall back to the lossy display path.
fn canonical_string(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string()
}

/// Display name for a project key: the final path component; empty key yields an empty string
/// (callers localize an "unknown project" label).
pub fn display_name(key: &str) -> String {
    if key.is_empty() {
        return String::new();
    }
    Path::new(key)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| key.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryIdentity {
    NonRepository,
    Github(String),
    Unavailable,
}

/// Resolve canonical repository identity for a project key.
///
/// GitHub identity comes from the Git root's `origin` URL. A local directory name is never used as
/// a fallback because worktrees and renamed checkouts are not canonical GitHub identity. A Git
/// repository whose canonical remote cannot be resolved is distinct from a non-repository request
/// so channel renderers can fail closed instead of silently omitting required context.
pub fn repository_identity(key: &str) -> RepositoryIdentity {
    let Some(root) = git_root(Path::new(key)) else {
        return RepositoryIdentity::NonRepository;
    };
    let Ok(output) = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["remote", "get-url", "origin"])
        .output()
    else {
        return RepositoryIdentity::Unavailable;
    };
    if !output.status.success() {
        return RepositoryIdentity::Unavailable;
    }
    std::str::from_utf8(&output.stdout)
        .ok()
        .and_then(repository_name_from_github_remote)
        .map(RepositoryIdentity::Github)
        .unwrap_or(RepositoryIdentity::Unavailable)
}

fn repository_name_from_github_remote(remote: &str) -> Option<String> {
    let remote = remote.trim().trim_end_matches('/');
    let path = remote
        .strip_prefix("https://github.com/")
        .or_else(|| remote.strip_prefix("ssh://git@github.com/"))
        .or_else(|| remote.strip_prefix("git@github.com:"))?;
    let mut segments = path.split('/');
    let owner = segments.next()?;
    let repository = segments.next()?;
    let repository = repository.strip_suffix(".git").unwrap_or(repository);
    if owner.is_empty() || repository.is_empty() || segments.next().is_some() {
        return None;
    }
    Some(repository.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::tempdir;

    #[test]
    fn subdir_resolves_to_git_root() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join(".git")).unwrap();
        let sub = root.join("a").join("b");
        fs::create_dir_all(&sub).unwrap();
        let found = git_root(&sub).unwrap();
        // Compare canonicalized to tolerate /private symlink on macOS temp dirs.
        assert_eq!(
            fs::canonicalize(&found).unwrap(),
            fs::canonicalize(root).unwrap()
        );
    }

    #[test]
    fn no_git_returns_none() {
        let dir = tempdir().unwrap();
        let sub = dir.path().join("x");
        fs::create_dir_all(&sub).unwrap();
        assert!(git_root(&sub).is_none());
    }

    #[test]
    fn display_name_is_basename() {
        assert_eq!(display_name("/home/u/my-proj"), "my-proj");
        assert_eq!(display_name(""), "");
    }

    #[test]
    fn github_https_and_ssh_remotes_resolve_to_repository_slug() {
        assert_eq!(
            repository_name_from_github_remote("https://github.com/cigit-zgy/human-in-loop.git"),
            Some("human-in-loop".into())
        );
        assert_eq!(
            repository_name_from_github_remote("git@github.com:cigit-zgy/water-biomodel-agent.git"),
            Some("water-biomodel-agent".into())
        );
    }

    #[test]
    fn canonical_remote_wins_over_local_directory_identity() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("task-worktree-name");
        fs::create_dir_all(&root).unwrap();
        assert!(Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "git@github.com:cigit-zgy/human-in-loop.git",
            ])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());

        assert_eq!(
            repository_identity(root.to_str().unwrap()),
            RepositoryIdentity::Github("human-in-loop".into())
        );
    }

    #[test]
    fn non_repository_has_no_github_repository_label() {
        let dir = tempdir().unwrap();
        assert_eq!(
            repository_identity(dir.path().to_str().unwrap()),
            RepositoryIdentity::NonRepository
        );
    }

    #[test]
    fn repository_without_a_canonical_github_remote_is_unavailable() {
        let dir = tempdir().unwrap();
        assert!(Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(dir.path())
            .status()
            .unwrap()
            .success());
        assert_eq!(
            repository_identity(dir.path().to_str().unwrap()),
            RepositoryIdentity::Unavailable
        );
    }
}
