use crate::git;
use anyhow::{Context, Result, bail};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const WORKTREES_DIR: &str = ".worktrees";

#[derive(Debug, Clone)]
pub struct Worktree {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub prunable: bool,
}

#[derive(Debug)]
pub struct Repo {
    pub root: PathBuf,
    pub common_dir: PathBuf,
    pub main: String,
}

/// False in bare repos and where `git config gj.worktrees` is `false`.
/// Disabled means every intercepted command goes to git untouched.
pub fn enabled() -> bool {
    if git::capture(["config", "--type=bool", "core.bare"])
        .map(|s| s.trim() == "true")
        .unwrap_or(false)
    {
        return false;
    }
    !git::capture(["config", "--type=bool", "gj.worktrees"])
        .map(|s| s.trim() == "false")
        .unwrap_or(false)
}

pub fn slug(branch: &str) -> String {
    branch.replace('/', "-")
}

pub fn is_clean(dir: &Path) -> Result<bool> {
    Ok(git::capture_in(dir, ["status", "--porcelain"])?.trim().is_empty())
}

/// `Some(branch)` or `None` when HEAD is detached.
pub fn current_branch(dir: &Path) -> Result<Option<String>> {
    let out = git::capture_in(dir, ["symbolic-ref", "-q", "--short", "HEAD"]);
    match out {
        Ok(s) => Ok(Some(s.trim().to_string())),
        Err(_) => Ok(None),
    }
}

pub fn is_rev(x: &str) -> bool {
    git::ok(["rev-parse", "--verify", "--quiet", &format!("{x}^{{commit}}")])
}

pub fn branch_exists(b: &str) -> bool {
    git::ok(["show-ref", "--verify", "--quiet", &format!("refs/heads/{b}")])
}

pub fn remote_branch_exists(b: &str) -> bool {
    let Ok(remotes) = git::capture(["remote"]) else {
        return false;
    };
    remotes.lines().map(str::trim).filter(|r| !r.is_empty()).any(|r| {
        git::ok([
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/remotes/{r}/{b}"),
        ])
    })
}

pub fn is_branch(b: &str) -> bool {
    branch_exists(b) || remote_branch_exists(b)
}

pub fn parse_worktrees(porcelain: &str) -> Vec<Worktree> {
    let mut out = Vec::new();
    let mut cur: Option<Worktree> = None;
    for line in porcelain.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            if let Some(wt) = cur.take() {
                out.push(wt);
            }
            cur = Some(Worktree {
                path: PathBuf::from(p),
                branch: None,
                prunable: false,
            });
        } else if let Some(wt) = cur.as_mut() {
            if let Some(b) = line.strip_prefix("branch ") {
                wt.branch = Some(b.strip_prefix("refs/heads/").unwrap_or(b).to_string());
            } else if line.starts_with("prunable") {
                wt.prunable = true;
            }
        }
    }
    if let Some(wt) = cur.take() {
        out.push(wt);
    }
    out
}

fn main_branch() -> Result<String> {
    if let Ok(m) = git::capture(["config", "gj.main"]) {
        let m = m.trim();
        if !m.is_empty() {
            return Ok(m.to_string());
        }
    }
    if let Ok(head) = git::capture(["symbolic-ref", "-q", "refs/remotes/origin/HEAD"])
        && let Some(b) = head.trim().strip_prefix("refs/remotes/origin/") {
            return Ok(b.to_string());
        }
    for cand in ["main", "master"] {
        if branch_exists(cand) {
            return Ok(cand.to_string());
        }
    }
    bail!("cannot determine main branch; set it with: git config gj.main <name>")
}

impl Repo {
    pub fn open() -> Result<Repo> {
        if !git::in_repo() {
            bail!("not inside a git repository");
        }
        let list = git::capture(["worktree", "list", "--porcelain"])?;
        let root = parse_worktrees(&list)
            .into_iter()
            .next()
            .map(|w| w.path)
            .context("git worktree list returned nothing")?;
        let common_dir = PathBuf::from(
            git::capture(["rev-parse", "--path-format=absolute", "--git-common-dir"])?.trim(),
        );
        let main = main_branch()?;
        Ok(Repo {
            root,
            common_dir,
            main,
        })
    }

    pub fn worktrees(&self) -> Result<Vec<Worktree>> {
        Ok(parse_worktrees(&git::capture_in(
            &self.root,
            ["worktree", "list", "--porcelain"],
        )?))
    }

    pub fn worktree_of(&self, branch: &str) -> Result<Option<Worktree>> {
        Ok(self
            .worktrees()?
            .into_iter()
            .find(|w| w.branch.as_deref() == Some(branch)))
    }

    pub fn worktree_at(&self, path: &Path) -> Result<Option<Worktree>> {
        let want = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        Ok(self.worktrees()?.into_iter().find(|w| {
            fs::canonicalize(&w.path).unwrap_or_else(|_| w.path.clone()) == want
        }))
    }

    pub fn path_for(&self, branch: &str) -> PathBuf {
        if branch == self.main {
            self.root.clone()
        } else {
            self.root.join(WORKTREES_DIR).join(slug(branch))
        }
    }

    pub fn is_root(&self, path: &Path) -> bool {
        same_path(path, &self.root)
    }

    pub fn prune(&self) -> Result<()> {
        git::run_in(&self.root, ["worktree", "prune"])
    }

    pub fn ensure_exclude(&self) -> Result<()> {
        let info = self.common_dir.join("info");
        let file = info.join("exclude");
        let want = format!("{WORKTREES_DIR}/");
        if let Ok(existing) = fs::read_to_string(&file)
            && existing.lines().any(|l| l.trim() == want) {
                return Ok(());
            }
        fs::create_dir_all(&info).with_context(|| format!("creating {}", info.display()))?;
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file)
            .with_context(|| format!("opening {}", file.display()))?;
        writeln!(f, "{want}")?;
        Ok(())
    }
}

pub fn same_path(a: &Path, b: &Path) -> bool {
    let ca = fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
    let cb = fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf());
    ca == cb
}

/// True when `inner` is `outer` or lies below it.
pub fn is_within(inner: &Path, outer: &Path) -> bool {
    let ci = fs::canonicalize(inner).unwrap_or_else(|_| inner.to_path_buf());
    let co = fs::canonicalize(outer).unwrap_or_else(|_| outer.to_path_buf());
    ci.starts_with(&co)
}
