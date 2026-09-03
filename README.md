# gj

Interactive fuzzy branch / tag picker for git. Run `gj` inside a git repo,
type to filter, hit `Enter` to switch.

## Install

```sh
cargo install --path .
```

## What it lists

In this order:

1. **Local branches** — `main`
2. **Remote-only branches** — `feature-x  · remote` (de-duped against locals;
   `*/HEAD` symrefs filtered)
3. **Tags** — `v1.2.3  · tag` (newest version first)

## What it does on `Enter`

- Local / remote → `git switch <name>` (DWIM-tracks remote-only branches).
- Tag → `git checkout <name>` (lands detached at the tag).

`Esc` / `Ctrl-C` exits 130 without changing branches.

## Security model

Every git invocation goes through `src/git.rs`, which builds
`Command::new("git")` with vector arguments. No shell is ever invoked, so a
branch or tag name containing `;`, `$(…)`, or other shell metacharacters
cannot inject commands — git sees the whole string as a single argument.

## Layout

```
src/
├── main.rs    # entry + picker logic
└── git.rs     # the only place Command::new("git") lives
```
