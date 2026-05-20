use anyhow::{Context, Result, anyhow, bail};
use inquire::{InquireError, Text};
use std::path::{Path, PathBuf};
use url::Url;

use crate::config::Config;
use crate::git;

pub fn run(base: Option<String>) -> Result<()> {
    let base = match base {
        Some(b) => b,
        None => prompt_for_base()?,
    };

    let (host, namespace) = parse_namespace(&base)?;
    let root = project_root()?;
    let repo_name = root
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("could not determine repo name from {}", root.display()))?
        .to_string();

    let ssh_url = format!("git@{}:{}/{}.git", host, namespace, repo_name);

    let cfg = Config::load()?;
    let (profile_key, profile) = super::pick_profile(&cfg, &ssh_url)?;

    if !is_repo_at(&root) {
        git::run_in(&root, ["init"])?;
    }

    git::run_in(&root, ["config", "user.name", profile.name.as_str()])?;
    git::run_in(&root, ["config", "user.email", profile.email.as_str()])?;

    if has_origin(&root)? {
        bail!("remote 'origin' already exists in {} — remove it first", root.display());
    }
    git::run_in(&root, ["remote", "add", "origin", ssh_url.as_str()])?;

    if !has_head(&root) {
        git::run_in(&root, ["add", "-A"])?;
        git::run_in(&root, ["commit", "-m", "Initial commit"])?;
    }

    git::run_in(&root, ["push", "-u", "origin", "HEAD"])?;

    println!(
        "pushed {} to {} using profile '{}'",
        repo_name, ssh_url, profile_key
    );
    Ok(())
}

fn prompt_for_base() -> Result<String> {
    match Text::new("namespace url:")
        .with_placeholder("github.com/yourname")
        .prompt()
    {
        Ok(s) => Ok(s),
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
            std::process::exit(130);
        }
        Err(e) => Err(e.into()),
    }
}

fn project_root() -> Result<PathBuf> {
    if git::in_repo() {
        let out = git::capture(["rev-parse", "--show-toplevel"])?;
        Ok(PathBuf::from(out.trim()))
    } else {
        Ok(std::env::current_dir()?)
    }
}

fn is_repo_at(path: &Path) -> bool {
    git::silent_in(path, ["rev-parse", "--git-dir"])
}

fn has_origin(cwd: &Path) -> Result<bool> {
    let out = git::capture_in(cwd, ["remote"])?;
    Ok(out.lines().any(|line| line.trim() == "origin"))
}

fn has_head(cwd: &Path) -> bool {
    git::silent_in(cwd, ["rev-parse", "--verify", "HEAD"])
}

/// Accepts `github.com/foo`, `https://github.com/foo`, `git@github.com:foo`,
/// and `ssh://git@github.com/foo`. Returns `(host, namespace)`.
pub fn parse_namespace(input: &str) -> Result<(String, String)> {
    let trimmed = input.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        bail!("namespace url is empty");
    }

    let normalized = if trimmed.contains("://") {
        trimmed.to_string()
    } else if trimmed.starts_with("git@") {
        let after_at = &trimmed[4..];
        let (host, path) = after_at
            .split_once(':')
            .ok_or_else(|| anyhow!("invalid scp-form url: '{}'", input))?;
        format!("ssh://git@{}/{}", host, path)
    } else {
        format!("https://{}", trimmed)
    };

    let parsed = Url::parse(&normalized)
        .with_context(|| format!("could not parse namespace url '{}'", input))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow!("namespace url has no host: '{}'", input))?
        .to_string();
    let namespace = parsed.path().trim_start_matches('/').trim_end_matches('/');
    if namespace.is_empty() {
        bail!("namespace url has no namespace (e.g. user/org): '{}'", input);
    }
    Ok((host, namespace.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bare_host_namespace() {
        let (h, n) = parse_namespace("github.com/jgfrausing").unwrap();
        assert_eq!(h, "github.com");
        assert_eq!(n, "jgfrausing");
    }

    #[test]
    fn parse_https_form() {
        let (h, n) = parse_namespace("https://github.com/jgfrausing").unwrap();
        assert_eq!(h, "github.com");
        assert_eq!(n, "jgfrausing");
    }

    #[test]
    fn parse_scp_form() {
        let (h, n) = parse_namespace("git@github.com:jgfrausing").unwrap();
        assert_eq!(h, "github.com");
        assert_eq!(n, "jgfrausing");
    }

    #[test]
    fn parse_strips_trailing_slash() {
        let (h, n) = parse_namespace("github.com/jgfrausing/").unwrap();
        assert_eq!(h, "github.com");
        assert_eq!(n, "jgfrausing");
    }

    #[test]
    fn parse_nested_namespace() {
        let (h, n) = parse_namespace("dev.azure.com/mft-energy/some-project").unwrap();
        assert_eq!(h, "dev.azure.com");
        assert_eq!(n, "mft-energy/some-project");
    }

    #[test]
    fn parse_empty_rejected() {
        assert!(parse_namespace("").is_err());
        assert!(parse_namespace("   ").is_err());
    }

    #[test]
    fn parse_missing_namespace_rejected() {
        assert!(parse_namespace("github.com").is_err());
        assert!(parse_namespace("https://github.com").is_err());
    }
}
