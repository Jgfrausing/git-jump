# gj: a git front end where branch = worktree

Plan for `~/code/git-jump`. Everything below is a change to this repo, except the zsh wrapper in
the last section. `~/.gitconfig`, its aliases, lazygit and the `dot` setup are not touched.

gj becomes the thing you type instead of `git`. It intercepts the handful of git commands that
change or delete a branch and does them the worktree way; every other argument list is handed to
`git` untouched, so `gj s`, `gj cp "msg"`, `gj pr`, `gj stash`, `gj log`, `gj diff`, `gj co <sha>`
behave exactly like `git ...`, aliases included. There is nothing to remember: `gj` where `git`
used to go.

## Context

Today gj is a fuzzy picker over local branches, remote-only branches and tags that runs
`git switch <name>` (or `git checkout <tag>`). The wanted workflow: the repo directory stays on
main, every other branch lives in `<repo>/.worktrees/<slug>`, switching to a branch moves the
shell into that worktree (created on demand), and deleting a branch also removes its worktree.

Command history (atuin, 11.5k commands): `git co <x>` 253 (169 of them `co main`, 38 `co -`),
`git fapp` 209, `git b <words>` 105, `gj` 61, `git stash` / `stash pop` 41 / 33 (almost always
around `co main; fapp`), `git merge -` 22, `git delete-gone` 21, `git branch -D` 3. Frequent
sequences: `co <x> -> fapp` 116 times, `co main; fapp; b feat/...; pr`, and `co main; co -;
merge -`. Typos `co mian` / `co mai` 6 times. Four real gestures: go to main and sync it, start a
branch off fresh main, pull main into the current branch, jump to a branch. With worktrees the
stash dance disappears, since main is always checked out in the root.

Facts that shape the design:

- A binary cannot change its parent shell's directory, and gj must not capture stdout of
  pass-through commands (pagers, colours, `git log`). So gj reports a "move here" directory
  through a file named by `GJ_CD_FILE`, and a small zsh wrapper reads it after gj exits. The
  same pattern as the `y()` yazi wrapper in `~/.zshrc` lines 84-92. Without `GJ_CD_FILE` set
  (scripts, tests) gj prints the path on stdout instead.
- Pass-through is `exec git <args>` (`std::os::unix::process::CommandExt::exec`): git inherits the
  tty, the pager works, aliases resolve, the exit code is git's.
- inquire draws the picker on stderr (inquire 0.9.4 `src/terminal/crossterm.rs:97`). Cargo.lock
  pins 0.7.5; confirm with `gj >/dev/null` after building, or bump to `inquire = "0.9"`.
- `git worktree add <path> <branch>` DWIMs a remote-only name into a tracking branch (git 2.53).
- `git branch -d` refuses a branch checked out in any worktree, so removal is worktree first,
  branch second. Plain `git delete-gone` / `git fapp` (untouched) will print "Cannot delete
  branch ... checked out at" for gone branches gj holds in worktrees; `gj fapp` is the version
  that handles it.
- `worktree.*` is a git-owned config section, so gj's keys live under `gj.*`.
- `~/code` state: lazer_trader has 3 prunable worktree entries from a manual attempt;
  tmux-browser-bridge has a worktree outside `.worktrees/` (`~/code/tbb-session-id`), so existing
  worktrees are found by branch, not by computed path. 13 repos sit on a non-main branch with
  clean trees (JEPX-connector, lazer_hub, lazer_trader_otel_collector, lazer_trader, NPSProvider,
  orderbook_exploration, rusty_nats, rusty_notifications, rusty_timber, rusty-algos, skynet,
  tmux-pr5433, tmux). claude-butler has no `origin/HEAD` symref; tmux and tmux-pr5433 use `master`.
- `~/.local/bin/orchestra` creates `.orchestra/<slug>-HHMMSS` worktrees and hides that dir via
  `.git/info/exclude` (lines 63-65). gj does the same for `.worktrees/`. Orchestra's worktrees
  appear in `gj wt ls` and prune when stale; nothing else about them changes.
- git-jump's working tree has 11 uncommitted changes (removal of `clone` / `new-repo`). Commit
  those first so this diff stays readable.

## Command surface

Dispatch on the first argument. Intercepted forms are listed exactly; anything that does not
match a row is `exec git <args>`.

| You type | gj does | Notes |
|---|---|---|
| `gj` | picker → go | as today, plus main first and `· wt` marks on branches that have a worktree |
| `gj co <b>`, `gj checkout <b>`, `gj switch <b>` | go | exactly one positional, no flags. `<b>` a local or remote-only branch → worktree. Otherwise (sha, tag, file, `origin/main <paths>`, any flag, `--`) → pass through |
| `gj co <typo>` | picker filtered by `<typo>` | only when `<typo>` is not a branch, not a rev (`rev-parse --verify --quiet <x>^{commit}` fails) and not an existing path |
| `gj co -`, `gj switch -`, `gj -` | `cd -` | done by the wrapper; gj has no state |
| `gj b <words...>` | new | slug = words joined with `-`, same rule as the `b` alias; based on freshly synced main by default, `--here` to stack on the current worktree's HEAD, `--no-sync` to skip the fetch |
| `gj co -b <b> [<start>]`, `gj checkout -b`, `gj switch -c <b> [<start>]` | new | with explicit `<start>` no sync; without, same default as `gj b`. Other checkout/switch flags → pass through |
| `gj branch -d <b>...`, `gj branch -D <b>...` | rm | any other `branch` form (`-r`, `-a`, `-m`, `--list`, no args) → pass through |
| `gj fapp`, `gj delete-gone` | sync | fetch/prune, ff-pull root main, remove gone branches with their worktrees, `worktree prune` |
| `gj up` | sync, then `git merge <main>` in the current worktree | replaces `co main; co -; merge -`. Error in root: "already on main" |
| `gj adopt` | migration | root on a non-main branch → move it to a worktree, root back to main |
| `gj wt ls`, `gj wt path <b>`, `gj wt root`, `gj wt main`, `gj wt prune`, `gj wt rm <b>... [-f]` | helpers | `gj wt rm` with no args opens a multi-select. `gj worktree ...` (git's own) still passes through |
| everything else | `exec git <args>` | `s`, `cp`, `pr`, `stash`, `log`, `rm <file>`, `merge <x>`, `restore`, `publish`, ... |

Tags in the picker stay as today: `git checkout <tag>` in the current worktree, detached, no
move. A tag is not a branch and a detached worktree has nothing for rm to delete. The next
`gj co <branch>` repairs a slug worktree left detached (go, step 6).

Reporting a directory: write it to `$GJ_CD_FILE` if set, else print on stdout. At most one
directory per run. Exit codes: 0 ok, 1 git or internal error, 2 usage, 4 refused because of
uncommitted changes, 130 cancelled; pass-through exits with git's code.

Mixing with plain git stays fine: `git co feat/x` in the root moves the root off main; the next
gj branch operation runs adopt when the root is clean, and exits 4 with a "commit or stash there"
message when it is dirty.

## Shared pieces

- `root`: first `worktree` entry of `git worktree list --porcelain`; correct from inside a linked
  worktree.
- `main`: `git config gj.main`, else `refs/remotes/origin/HEAD` with `origin/` stripped, else
  `main` if `refs/heads/main` exists, else `master`, else error "cannot determine main branch;
  set it with: git config gj.main <name>".
- `slug(b)`: `/` → `-`. `path_for(b)`: root for main, else `root/.worktrees/slug`.
- `worktree_of(b)`: porcelain lookup by `branch refs/heads/b`, path wherever it is.
- `clean(dir)`: `git -C dir status --porcelain` empty. Ignored files (`target/`, `.env`) do not
  count and never block adopt or rm.
- `ensure_exclude()`: append `.worktrees/` to `<git-common-dir>/info/exclude` if missing.
- `enabled()`: false when `core.bare` is true or `git config gj.worktrees` is `false`. Disabled →
  intercepted commands pass through to git unchanged (in-place checkout). Per-repo opt-out for
  build trees.
- After any directory removal, git calls run with `-C root` so a deleted cwd cannot break them.

## Operations

### go `<b>`

1. `git worktree prune`.
2. `<b>` is main: root on main → report root. Root detached and clean → `git -C root switch
   main`, report root. Root on B and clean → adopt B, report root. Dirty → exit 4 "root is on 'B'
   with uncommitted changes; commit or stash there first".
3. `worktree_of(b)` is Some: equals root → adopt (needs clean root), report the new path;
   otherwise report that path as is (keeps `tbb-session-id` working without moving it).
4. `path_for(b)` is a registered worktree holding something else: clean → `git -C path switch b`,
   report; dirty → exit 4 "slug collision: <path> holds '<other>'". Exists on disk but
   unregistered → error "remove it or run git worktree repair".
5. `ensure_exclude`, `mkdir -p root/.worktrees`, `git worktree add <path> <b>` with stderr
   inherited. Report path.

### new `<name> [--here | --no-sync | <start>]`

`git check-ref-format --branch <name>` else exit 2. Exists → error "branch exists; use gj co
<name>". Base: `<start>` if given; `--here` → HEAD of the current worktree; otherwise main after
the fetch-and-ff-pull half of sync (fetch failure → warn on stderr, use local main). Then
`ensure_exclude`, `git worktree add -b <name> <path> <base>`, report path. Whether `worktree add
-b` sets an upstream when the base is `origin/main` is unverified; test 4 below checks it, and
if it does, pass `--no-track` so a later `git pull` in the new worktree never targets main
(today's `checkout -b` off local main has no upstream either; `git publish` sets it).

### rm `<b>... [-f]`

For each name: main → refuse. Unknown → error. No worktree → `git -C root branch -d` (`-D` with
`-f`). Held by root → clean: `git -C root switch main`, then delete; dirty → exit 4. Linked
worktree without `-f`: dirty → exit 4 "worktree has uncommitted or untracked changes (use -D)";
else `git -C wt switch --detach`, `git -C root branch -d <b>` (git's merged check runs while the
tree still exists; on "not fully merged" restore with `switch <b>` and exit 1), then `git
worktree remove <wt>`. With `-f`: `worktree remove --force` then `branch -D`. If the cwd was
inside a removed worktree, report root.

### sync, up

sync: `git -C root fetch --all --prune`. Root on main and clean → `git -C root pull --ff-only`,
otherwise a stderr note and skip. Gone branches = `for-each-ref --format='%(refname:short)
%(upstream:track)' refs/heads` with second field `[gone]` (not `git branch -vv`, which marks
worktree branches with `+`). Each → rm with force, matching `delete-gone`'s `-D`. Then `git
worktree prune`. Report root only if the cwd was removed. up = sync, then `git merge <main>` in
the current worktree; conflicts are left to the user as with `git merge -` today.

### adopt

Root on main → report root. Detached and clean → `switch main`. On B: require clean or exit 4;
prune; `path_for(B)` must not exist; `ensure_exclude`; `git -C root switch --detach`;
`git -C root worktree add <path> B` (on failure `switch B` back, exit 1); `git -C root switch
<main>` (DWIM creates local main from origin if missing; if that fails say so and leave the
worktree in place). Report path.

## Code layout

- `src/git.rs`: still the only module constructing `Command::new("git")`. Add `run_in(dir,
  args)`, `capture_in(dir, args)`, `run_inherit_stderr(args)`, `ok(args) -> bool`, and
  `exec(args) -> !` for pass-through. Vector args only, no shell.
- `src/repo.rs` (new): `Repo { root, common_dir, main }`, `Worktree { path, branch:
  Option<String>, prunable: bool }`, porcelain parser, `slug`, `path_for`, `worktree_of`,
  `is_clean`, `ensure_exclude`, `enabled`, `main_branch`.
- `src/wt.rs` (new): `go`, `new`, `rm`, `sync`, `up`, `adopt`, each returning
  `Result<Option<PathBuf>>` (Some = report and the shell moves).
- `src/picker.rs`: current entry building and `Select`, plus `MultiSelect` for `wt rm`, both with
  an optional starting filter.
- `src/main.rs`: hand-rolled dispatch on `args[1..]` (no clap: the intercepted grammar is a
  fixed set of shapes and everything else must reach git verbatim, which an argument parser
  would fight). One function per row of the table above, each returning `Intercept(op)` or
  `PassThrough`. `report(path)` writes `GJ_CD_FILE` or stdout.
- `Cargo.toml`: consider `inquire = "0.9"`.
- `AGENTS.md`: rewrite "Action depends on kind" and "Out of scope" for the new job ("git front
  end, branch = worktree, unknown args exec git"); extend the single-chokepoint rule to `repo.rs`
  and `wt.rs`; keep the fixed listing order rule with main first; document the `GJ_CD_FILE`
  contract, the pass-through rule and the exit codes.
- `README.md`: the table above, the wrapper snippet, the `gj.main` / `gj.worktrees` keys.

Build and install: `cargo build` warning-free, `cargo install --path .` refreshes `~/.cargo/bin/gj`.

## Outside the repo (not for the gj agent)

`~/.zshrc`, or a file it sources:

```zsh
gj() {
  case "$1 $2" in
    "- "|"co -"|"checkout -"|"switch -") builtin cd -; return ;;
  esac
  local f; f=$(mktemp -t gj-cd.XXXXXX) || return
  GJ_CD_FILE=$f command gj "$@"
  local rc=$? d; d=$(<"$f"); rm -f -- "$f"
  [[ -n $d && -d $d && $d != $PWD ]] && builtin cd -- "$d"
  return $rc
}
```

After install, the one-time migration is `gj adopt` in each of the 13 repos above, lazer_trader
first (its stale entries prune on the way). For tmux and tmux-pr5433 check `gj wt main` prints
`master`; they look like build trees, and `git config gj.worktrees false` leaves either alone.
jj removal is a separate task, not part of this plan.

## Verification

`cargo build` with no warnings, `cargo clippy` clean.

Scripted test `tests/wt.sh` (bash; bare `remote.git` and a clone under a temp dir; gated so
`cargo test` never touches a real repo). gj runs without `GJ_CD_FILE`, so the reported path is
its stdout.

1. `gj b my new thing` → prints `.worktrees/my-new-thing`; exclude entry present; root clean and on main.
2. `gj co -b feat/b` from inside a worktree, then with `--here` → default bases on main, `--here` on that HEAD.
3. `gj co feat/a` from another worktree → prints the existing path, worktree count unchanged.
4. `gj switch remote-only` (branch pushed then deleted locally) → worktree created, `@{u}` is `origin/remote-only`; new branch off `origin/main` has the intended upstream behaviour.
5. `gj co main` from a worktree → prints root.
6. Pass-through: `gj s`, `gj log -1`, `gj co README`, `gj co <sha>`, `gj checkout origin/main README`, `gj branch`, `gj rm --cached README`, `gj worktree list` → identical output and exit code to `git ...`, stdout not swallowed (compare with `git`, and check `gj log` under `GIT_PAGER=cat`).
7. `gj co notabranch` with no tty → git's own pathspec error and exit code (picker only on a tty).
8. `git checkout <sha>` inside a worktree, then `gj co <its-branch>` from root → repaired, path printed.
9. `gj branch -D feat/b` with cwd inside its worktree → prints root; dir and branch gone.
10. `gj branch -d` of an unmerged branch → exit 1 with git's "not fully merged"; worktree still present and on its branch. `-D` removes it.
11. `gj fapp` after `git push origin -d gone` → `.worktrees/gone` removed; prints root when cwd was inside it, nothing otherwise.
12. `gj up` from a worktree → main merged into it; from root → error.
13. Fresh clone, `git switch -c pre`, commit, `gj co main` → adopt ran, `.worktrees/pre` holds `pre`, root on main. Dirty variant (`touch x`) → exit 4, nothing moved.
14. `git config gj.worktrees false`, `gj co feat/a` → plain in-place `git switch`, nothing printed.
15. `git init --initial-branch=master`, `gj wt main` → `master`; no `origin/HEAD` symref and a local `main` → `main`.
16. `GJ_CD_FILE=/tmp/x gj co feat/a` → path in the file, stdout empty.

Manual: `gj` picker shows main first and `· wt` marks; `gj co mai` opens the filtered picker;
Esc → 130 with nothing written; `gj >/dev/null` still shows the picker; `gj -` and `gj co -`
return to the previous directory via the wrapper; round trip in lazer_trader after adopt.

Unverified until run: inquire 0.7.5 stderr rendering (0.9.4 confirmed); upstream behaviour of
`worktree add -b` with a remote base.
