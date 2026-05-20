pub mod clone;
pub mod new_repo;
pub mod switch;

use anyhow::{Result, bail};
use inquire::{InquireError, Select};

use crate::config::{self, Config, Profile};

/// Resolve a profile for the given URL via the user's config. If no rule
/// matches and no `default` is set, prompt the user to pick one from the
/// defined profiles. Bails if no profiles exist.
pub(crate) fn pick_profile(cfg: &Config, url: &str) -> Result<(String, Profile)> {
    if let Some((k, p)) = cfg.resolve(url)? {
        return Ok((k.to_string(), p.clone()));
    }
    let keys = cfg.profile_keys();
    if keys.is_empty() {
        bail!(
            "no profile matched and config has no profiles defined (see {})",
            config::config_path()?.display()
        );
    }
    let prompt = format!("no rule matched for {} — pick a profile", url);
    let chosen = match Select::new(&prompt, keys).prompt() {
        Ok(k) => k,
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
            std::process::exit(130);
        }
        Err(e) => return Err(e.into()),
    };
    let profile = cfg
        .profiles
        .get(&chosen)
        .expect("selected key must exist in profiles")
        .clone();
    Ok((chosen, profile))
}
