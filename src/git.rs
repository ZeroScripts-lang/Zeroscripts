//! Git package source (§93) and Git-based publishing (§94).
//!
//! Everything here shells out to the `git` CLI: the spec gives no transport
//! protocol of its own, and `git` is the mechanism §93/§94 name. Arguments are
//! passed directly (never through a shell) and URLs are stopped with `--` so a
//! hostile path cannot become a `git` option.

use std::path::{Path, PathBuf};

use crate::package::{PackageError, PACKAGE_FILE};
use crate::version::Version;

/// A `git:` dependency/source specifier (§93): `git:<url>[#<ref>]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitSource {
    pub url: String,
    /// Branch, tag or commit; `None` = the repository's default branch.
    pub reference: Option<String>,
}

impl GitSource {
    /// Parse a `git:` prefixed specifier. The prefix is mandatory (§93:
    /// "URL repository harus menggunakan prefix `git:`").
    pub fn parse(spec: &str) -> Result<GitSource, String> {
        let rest = spec
            .strip_prefix("git:")
            .ok_or_else(|| format!("git source must start with 'git:' ({})", spec))?;
        let (url, reference) = match rest.find('#') {
            Some(i) => (&rest[..i], Some(rest[i + 1..].to_string())),
            None => (rest, None),
        };
        if url.is_empty() {
            return Err(format!("git source has no repository URL ({})", spec));
        }
        if let Some(r) = &reference {
            if r.is_empty() {
                return Err(format!("git source has an empty reference ({})", spec));
            }
        }
        Ok(GitSource {
            url: url.to_string(),
            reference,
        })
    }

    /// The specifier as written in `project.zson` / the lockfile `source` field.
    pub fn spec(&self) -> String {
        match &self.reference {
            Some(r) => format!("git:{}#{}", self.url, r),
            None => format!("git:{}", self.url),
        }
    }
}

impl std::fmt::Display for GitSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.spec())
    }
}

/// A package read out of a Git repository: its metadata identity plus the
/// commit that the lockfile records (§93, §97).
#[derive(Debug, Clone)]
pub struct GitCheckout {
    pub dir: PathBuf,
    pub version: Version,
    pub commit: String,
}

fn git_output(args: &[&str], cwd: Option<&Path>) -> Result<String, PackageError> {
    let mut cmd = std::process::Command::new("git");
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let out = cmd
        .output()
        .map_err(|e| PackageError::Git(format!("Cannot run git: {}", e)))?;
    if !out.status.success() {
        let detail = String::from_utf8_lossy(&out.stderr);
        let detail = detail.trim();
        let detail = if detail.is_empty() {
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        } else {
            detail.to_string()
        };
        return Err(PackageError::Git(format!("git {} failed: {}", args[0], detail)));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Clone `source` into `dest` (which must not exist yet), check out the
/// requested reference, then read `package.zson` (§94: a repository without
/// valid package metadata cannot be used as a package).
pub fn checkout(source: &GitSource, dest: &Path) -> Result<GitCheckout, PackageError> {
    if dest.exists() {
        std::fs::remove_dir_all(dest)
            .map_err(|e| PackageError::Git(format!("Cannot prepare {}: {}", dest.display(), e)))?;
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| PackageError::Git(format!("Cannot create {}: {}", parent.display(), e)))?;
    }

    let dest_str = dest.display().to_string();
    git_output(
        &["clone", "--quiet", "--", &source.url, &dest_str],
        None,
    )?;

    if let Some(reference) = &source.reference {
        // `--end-of-options` keeps a hostile ref like `-f` from becoming a
        // checkout option; `--` would wrongly treat the ref as a pathspec.
        git_output(
            &["checkout", "--quiet", "--end-of-options", reference],
            Some(dest),
        )?;
    }

    let meta_path = dest.join(PACKAGE_FILE);
    let text = std::fs::read_to_string(&meta_path).map_err(|e| {
        PackageError::Git(format!(
            "Repository '{}' has no readable {}: {}",
            source.url,
            PACKAGE_FILE,
            e
        ))
    })?;
    let meta = crate::package::parse_package_meta(&text, &meta_path.display().to_string())
        .map_err(|e| PackageError::Git(format!("Repository '{}': {}", source.url, e.message())))?;

    let commit = git_output(&["rev-parse", "HEAD"], Some(dest))?;
    let commit = commit.trim().to_string();
    if commit.is_empty() {
        return Err(PackageError::Git(format!(
            "Repository '{}' has no commits",
            source.url
        )));
    }

    Ok(GitCheckout {
        dir: dest.to_path_buf(),
        version: meta.version.parse().map_err(|e| {
            PackageError::Git(format!("Repository '{}': {}", source.url, e))
        })?,
        commit,
    })
}

/// Check out a concrete commit that is already present in `dir` (§93: "Git
/// package dapat menggunakan branch, tag, atau commit tertentu"; §97: the
/// recorded commit is the source identity). Returns the resulting `HEAD`.
pub fn checkout_commit(dir: &Path, commit: &str) -> Result<String, PackageError> {
    git_output(&["checkout", "--quiet", "--", commit], Some(dir))?;
    let head = git_output(&["rev-parse", "HEAD"], Some(dir))?;
    let head = head.trim().to_string();
    if head.is_empty() {
        return Err(PackageError::Git(format!(
            "Cannot read HEAD after checking out {}",
            commit
        )));
    }
    Ok(head)
}

/// `true` when `dir` is inside a Git working tree (§94 tagging).
pub fn is_repository(dir: &Path) -> bool {
    let mut cur = Some(dir);
    while let Some(d) = cur {
        if d.join(".git").exists() {
            return true;
        }
        cur = d.parent();
    }
    false
}

/// Top level of the working tree containing `dir`.
pub fn repository_root(dir: &Path) -> Option<PathBuf> {
    let mut cur = Some(dir);
    while let Some(d) = cur {
        if d.join(".git").exists() {
            return Some(d.to_path_buf());
        }
        cur = d.parent();
    }
    None
}

/// `true` when the repository already has `tag` (§94: a published version may
/// not be changed).
pub fn has_tag(repo: &Path, tag: &str) -> Result<bool, PackageError> {
    let out = git_output(&["tag", "--list", tag], Some(repo))?;
    Ok(out.lines().any(|line| line.trim() == tag))
}

/// Create the version tag `v1.2.0` used to mark a published release (§94).
pub fn create_tag(repo: &Path, tag: &str) -> Result<(), PackageError> {
    git_output(&["tag", tag], Some(repo))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_source_parse_prefix_and_ref() {
        let s = GitSource::parse("git:https://github.com/user/http-utils.git#v1.2.0").unwrap();
        assert_eq!(s.url, "https://github.com/user/http-utils.git");
        assert_eq!(s.reference.as_deref(), Some("v1.2.0"));

        let s = GitSource::parse("git:https://github.com/user/http-utils.git#main").unwrap();
        assert_eq!(s.reference.as_deref(), Some("main"));

        let s = GitSource::parse("git:https://github.com/user/http-utils.git#8f42c1a").unwrap();
        assert_eq!(s.reference.as_deref(), Some("8f42c1a"));

        let s = GitSource::parse("git:/tmp/repo").unwrap();
        assert_eq!(s.url, "/tmp/repo");
        assert_eq!(s.reference, None);
        assert_eq!(s.spec(), "git:/tmp/repo");
    }

    #[test]
    fn git_source_rejects_missing_pieces() {
        for bad in [
            "https://github.com/user/x.git", // missing git: prefix (§93)
            "git:",
            "git:https://example.com/x.git#",
        ] {
            assert!(GitSource::parse(bad).is_err(), "expected '{}' rejected", bad);
        }
    }
}
