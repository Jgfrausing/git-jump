use anyhow::{Context, Result, anyhow};
use globset::{Glob, GlobSet, GlobSetBuilder};
use indexmap::IndexMap;
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use url::Url;

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub profiles: IndexMap<String, Profile>,
    #[serde(default)]
    pub default: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Profile {
    pub name: String,
    pub email: String,
    #[serde(default)]
    pub patterns: Vec<String>,
    /// SSH host alias from `~/.ssh/config`. When set, git-jump rewrites the
    /// host of SSH-form URLs (e.g. `github.com` → `github-personal`) so that
    /// the right `IdentityFile` is selected by openssh. HTTPS URLs are
    /// untouched.
    #[serde(default)]
    pub host_alias: Option<String>,
}

pub fn config_path() -> Result<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => dirs::home_dir()
            .ok_or_else(|| anyhow!("could not locate $HOME"))?
            .join(".config"),
    };
    Ok(base.join("git-jump").join("config.toml"))
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = config_path()?;
        if !path.exists() {
            return Ok(Self {
                profiles: IndexMap::new(),
                default: None,
            });
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let cfg: Config = toml::from_str(&raw)
            .with_context(|| format!("parsing {}", path.display()))?;
        if let Some(d) = &cfg.default {
            if !cfg.profiles.contains_key(d) {
                return Err(anyhow!(
                    "default profile '{}' is not defined in [profiles]",
                    d
                ));
            }
        }
        Ok(cfg)
    }

    pub fn resolve(&self, url: &str) -> Result<Option<(&str, &Profile)>> {
        let target = match_target(url)?;
        for (key, profile) in &self.profiles {
            let set = build_globs(&profile.patterns)
                .with_context(|| format!("compiling patterns for profile '{}'", key))?;
            if set.is_match(&target) {
                return Ok(Some((key.as_str(), profile)));
            }
        }
        if let Some(d) = &self.default {
            if let Some(p) = self.profiles.get(d) {
                return Ok(Some((d.as_str(), p)));
            }
        }
        Ok(None)
    }

    pub fn profile_keys(&self) -> Vec<String> {
        self.profiles.keys().cloned().collect()
    }
}

/// Rewrite the host of an SSH-form URL. HTTPS and other non-SSH URLs are
/// returned unchanged so that profile rules can include HTTPS clones without
/// silently rerouting them through an ssh host alias.
pub fn rewrite_ssh_host(url: &str, new_host: &str) -> String {
    if let Some(rest) = url.strip_prefix("ssh://") {
        let slash = match rest.find('/') {
            Some(i) => i,
            None => return url.to_string(),
        };
        let (auth, tail) = rest.split_at(slash);
        let user_prefix = match auth.rfind('@') {
            Some(i) => &auth[..=i],
            None => "",
        };
        return format!("ssh://{}{}{}", user_prefix, new_host, tail);
    }
    if !url.contains("://") {
        if let Some(at) = url.find('@') {
            let user = &url[..at];
            let rest = &url[at + 1..];
            if let Some(colon) = rest.find(':') {
                let path = &rest[colon + 1..];
                return format!("{}@{}:{}", user, new_host, path);
            }
        }
    }
    url.to_string()
}

fn build_globs(patterns: &[String]) -> Result<GlobSet> {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        b.add(Glob::new(p).with_context(|| format!("invalid glob '{}'", p))?);
    }
    Ok(b.build()?)
}

/// Build the string that patterns are matched against:
/// `<host>/<path-without-leading-slash-or-.git-suffix>`.
pub fn match_target(input: &str) -> Result<String> {
    let normalized = normalize_url(input);
    let parsed = Url::parse(&normalized)
        .with_context(|| format!("could not parse URL '{}'", input))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow!("URL '{}' has no host", input))?;
    let path = parsed.path().trim_start_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    Ok(format!("{}/{}", host, path))
}

/// Convert `git@host:path` (scp-like) to `ssh://git@host/path` so the `url` crate
/// can parse it. Leaves other forms untouched.
fn normalize_url(input: &str) -> String {
    if input.contains("://") {
        return input.to_string();
    }
    if let Some(at) = input.find('@') {
        let after_at = &input[at + 1..];
        if let Some(colon) = after_at.find(':') {
            let host = &after_at[..colon];
            let path = &after_at[colon + 1..];
            let user = &input[..at];
            return format!("ssh://{}@{}/{}", user, host, path);
        }
    }
    input.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_scp_form() {
        assert_eq!(
            normalize_url("git@github.com:foo/bar.git"),
            "ssh://git@github.com/foo/bar.git"
        );
    }

    #[test]
    fn normalize_https_passthrough() {
        assert_eq!(
            normalize_url("https://github.com/foo/bar.git"),
            "https://github.com/foo/bar.git"
        );
    }

    #[test]
    fn target_strips_dot_git() {
        assert_eq!(
            match_target("git@github.com:foo/bar.git").unwrap(),
            "github.com/foo/bar"
        );
        assert_eq!(
            match_target("https://github.com/foo/bar").unwrap(),
            "github.com/foo/bar"
        );
    }

    #[test]
    fn resolve_first_match_wins() {
        let raw = r#"
[profiles.work]
name = "W"
email = "w@x"
patterns = ["github.com/mft-energy/*"]

[profiles.personal]
name = "P"
email = "p@x"
patterns = ["github.com/*"]
"#;
        let cfg: Config = toml::from_str(raw).unwrap();
        let (key, _) = cfg
            .resolve("git@github.com:mft-energy/repo.git")
            .unwrap()
            .unwrap();
        assert_eq!(key, "work");
        let (key, _) = cfg
            .resolve("git@github.com:someone/other.git")
            .unwrap()
            .unwrap();
        assert_eq!(key, "personal");
    }

    #[test]
    fn resolve_miss_returns_none_without_default() {
        let raw = r#"
[profiles.work]
name = "W"
email = "w@x"
patterns = ["github.com/mft-energy/*"]
"#;
        let cfg: Config = toml::from_str(raw).unwrap();
        assert!(
            cfg.resolve("git@gitlab.com:foo/bar.git")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn rewrite_scp_form() {
        assert_eq!(
            rewrite_ssh_host("git@github.com:foo/bar.git", "github-personal"),
            "git@github-personal:foo/bar.git"
        );
    }

    #[test]
    fn rewrite_ssh_scheme() {
        assert_eq!(
            rewrite_ssh_host("ssh://git@github.com/foo/bar.git", "github-personal"),
            "ssh://git@github-personal/foo/bar.git"
        );
    }

    #[test]
    fn rewrite_leaves_https_alone() {
        assert_eq!(
            rewrite_ssh_host("https://github.com/foo/bar.git", "github-personal"),
            "https://github.com/foo/bar.git"
        );
    }

    #[test]
    fn resolve_falls_back_to_default() {
        let raw = r#"
default = "personal"

[profiles.work]
name = "W"
email = "w@x"
patterns = ["github.com/mft-energy/*"]

[profiles.personal]
name = "P"
email = "p@x"
patterns = []
"#;
        let cfg: Config = toml::from_str(raw).unwrap();
        let (key, _) = cfg
            .resolve("git@gitlab.com:foo/bar.git")
            .unwrap()
            .unwrap();
        assert_eq!(key, "personal");
    }
}
