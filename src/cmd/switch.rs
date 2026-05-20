use anyhow::{Result, bail};
use inquire::{InquireError, Select};
use std::collections::HashSet;
use std::fmt;

use crate::git;

struct Entry {
    name: String,
    remote_only: bool,
}

impl fmt::Display for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.remote_only {
            write!(f, "{}  · remote", self.name)
        } else {
            write!(f, "{}", self.name)
        }
    }
}

pub fn run() -> Result<()> {
    if !git::in_repo() {
        bail!("not inside a git repository");
    }

    let locals: Vec<String> = git::capture(["branch", "--format=%(refname:short)"])?
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    let remotes_full: Vec<String> = git::capture(["branch", "-r", "--format=%(refname:short)"])?
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.ends_with("/HEAD"))
        .map(String::from)
        .collect();

    let local_set: HashSet<&str> = locals.iter().map(String::as_str).collect();

    let mut entries: Vec<Entry> = Vec::with_capacity(locals.len() + remotes_full.len());
    for name in &locals {
        entries.push(Entry {
            name: name.clone(),
            remote_only: false,
        });
    }

    let mut seen_remote: HashSet<String> = HashSet::new();
    for full in &remotes_full {
        let short = full.split_once('/').map(|(_, s)| s).unwrap_or(full.as_str());
        if local_set.contains(short) {
            continue;
        }
        if !seen_remote.insert(short.to_string()) {
            continue;
        }
        entries.push(Entry {
            name: short.to_string(),
            remote_only: true,
        });
    }

    if entries.is_empty() {
        bail!("no branches to choose from");
    }

    let chosen = match Select::new("checkout", entries).with_page_size(15).prompt() {
        Ok(e) => e,
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
            std::process::exit(130);
        }
        Err(e) => return Err(e.into()),
    };

    git::run(["switch", &chosen.name])
}
