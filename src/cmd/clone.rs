use anyhow::{Result, anyhow};
use std::path::PathBuf;

use crate::config::{self, Config};
use crate::git;

pub fn run(url: String, dir: Option<String>) -> Result<()> {
    let cfg = Config::load()?;
    let (profile_key, profile) = super::pick_profile(&cfg, &url)?;

    let dest = match dir {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(derive_dir(&url)?),
    };

    let dest_str = dest
        .to_str()
        .ok_or_else(|| anyhow!("destination path is not valid utf-8: {}", dest.display()))?;

    git::run(["clone", url.as_str(), dest_str])?;
    git::run_in(&dest, ["config", "user.name", profile.name.as_str()])?;
    git::run_in(&dest, ["config", "user.email", profile.email.as_str()])?;

    println!(
        "cloned into {} using profile '{}'",
        dest.display(),
        profile_key
    );
    Ok(())
}

fn derive_dir(url: &str) -> Result<String> {
    let target = config::match_target(url)?;
    let last = target
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("could not derive directory name from URL '{}'", url))?;
    Ok(last.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_from_scp_url() {
        assert_eq!(derive_dir("git@github.com:foo/bar.git").unwrap(), "bar");
    }

    #[test]
    fn derive_from_https_url() {
        assert_eq!(
            derive_dir("https://github.com/foo/bar.git").unwrap(),
            "bar"
        );
    }

    #[test]
    fn derive_without_dot_git() {
        assert_eq!(derive_dir("https://github.com/foo/bar").unwrap(), "bar");
    }
}
