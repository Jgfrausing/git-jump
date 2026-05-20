use anyhow::{Result, bail};
use inquire::{InquireError, Select};
use std::collections::HashSet;
use std::fmt;

use crate::git;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Local,
    Remote,
    Tag,
}

struct Entry {
    name: String,
    kind: Kind,
}

impl fmt::Display for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            Kind::Local => write!(f, "{}", self.name),
            Kind::Remote => write!(f, "{}  · remote", self.name),
            Kind::Tag => write!(f, "{}  · tag", self.name),
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

    let tags: Vec<String> = git::capture(["tag", "--list", "--sort=-version:refname"])?
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    let local_set: HashSet<&str> = locals.iter().map(String::as_str).collect();

    let mut entries: Vec<Entry> =
        Vec::with_capacity(locals.len() + remotes_full.len() + tags.len());

    for name in &locals {
        entries.push(Entry {
            name: name.clone(),
            kind: Kind::Local,
        });
    }

    let mut seen_remote: HashSet<String> = HashSet::new();
    for full in &remotes_full {
        // A real remote-tracking branch is `<remote>/<branch>`. `git branch -r
        // --format=%(refname:short)` will sometimes also emit a bare remote
        // name (e.g. `origin`) for `refs/remotes/<remote>/HEAD` depending on
        // git's symref-shortening behavior — skip those, they aren't checkout
        // targets.
        let short = match full.split_once('/') {
            Some((_, s)) if !s.is_empty() && s != "HEAD" => s,
            _ => continue,
        };
        if local_set.contains(short) {
            continue;
        }
        if !seen_remote.insert(short.to_string()) {
            continue;
        }
        entries.push(Entry {
            name: short.to_string(),
            kind: Kind::Remote,
        });
    }

    for tag in tags {
        entries.push(Entry {
            name: tag,
            kind: Kind::Tag,
        });
    }

    if entries.is_empty() {
        bail!("no branches or tags to choose from");
    }

    let chosen = match Select::new("checkout", entries).with_page_size(15).prompt() {
        Ok(e) => e,
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
            std::process::exit(130);
        }
        Err(e) => return Err(e.into()),
    };

    match chosen.kind {
        // `git switch` handles both local branches and DWIM-tracks remote-only.
        Kind::Local | Kind::Remote => git::run(["switch", &chosen.name]),
        // Tags can't be a branch tip — land detached.
        Kind::Tag => git::run(["checkout", &chosen.name]),
    }
}
