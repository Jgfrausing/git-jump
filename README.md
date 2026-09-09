# gj

A git front end where every branch lives in its own worktree. Type `gj` where
you would type `git`. The few commands that change or delete a branch are done
the worktree way; everything else is handed to `git` untouched, aliases
included, so `gj s`, `gj cp "msg"`, `gj log`, `gj stash` behave exactly like
`git ...`.

The repo directory stays on main. Every other branch is checked out under
`<repo>/.worktrees/<slug>` (created on demand, `feat/x` becomes `feat-x`), and
switching to a branch moves your shell into that directory. Deleting a branch
removes its worktree too, so there is no stash dance around `checkout main`.

[TUTORIAL.md](./TUTORIAL.md) walks through one branch's life with gj.

## Install

```sh
cargo install --path .
```

Then add the shell wrapper below. A binary cannot change its parent shell's
directory, so gj writes the "move here" path to the file named by
`GJ_CD_FILE` and the wrapper does the `cd` after gj exits. Without
`GJ_CD_FILE` set (scripts, tests) gj prints the path on stdout instead.

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

## Commands

Dispatch is on the first argument. The intercepted forms are listed exactly;
anything that does not match a row is `exec git <args>`.

| You type | gj does |
|---|---|
| `gj --help`, `gj -h`, `gj help` | usage summary. `gj help <topic>` still goes to git |
| `gj` | picker over branches and tags, main first. A local branch is marked `· root` when the root directory holds it and `· wt` when a linked worktree does, so main carries `· root` in the usual case and nothing when the root has been moved off it. Both markers appear together, and `· wt ×2` and up, when `worktree add --force` or `switch --ignore-other-worktrees` has put one branch in several trees. Enter moves to that branch's worktree |
| `gj co <b>`, `gj checkout <b>`, `gj switch <b>` | go to `<b>`'s worktree, creating it if needed. Only for exactly one positional that names a local or remote-only branch. A sha, tag, file path, `origin/main <paths>`, any flag or `--` passes through to git |
| `gj co <typo>` | opens the picker filtered by `<typo>`, when `<typo>` is not a branch, not a rev and not a path, and there is a tty. Without a tty git's own error is returned |
| `gj co -`, `gj switch -`, `gj -` | `cd -`, done by the wrapper |
| `gj b <words...>` | new branch named `words-joined-with-dashes`, based on freshly synced main. `--here` stacks on the current worktree's HEAD, `--no-sync` skips the fetch |
| `gj co -b <b> [<start>]`, `gj switch -c <b> [<start>]` | new branch. With `<start>` no sync; without, same as `gj b`. `--here` and `--no-sync` work here too |
| `gj branch -d <b>...`, `gj branch -D <b>...` | delete branch and worktree. Any other `branch` form passes through |
| `gj fapp`, `gj delete-gone` | fetch and prune, ff-pull main in the root, remove branches whose upstream is gone together with their worktrees, `worktree prune` |
| `gj up` | sync, then `git merge <main>` in the current worktree. Errors in the root ("already on main") |
| `gj adopt` | root is on a non-main branch: move that branch to a worktree and put the root back on main |
| `gj wt ls` | list worktrees with their branch. `--tsv` prints `path<TAB>branch` lines, root first, for editors and scripts |
| `gj wt path <b>` | print the worktree path for `<b>` (existing, or where it would go) |
| `gj wt root`, `gj wt main` | print the root directory / the main branch name |
| `gj wt prune` | `git worktree prune` |
| `gj wt rm <b>... [-f]` | same as `branch -d` / `-D`. With no names, a multi-select over branches that have a worktree |
| everything else | `exec git <args>` |

Tags in the picker are checked out in place with `git checkout <tag>`
(detached HEAD, no move). The next `gj co <branch>` on a worktree that was
left detached puts it back on its branch.

### Mixing with plain git

Running `git checkout feat/x` in the root moves the root off main. The next
gj operation that needs the root (`gj co main`, `gj b`, `gj adopt`) moves that
branch into a worktree when the root is clean, and exits 4 with a "commit or
stash there first" message when it is dirty. Everything else keeps working in
the meantime.

`git branch -d` refuses a branch checked out in any worktree, so plain
`git delete-gone` prints "Cannot delete branch ... checked out at" for gone
branches gj holds in worktrees. `gj fapp` is the version that handles it.

### Exit codes

| Code | Meaning |
|---|---|
| 0 | ok |
| 1 | git or internal error |
| 2 | usage |
| 4 | refused because a worktree has uncommitted changes |
| 130 | picker cancelled with `Esc` / `Ctrl-C` |

Pass-through exits with git's own code.

## Configuration

| Key | Effect |
|---|---|
| `git config gj.main <name>` | the main branch. Otherwise `origin/HEAD`, else a local `main`, else `master` |
| `git config gj.worktrees false` | per-repo opt-out. Intercepted commands go to git unchanged (in-place checkout), useful for build trees. Bare repos are always off |

gj adds `.worktrees/` to `.git/info/exclude` the first time it creates a
worktree, so the directory never shows up in `git status`.

## How it decides what is a branch

`gj co <x>` goes to a worktree only when `<x>` is the main branch, an existing
local branch, or `<remote>/<x>` exists for one of the configured remotes
(git's own DWIM then creates the tracking branch). Anything that resolves as a
commit (`rev-parse --verify <x>^{commit}`) or exists as a path is passed to
git, so `gj co <sha>`, `gj co v1.2.3` and `gj co README` behave as before.

## Security model

Every git invocation goes through `src/git.rs`, which builds
`Command::new("git")` with vector arguments. No shell is ever invoked, so a
branch name containing `;`, `$(…)` or other shell metacharacters cannot inject
commands. Pass-through is `exec`, so git inherits the terminal: pagers,
colours and interactive editors work, and the exit code is git's.

## Layout

```
src/
├── main.rs    # dispatch on argv, GJ_CD_FILE reporting
├── picker.rs  # inquire Select / MultiSelect over branches and tags
├── wt.rs      # go, new, rm, sync, up, adopt
├── repo.rs    # root, main branch, worktree list, slug paths, exclude file
├── exit.rs    # exit-code error type
└── git.rs     # the only place Command::new("git") lives
tests/
└── wt.sh      # scripted checks against a throwaway remote and clone
```
