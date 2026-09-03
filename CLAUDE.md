# CLAUDE.md

Guidance for AI agents working in this repo. User-facing description is in
[README.md](./README.md).

## What gj is

A git front end where branch = worktree. The root directory stays on main,
every other branch lives in `<root>/.worktrees/<slug>`, and switching moves
the shell there. Commands gj does not recognise are `exec`'d to git verbatim.

## Architectural rules

### Single git chokepoint

`src/git.rs` is the **only** module that constructs `Command::new("git")`.
`repo.rs`, `wt.rs`, `picker.rs` and `main.rs` call `git::run_in`,
`git::run_tty_in`, `git::capture`, `git::capture_in`, `git::ok`, `git::ok_in`
or `git::exec`. Vector args, no shell. If you need git behaviour that isn't
exposed, add a helper to `git.rs`.

### stdout is the report channel

gj reports at most one "move here" directory per run: to `$GJ_CD_FILE` when
the zsh wrapper set it, otherwise to stdout (scripts and `tests/wt.sh` rely on
this). So internal git steps must not print to stdout. `git::run_in` forwards
git's stdout to our stderr for that reason. `git::run_tty_in` inherits every
stream and is only for `merge` in `gj up`, which may open an editor and never
reports a directory in the same run.

### Pass-through is exec

Anything that does not match an intercepted shape in `main.rs::run` becomes
`git::exec(args)`: same tty, same pager, same aliases, same exit code. Adding
an intercepted command means git loses that word. `b`, `fapp`, `delete-gone`,
`up`, `adopt`, `wt` are already taken; check `~/.gitconfig` aliases before
taking another.

Intercepted commands also pass through outside a repo, in a bare repo, and
where `git config gj.worktrees` is `false` (`main.rs::gate`).

### Dispatch is hand-rolled

No clap. The intercepted grammar is a fixed set of shapes and everything else
must reach git untouched, which an argument parser would fight. One match arm
per row of the README table.

### Listing order in the picker

`picker::entries` emits main first, then the other local branches, then
remote-only branches (de-duped against locals, `<remote>/HEAD` and bare
remote names dropped), then tags newest version first. Changing this changes
user-facing behaviour; call it out.

### Action depends on kind

`Local` / `Remote` picks go through `wt::go` (worktree, shell moves). `Tag`
picks are `git checkout <tag>` in place: a tag is not a branch, and a detached
worktree has nothing for `rm` to delete.

### Exit codes

0 ok, 1 git or internal error, 2 usage, 4 refused because of uncommitted
changes, 130 cancelled. `exit.rs` carries 2 and 4 as a typed error that
`main` downcasts. Don't swallow inquire's `OperationCanceled` /
`OperationInterrupted`; `picker::cancelled` exits 130.

### Removing a directory the process is standing in

`wt::leave_if_inside` chdirs to the root before a worktree is removed, and the
op then reports the root so the shell follows. git happily removes the
worktree it was started in; it is the next git call that dies with "Unable to
read current working directory".

## Style

- No comments except where the **why** isn't obvious.
- `anyhow::Result` at the public boundary, `bail!` for clean error messages,
  `exit::usage` / `exit::refused` for codes 2 and 4.
- Unit tests: none. `tests/wt.sh` is the test suite: bash, builds a bare
  remote and clones under `mktemp -d`, never touches a real repo, and is not
  wired into `cargo test`. Extend it when adding behaviour.

## Verification

```sh
cargo build           # must finish clean (no warnings)
cargo clippy          # clean
tests/wt.sh           # all checks pass
cargo install --path . # refreshes ~/.cargo/bin/gj
```

## Out of scope

The tool used to ship `clone` and `new-repo` subcommands that handled
identity routing and SSH host aliases per-URL. That was removed in favour of
`url.<alias>.insteadOf` rules in `~/.gitconfig`, which give the same routing
to *every* git invocation. If you find yourself reaching for "let's add a
profile system" again, push back; gitconfig already does it.
