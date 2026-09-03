use crate::exit::CANCELLED;
use crate::git;
use crate::repo::Worktree;
use anyhow::{Result, bail};
use inquire::{InquireError, MultiSelect, Select};
use std::collections::HashSet;
use std::fmt;

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Local,
    Remote,
    Tag,
}

pub struct Entry {
    pub name: String,
    pub kind: Kind,
    pub has_worktree: bool,
}

impl fmt::Display for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            Kind::Local if self.has_worktree => write!(f, "{}  · wt", self.name),
            Kind::Local => write!(f, "{}", self.name),
            Kind::Remote => write!(f, "{}  · remote", self.name),
            Kind::Tag => write!(f, "{}  · tag", self.name),
        }
    }
}

fn lines(out: String) -> Vec<String> {
    out.lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

/// Local branches (main first when known), remote-only branches, tags.
pub fn entries(main: Option<&str>, worktrees: &[Worktree]) -> Result<Vec<Entry>> {
    let mut locals = lines(git::capture(["branch", "--format=%(refname:short)"])?);
    if let Some(main) = main
        && let Some(i) = locals.iter().position(|b| b == main) {
            let m = locals.remove(i);
            locals.insert(0, m);
        }
    let remotes_full = lines(git::capture(["branch", "-r", "--format=%(refname:short)"])?);
    let tags = lines(git::capture(["tag", "--list", "--sort=-version:refname"])?);

    let held: HashSet<&str> = worktrees
        .iter()
        .filter_map(|w| w.branch.as_deref())
        .filter(|b| Some(*b) != main)
        .collect();
    let local_set: HashSet<&str> = locals.iter().map(String::as_str).collect();

    let mut out = Vec::with_capacity(locals.len() + remotes_full.len() + tags.len());
    for name in &locals {
        out.push(Entry {
            name: name.clone(),
            kind: Kind::Local,
            has_worktree: held.contains(name.as_str()),
        });
    }

    let mut seen_remote: HashSet<String> = HashSet::new();
    for full in &remotes_full {
        // A real remote-tracking branch is `<remote>/<branch>`. git can
        // shorten `refs/remotes/<remote>/HEAD` to a bare remote name like
        // `origin`; that has no `/` and is skipped along with `*/HEAD`.
        let short = match full.split_once('/') {
            Some((_, s)) if !s.is_empty() && s != "HEAD" => s,
            _ => continue,
        };
        if local_set.contains(short) || !seen_remote.insert(short.to_string()) {
            continue;
        }
        out.push(Entry {
            name: short.to_string(),
            kind: Kind::Remote,
            has_worktree: false,
        });
    }

    for tag in tags {
        out.push(Entry {
            name: tag,
            kind: Kind::Tag,
            has_worktree: false,
        });
    }

    if out.is_empty() {
        bail!("no branches or tags to choose from");
    }
    Ok(out)
}

fn cancelled<T>(r: Result<T, InquireError>) -> Result<T> {
    match r {
        Ok(v) => Ok(v),
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
            std::process::exit(CANCELLED)
        }
        Err(e) => Err(e.into()),
    }
}

pub fn select(entries: Vec<Entry>, filter: Option<&str>) -> Result<Entry> {
    let mut p = Select::new("checkout", entries).with_page_size(15);
    if let Some(f) = filter {
        p = p.with_starting_filter_input(f);
    }
    cancelled(p.prompt())
}

pub fn multi_select(prompt: &str, items: Vec<String>) -> Result<Vec<String>> {
    if items.is_empty() {
        bail!("nothing to choose from");
    }
    cancelled(MultiSelect::new(prompt, items).with_page_size(15).prompt())
}
