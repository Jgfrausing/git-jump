# CLAUDE.md

Guidance for AI agents working in this repo. User-facing description is in
[README.md](./README.md).

## Architectural rules

### Single git chokepoint

`src/git.rs` is the **only** module that constructs `Command::new("git")`.
Everything else calls `git::run`, `git::capture`, or `git::in_repo`. This is
what keeps the tool injection-safe (no shell, vector args). If you need git
behavior that isn't exposed, add a helper to `git.rs` rather than spawning
git from anywhere else.

### Branches and tags are listed in fixed order

`main.rs::run` emits entries in this order:

1. Local branches (kind `Local`)
2. Remote-only branches (kind `Remote`) — de-duped against locals,
   `<remote>/HEAD` symrefs filtered, refs without a `<remote>/<branch>`
   structure dropped (git's symref-shortening can otherwise produce a bare
   remote name like `origin`)
3. Tags (kind `Tag`), newest version first

If you change this order, the user-facing behavior changes — call it out.

### Action depends on kind

`Local` / `Remote` selections go through `git switch` (which DWIM-tracks
remote-only branches). `Tag` selections go through `git checkout` (detached
HEAD). Don't unify these — `git switch` rejects tags by design.

### `Esc` / `Ctrl-C` exits 130

Cancellation handling for `inquire` matches the conventional SIGINT exit
code. Don't swallow `OperationCanceled` / `OperationInterrupted` as errors.

## Style

- No comments except where the **why** isn't obvious.
- `anyhow::Result` at the public boundary, `bail!` for clean error messages.
- No tests at present — the logic is small enough to read in one pass, and
  the branch-listing pipeline depends on a real git repo to be meaningful.
  If you add tests, gate any that touch a real repo so they're skipped in CI.

## Verification

```sh
cargo build           # must finish clean (no warnings)
cargo run             # inside a git repo, should open the picker
```

## Out of scope

The tool used to ship `clone` and `new-repo` subcommands that handled
identity routing and SSH host aliases per-URL. That was removed in favour of
`url.<alias>.insteadOf` rules in `~/.gitconfig`, which give the same routing
to *every* git invocation (not just ones that go through this tool). If you
find yourself reaching for "let's add a profile system" again, push back —
gitconfig already does it.
