mod exit;
mod git;
mod picker;
mod repo;
mod wt;

use anyhow::{Result, bail};
use exit::usage;
use repo::Repo;
use std::io::IsTerminal;
use std::path::Path;
use wt::{Base, Moved};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(Some(path)) => report(&path),
        Ok(None) => {}
        Err(err) => {
            if let Some(e) = err.downcast_ref::<exit::Exit>() {
                eprintln!("error: {}", e.msg);
                std::process::exit(e.code);
            }
            eprintln!("error: {err:#}");
            std::process::exit(1);
        }
    }
}

/// The "move here" contract: the path goes to `$GJ_CD_FILE` when the zsh
/// wrapper set one, otherwise to stdout for scripts.
fn report(path: &Path) {
    match std::env::var_os("GJ_CD_FILE") {
        Some(f) => {
            if let Err(e) = std::fs::write(&f, format!("{}\n", path.display())) {
                eprintln!("gj: could not write {}: {e}", Path::new(&f).display());
            }
        }
        None => println!("{}", path.display()),
    }
}

fn run(args: &[String]) -> Result<Moved> {
    if args.is_empty() {
        return pick(None);
    }
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    match a[0] {
        "co" | "checkout" | "switch" => checkout(args, &a),
        "b" => {
            let repo = gate(args)?;
            let (words, base) = parse_new(&a[1..])?;
            if words.is_empty() {
                return Err(usage("usage: gj b <words...> [--here] [--no-sync]"));
            }
            let name = words.join("-");
            wt::new(&repo, &name, base)
        }
        "branch" => branch(args, &a),
        "fapp" | "delete-gone" => wt::sync(&gate(args)?),
        "up" => wt::up(&gate(args)?),
        "adopt" => wt::adopt(&gate(args)?),
        "wt" => worktree_cmd(args, &a[1..]),
        _ => git::exec(args),
    }
}

/// Intercepted commands only apply inside a repo with worktree mode on.
/// Anywhere else the argument list goes to git exactly as typed.
fn gate(args: &[String]) -> Result<Repo> {
    if !git::in_repo() || !repo::enabled() {
        git::exec(args);
    }
    Repo::open()
}

fn parse_new<'a>(rest: &[&'a str]) -> Result<(Vec<&'a str>, Base)> {
    let mut words = Vec::new();
    let mut here = false;
    let mut sync = true;
    for w in rest {
        match *w {
            "--here" => here = true,
            "--no-sync" => sync = false,
            _ if w.starts_with('-') => {
                return Err(usage(format!("unknown flag {w}; gj b takes --here and --no-sync")));
            }
            _ => words.push(*w),
        }
    }
    let base = if here {
        Base::Here
    } else {
        Base::Main { sync }
    };
    Ok((words, base))
}

fn checkout(args: &[String], a: &[&str]) -> Result<Moved> {
    let create = if a[0] == "switch" { "-c" } else { "-b" };
    match &a[1..] {
        [b] if !b.starts_with('-') => go_or_pass(args, b),
        [flag, rest @ ..] if *flag == create && !rest.is_empty() => {
            let repo = gate(args)?;
            let (words, base) = parse_new(rest)?;
            match words.as_slice() {
                [name] => wt::new(&repo, name, base),
                [name, start] if matches!(base, Base::Main { sync: true }) => {
                    wt::new(&repo, name, Base::Start(start.to_string()))
                }
                _ => git::exec(args),
            }
        }
        _ => git::exec(args),
    }
}

fn go_or_pass(args: &[String], b: &str) -> Result<Moved> {
    let repo = gate(args)?;
    if b == repo.main || repo::is_branch(b) {
        return wt::go(&repo, b);
    }
    if repo::is_rev(b) || Path::new(b).exists() {
        git::exec(args);
    }
    let tty = std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
    if tty {
        pick(Some(b))
    } else {
        git::exec(args)
    }
}

fn branch(args: &[String], a: &[&str]) -> Result<Moved> {
    match &a[1..] {
        [flag @ ("-d" | "-D"), names @ ..]
            if !names.is_empty() && names.iter().all(|n| !n.starts_with('-')) =>
        {
            let repo = gate(args)?;
            let names: Vec<String> = names.iter().map(|s| s.to_string()).collect();
            wt::rm(&repo, &names, *flag == "-D")
        }
        _ => git::exec(args),
    }
}

fn worktree_cmd(args: &[String], rest: &[&str]) -> Result<Moved> {
    const HELP: &str = "usage: gj wt ls | path <branch> | root | main | prune | rm [<branch>...] [-f]";
    let repo = gate(args)?;
    match rest {
        ["ls"] => wt::list(&repo).map(|_| None),
        ["path", b] => {
            let path = match repo.worktree_of(b)? {
                Some(w) => w.path,
                None => repo.path_for(b),
            };
            println!("{}", path.display());
            Ok(None)
        }
        ["root"] => {
            println!("{}", repo.root.display());
            Ok(None)
        }
        ["main"] => {
            println!("{}", repo.main);
            Ok(None)
        }
        ["prune"] => repo.prune().map(|_| None),
        ["rm", more @ ..] => {
            let force = more.contains(&"-f");
            let mut names: Vec<String> = more
                .iter()
                .filter(|s| **s != "-f")
                .map(|s| s.to_string())
                .collect();
            if names.iter().any(|n| n.starts_with('-')) {
                return Err(usage(HELP));
            }
            if names.is_empty() {
                names = picker::multi_select("remove worktrees", wt::linked_branches(&repo)?)?;
                if names.is_empty() {
                    return Ok(None);
                }
            }
            wt::rm(&repo, &names, force)
        }
        _ => Err(usage(HELP)),
    }
}

fn pick(filter: Option<&str>) -> Result<Moved> {
    if !git::in_repo() {
        bail!("not inside a git repository");
    }
    let cwd = std::env::current_dir()?;
    if !repo::enabled() {
        let chosen = picker::select(picker::entries(None, &[])?, filter)?;
        match chosen.kind {
            picker::Kind::Local | picker::Kind::Remote => {
                git::run_in(&cwd, ["switch", &chosen.name])?
            }
            picker::Kind::Tag => git::run_in(&cwd, ["checkout", &chosen.name])?,
        }
        return Ok(None);
    }
    let repo = Repo::open()?;
    let entries = picker::entries(Some(&repo.main), &repo.worktrees()?)?;
    let chosen = picker::select(entries, filter)?;
    match chosen.kind {
        picker::Kind::Local | picker::Kind::Remote => wt::go(&repo, &chosen.name),
        // A tag is not a branch: detach in place, as before worktree mode.
        picker::Kind::Tag => {
            git::run_in(&cwd, ["checkout", &chosen.name])?;
            Ok(None)
        }
    }
}
