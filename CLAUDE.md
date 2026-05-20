# CLAUDE.md

Guidance for AI agents working in this repo. The user-facing project description
is in [README.md](./README.md) — don't duplicate it here.

## Architectural rules

### Single git chokepoint

`src/git.rs` is the **only** module that constructs `Command::new("git")`.
Everything else calls `git::run`, `git::run_in`, `git::capture`,
`git::capture_in`, or `git::silent_in`. This is non-negotiable: it is what
makes the tool injection-safe (no shell, vector args). If you find yourself
wanting to spawn git from another module, add a helper to `git.rs` instead.

### URLs never go through a shell

Branch names, URLs, and namespaces are always passed as separate `args` entries
to git. Never `format!` user input into a string that gets split by whitespace.
The chokepoint rule above enforces this structurally.

### Profile resolution is order-sensitive

`Config::profiles` is an `IndexMap`, not a `HashMap`. The order in
`config.toml` is the matching order — first profile whose globs match wins.
Tests in `src/config.rs` lock this in (see `resolve_first_match_wins`). Do
not switch back to `HashMap`.

### `config_path()` is XDG, not platform

We do **not** use `dirs::config_dir()` because on macOS it returns
`~/Library/Application Support`. The current implementation honours
`$XDG_CONFIG_HOME` and falls back to `~/.config`. Keep it that way.

### SSH host rewriting only on SSH URLs

`config::rewrite_ssh_host` accepts `git@host:path` and `ssh://git@host/path`
and leaves anything else (most importantly HTTPS) untouched. The reasoning:
HTTPS uses credential helpers, not SSH keys, so silently rerouting through an
`~/.ssh/config` alias would be wrong.

## Adding a new subcommand

1. Create `src/cmd/<name>.rs` exporting `pub fn run(...) -> anyhow::Result<()>`.
2. Add `pub mod <name>;` to `src/cmd/mod.rs`.
3. Add a variant to the `Command` enum in `src/main.rs` and a match arm in
   `run()`.
4. If the command needs to resolve a profile from a URL, call
   `super::pick_profile(&cfg, &url)` — don't reimplement the resolve-or-prompt
   dance.

## Style

- Comments only where the **why** isn't obvious from the code. No "this
  function does X" comments — the function name does that.
- `anyhow::Result` everywhere user-visible. `bail!` for clean error messages.
- Cancellation handling for `inquire`: `OperationCanceled` /
  `OperationInterrupted` → `std::process::exit(130)`. Don't swallow them.
- Tests live in a `#[cfg(test)] mod tests` block at the bottom of the same
  file as the code under test. No separate `tests/` dir.

## Verification

```sh
cargo build           # must finish clean (no warnings)
cargo test            # unit tests in config.rs, cmd/clone.rs, cmd/new_repo.rs
cargo run -- --help   # sanity-check subcommand surface
```

There are no integration tests that touch a real remote; if you add one,
gate it behind a feature flag so CI never tries to clone something.

## Out of scope (don't add without asking)

- libgit2 / the `git2` crate — we shell out on purpose.
- GPG / signing-key configuration.
- Calling out to `gh` (or other host CLIs) to create remote-side repos.
- Branch operations beyond `switch` (delete, rebase, worktree, …).
