use crate::exit::CANCELLED;
use crate::git;
use crate::repo::{Repo, Worktree};
use anyhow::{Result, bail};
use inquire::{InquireError, MultiSelect, Select};
use std::collections::{HashMap, HashSet};
use std::fmt;

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Local,
    Remote,
    Tag,
}

/// Which worktrees hold a local branch. Both fields can be set at once:
/// `worktree add --force` and `switch --ignore-other-worktrees` put one
/// branch in several trees, so this is a tally, not a choice.
#[derive(Clone, Copy, Default, PartialEq)]
pub struct Held {
    pub root: bool,
    pub linked: usize,
}

pub struct Entry {
    pub name: String,
    pub kind: Kind,
    pub held: Held,
}

impl fmt::Display for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            Kind::Remote => return write!(f, "{}  · remote", self.name),
            Kind::Tag => return write!(f, "{}  · tag", self.name),
            Kind::Local => {}
        }
        write!(f, "{}", self.name)?;
        if self.held.root {
            write!(f, "  · root")?;
        }
        match self.held.linked {
            0 => Ok(()),
            1 => write!(f, "  · wt"),
            n => write!(f, "  · wt ×{n}"),
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
/// `repo` is `None` when worktree mode is off, which makes every branch
/// `Held::No`.
pub fn entries(repo: Option<&Repo>, worktrees: &[Worktree]) -> Result<Vec<Entry>> {
    let main = repo.map(|r| r.main.as_str());
    let mut locals = lines(git::capture(["branch", "--format=%(refname:short)"])?);
    if let Some(main) = main
        && let Some(i) = locals.iter().position(|b| b == main) {
            let m = locals.remove(i);
            locals.insert(0, m);
        }
    let remotes_full = lines(git::capture(["branch", "-r", "--format=%(refname:short)"])?);
    let tags = lines(git::capture(["tag", "--list", "--sort=-version:refname"])?);

    let mut held: HashMap<&str, Held> = HashMap::new();
    for w in worktrees {
        if let (Some(b), Some(r)) = (w.branch.as_deref(), repo) {
            let e = held.entry(b).or_default();
            if r.is_root(&w.path) {
                e.root = true;
            } else {
                e.linked += 1;
            }
        }
    }
    let local_set: HashSet<&str> = locals.iter().map(String::as_str).collect();

    let mut out = Vec::with_capacity(locals.len() + remotes_full.len() + tags.len());
    for name in &locals {
        out.push(Entry {
            name: name.clone(),
            kind: Kind::Local,
            held: held.get(name.as_str()).copied().unwrap_or_default(),
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
            held: Held::default(),
        });
    }

    for tag in tags {
        out.push(Entry {
            name: tag,
            kind: Kind::Tag,
            held: Held::default(),
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
