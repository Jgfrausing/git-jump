use crate::exit::{refused, usage};
use crate::git;
use crate::repo::{self, Repo};
use anyhow::{Result, bail};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

/// `Some(path)` means "the shell should move here".
pub type Moved = Option<PathBuf>;

pub enum Base {
    Main { sync: bool },
    Here,
    Start(String),
}

fn cwd() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// A directory about to be removed must not stay our cwd: the next git call
/// would die with "Unable to read current working directory". Returns true
/// when the cwd was inside `dir`.
fn leave_if_inside(repo: &Repo, dir: &Path) -> bool {
    if repo::is_within(&cwd(), dir) {
        std::env::set_current_dir(&repo.root).ok();
        true
    } else {
        false
    }
}

fn dirty_root(repo: &Repo, on: &str) -> anyhow::Error {
    refused(format!(
        "root {} is on '{on}' with uncommitted changes; commit or stash there first",
        repo.root.display()
    ))
}

/// Puts the root worktree on main: no-op, `switch main` when detached, adopt
/// when it holds another branch. Refuses (exit 4) when the root is dirty.
fn root_to_main(repo: &Repo) -> Result<()> {
    match repo::current_branch(&repo.root)? {
        Some(b) if b == repo.main => Ok(()),
        None => {
            if !repo::is_clean(&repo.root)? {
                return Err(dirty_root(repo, "a detached HEAD"));
            }
            git::run_in(&repo.root, ["switch", &repo.main])
        }
        Some(_) => adopt(repo).map(|_| ()),
    }
}

fn add_worktree(repo: &Repo, path: &Path, extra: &[&str], target: &str) -> Result<()> {
    repo.ensure_exclude()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut args: Vec<&OsStr> = vec![OsStr::new("worktree"), OsStr::new("add")];
    args.extend(extra.iter().map(OsStr::new));
    args.push(path.as_os_str());
    args.push(OsStr::new(target));
    git::run_in(&repo.root, args)
}

pub fn go(repo: &Repo, b: &str) -> Result<Moved> {
    repo.prune()?;
    if b == repo.main {
        root_to_main(repo)?;
        return Ok(Some(repo.root.clone()));
    }
    if let Some(wt) = repo.worktree_of(b)? {
        if repo.is_root(&wt.path) {
            return adopt(repo);
        }
        return Ok(Some(wt.path));
    }
    let path = repo.path_for(b);
    if let Some(wt) = repo.worktree_at(&path)? {
        if !repo::is_clean(&path)? {
            return Err(refused(format!(
                "slug collision: {} holds '{}' with uncommitted changes",
                path.display(),
                wt.branch.as_deref().unwrap_or("a detached HEAD")
            )));
        }
        git::run_in(&path, ["switch", b])?;
        return Ok(Some(path));
    }
    if path.exists() {
        bail!(
            "{} exists but is not a registered worktree; remove it or run: git worktree repair",
            path.display()
        );
    }
    add_worktree(repo, &path, &[], b)?;
    Ok(Some(path))
}

fn main_has_upstream(repo: &Repo) -> bool {
    git::ok_in(
        &repo.root,
        ["rev-parse", "--verify", "--quiet", &format!("{}@{{u}}", repo.main)],
    )
}

/// The fetch-and-ff-pull half of sync. Returns true when local main is fresh.
fn refresh_main(repo: &Repo) -> bool {
    if let Err(e) = git::run_in(&repo.root, ["fetch", "--all", "--prune"]) {
        eprintln!("gj: fetch failed ({e}); using local {} as is", repo.main);
        return true;
    }
    if !main_has_upstream(repo) {
        return true;
    }
    let on_main = repo::current_branch(&repo.root)
        .ok()
        .flatten()
        .is_some_and(|b| b == repo.main);
    if !on_main || !repo::is_clean(&repo.root).unwrap_or(false) {
        eprintln!(
            "gj: root {} is not clean on '{}'; skipping pull",
            repo.root.display(),
            repo.main
        );
        return false;
    }
    match git::run_in(&repo.root, ["pull", "--ff-only"]) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("gj: pull --ff-only failed ({e})");
            false
        }
    }
}

pub fn new(repo: &Repo, name: &str, base: Base) -> Result<Moved> {
    if !git::ok(["check-ref-format", "--branch", name]) {
        return Err(usage(format!("'{name}' is not a valid branch name")));
    }
    if repo::branch_exists(name) {
        bail!("branch '{name}' exists; use: gj co {name}");
    }
    let path = repo.path_for(name);
    if path.exists() || repo.worktree_at(&path)?.is_some() {
        bail!("{} already exists", path.display());
    }
    let (start, extra): (String, &[&str]) = match base {
        Base::Start(s) => (s, &[]),
        Base::Here => (
            git::capture_in(&cwd(), ["rev-parse", "HEAD"])?.trim().to_string(),
            &[],
        ),
        Base::Main { sync } => {
            root_to_main(repo)?;
            let fresh = !sync || refresh_main(repo);
            let remote = format!("origin/{}", repo.main);
            if !fresh && repo::is_rev(&remote) {
                eprintln!("gj: basing '{name}' on {remote} instead");
                (remote, &["--no-track"])
            } else {
                (repo.main.clone(), &[])
            }
        }
    };
    let mut flags: Vec<&str> = extra.to_vec();
    flags.extend(["-b", name]);
    add_worktree(repo, &path, &flags, &start)?;
    Ok(Some(path))
}

pub fn rm(repo: &Repo, names: &[String], force: bool) -> Result<Moved> {
    let mut moved = false;
    let flag = if force { "-D" } else { "-d" };
    for b in names {
        if *b == repo.main {
            bail!("refusing to delete the main branch '{b}'");
        }
        if !repo::branch_exists(b) {
            bail!("no such branch: {b}");
        }
        match repo.worktree_of(b)? {
            None => git::run_in(&repo.root, ["branch", flag, b])?,
            Some(wt) if repo.is_root(&wt.path) => {
                if !repo::is_clean(&repo.root)? {
                    return Err(dirty_root(repo, b));
                }
                git::run_in(&repo.root, ["switch", &repo.main])?;
                git::run_in(&repo.root, ["branch", flag, b])?;
            }
            Some(wt) => {
                moved |= leave_if_inside(repo, &wt.path);
                if force {
                    git::run_in(
                        &repo.root,
                        [
                            OsStr::new("worktree"),
                            OsStr::new("remove"),
                            OsStr::new("--force"),
                            wt.path.as_os_str(),
                        ],
                    )?;
                    git::run_in(&repo.root, ["branch", "-D", b])?;
                } else {
                    if !repo::is_clean(&wt.path)? {
                        return Err(refused(format!(
                            "worktree {} has uncommitted or untracked changes (use -D)",
                            wt.path.display()
                        )));
                    }
                    // Detach so `branch -d` can run its merged check while the
                    // tree still exists; restore on refusal.
                    git::run_in(&wt.path, ["switch", "--detach"])?;
                    if let Err(e) = git::run_in(&repo.root, ["branch", "-d", b]) {
                        git::run_in(&wt.path, ["switch", b]).ok();
                        return Err(e);
                    }
                    git::run_in(
                        &repo.root,
                        [
                            OsStr::new("worktree"),
                            OsStr::new("remove"),
                            wt.path.as_os_str(),
                        ],
                    )?;
                }
            }
        }
    }
    Ok(moved.then(|| repo.root.clone()))
}

fn gone_branches(repo: &Repo) -> Result<Vec<String>> {
    let out = git::capture_in(
        &repo.root,
        [
            "for-each-ref",
            "--format=%(refname:short) %(upstream:track)",
            "refs/heads",
        ],
    )?;
    Ok(out
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let name = it.next()?;
            (it.next() == Some("[gone]")).then(|| name.to_string())
        })
        .collect())
}

pub fn sync(repo: &Repo) -> Result<Moved> {
    git::run_in(&repo.root, ["fetch", "--all", "--prune"])?;
    if main_has_upstream(repo) {
        let on_main = repo::current_branch(&repo.root)?.is_some_and(|b| b == repo.main);
        if on_main && repo::is_clean(&repo.root)? {
            git::run_in(&repo.root, ["pull", "--ff-only"])?;
        } else {
            eprintln!(
                "gj: root {} is not clean on '{}'; skipping pull",
                repo.root.display(),
                repo.main
            );
        }
    }
    let mut moved = false;
    let mut first_err = None;
    for b in gone_branches(repo)? {
        if b == repo.main {
            eprintln!("gj: '{b}' is gone upstream but is the main branch; leaving it");
            continue;
        }
        eprintln!("gj: removing gone branch '{b}'");
        match rm(repo, std::slice::from_ref(&b), true) {
            Ok(m) => moved |= m.is_some(),
            Err(e) => {
                eprintln!("gj: could not remove '{b}': {e:#}");
                first_err.get_or_insert(e);
            }
        }
    }
    repo.prune()?;
    if let Some(e) = first_err {
        return Err(e);
    }
    Ok(moved.then(|| repo.root.clone()))
}

pub fn up(repo: &Repo) -> Result<Moved> {
    let here = cwd();
    if repo::current_branch(&here)?.is_some_and(|b| b == repo.main) {
        bail!(
            "already on '{}'; gj up merges it into a branch worktree",
            repo.main
        );
    }
    let moved = sync(repo)?;
    if moved.is_some() {
        eprintln!("gj: the current worktree was removed by sync; nothing to merge into");
        return Ok(moved);
    }
    git::run_tty_in(&here, ["merge", &repo.main])?;
    Ok(None)
}

pub fn adopt(repo: &Repo) -> Result<Moved> {
    let Some(b) = repo::current_branch(&repo.root)? else {
        if !repo::is_clean(&repo.root)? {
            return Err(dirty_root(repo, "a detached HEAD"));
        }
        git::run_in(&repo.root, ["switch", &repo.main])?;
        return Ok(Some(repo.root.clone()));
    };
    if b == repo.main {
        return Ok(Some(repo.root.clone()));
    }
    if !repo::is_clean(&repo.root)? {
        return Err(dirty_root(repo, &b));
    }
    repo.prune()?;
    let path = repo.path_for(&b);
    if path.exists() || repo.worktree_at(&path)?.is_some() {
        bail!(
            "{} already exists; remove it before adopting '{b}'",
            path.display()
        );
    }
    git::run_in(&repo.root, ["switch", "--detach"])?;
    if let Err(e) = add_worktree(repo, &path, &[], &b) {
        git::run_in(&repo.root, ["switch", &b]).ok();
        return Err(e);
    }
    if let Err(e) = git::run_in(&repo.root, ["switch", &repo.main]) {
        eprintln!(
            "gj: '{b}' now lives in {}, but the root could not switch to '{}'; run: git switch {}",
            path.display(),
            repo.main,
            repo.main
        );
        return Err(e);
    }
    Ok(Some(path))
}

/// One `path<TAB>branch` line per worktree, root first. For editors and
/// scripts; the human form is `list`.
pub fn list_tsv(repo: &Repo) -> Result<()> {
    for wt in repo.worktrees()? {
        println!(
            "{}\t{}",
            wt.path.display(),
            wt.branch.as_deref().unwrap_or("(detached)")
        );
    }
    Ok(())
}

pub fn list(repo: &Repo) -> Result<()> {
    for wt in repo.worktrees()? {
        let what = match &wt.branch {
            Some(b) if repo.is_root(&wt.path) => format!("{b}  · root"),
            Some(b) => b.clone(),
            None => "(detached)".to_string(),
        };
        let mark = if wt.prunable { "  · prunable" } else { "" };
        println!("{}  {what}{mark}", wt.path.display());
    }
    Ok(())
}

/// Branches held by linked worktrees, for `gj wt rm` without arguments.
pub fn linked_branches(repo: &Repo) -> Result<Vec<String>> {
    Ok(repo
        .worktrees()?
        .into_iter()
        .filter(|w| !repo.is_root(&w.path))
        .filter_map(|w| w.branch)
        .collect())
}
