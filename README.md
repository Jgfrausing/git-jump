# git-jump

A small Rust CLI that streamlines two git workflows:

1. **Fuzzy branch switching** — `git-jump` opens an interactive picker over local
   and remote branches and runs `git switch` on the choice.
2. **Contextual identity routing** — `git-jump clone <url>` and
   `git-jump new-repo <namespace>` resolve the URL against profile rules in a
   TOML config and apply the right git identity (and, optionally, SSH host
   alias) automatically.

Because the binary is named `git-jump`, git treats it as a subcommand, so
`git jump`, `git jump clone …`, and `git jump new-repo …` all work too.

## Install

```sh
cargo install --path .
```

## Commands

### `git-jump` (no args)

Lists local branches first, then remote-only branches (de-duped, `HEAD`
pointers stripped). Type to fuzzy-filter; `Enter` to checkout. Remote-only
selections use `git switch <name>` which auto-creates a tracking branch on
modern git.

### `git-jump clone <url> [<dir>]`

Resolves the URL against your profile rules, clones, then sets the local
`user.name` / `user.email` from the matched profile. If the profile has a
`host_alias`, SSH-form URLs are rewritten to use it (e.g.
`git@github.com:...` → `git@github-personal:...`) so openssh picks the right
key. HTTPS URLs are left untouched.

### `git-jump new-repo [<namespace-url>]`

Pushes the current project to a brand-new remote.

- The repo name is taken from the project root directory.
- The namespace URL is the `host/path` to push to (e.g. `github.com/yourname`).
  If omitted you are prompted for it.
- Accepts `github.com/foo`, `https://github.com/foo`,
  `git@github.com:foo`, or `ssh://git@github.com/foo`.
- Steps: `git init` (if needed) → set identity → add `origin` (bails if one
  exists) → make an initial commit if no `HEAD` → `git push -u origin HEAD`.

git-jump does **not** create the remote repo for you. Create it via the
provider UI or `gh repo create` first, then run `new-repo`.

## Configuration

Path: `~/.config/git-jump/config.toml` (respects `$XDG_CONFIG_HOME`).
git-jump does **not** use macOS's `~/Library/Application Support` even though
that is the platform default — XDG is intentional.

```toml
# First profile whose `patterns` match wins. If no profile matches, falls back
# to `default`; if `default` is unset, you are prompted to pick one.
default = "personal"

[profiles.work]
name       = "Your Name"
email      = "you@company.com"
host_alias = "github-work"           # optional — see SSH section
patterns = [
  "github.com/your-org/*",
  "dev.azure.com/your-org/*",
]

[profiles.personal]
name       = "Your Name"
email      = "you@personal.tld"
host_alias = "github-personal"
patterns = ["github.com/your-user/*"]
```

Patterns are matched against `"<host>/<path>"` using `globset`. The path has
any `.git` suffix stripped and is **not** URL-decoded. `**` matches across
slashes; `*` does not.

### SSH host aliases

If you use multiple GitHub identities, define host aliases in `~/.ssh/config`:

```
Host github-personal
  HostName     github.com
  User         git
  IdentityFile ~/.ssh/id_ed25519_personal
  IdentitiesOnly yes

Host github-work
  HostName     github.com
  User         git
  IdentityFile ~/.ssh/id_ed25519
  IdentitiesOnly yes
```

Then set `host_alias = "github-personal"` on the profile. git-jump rewrites
SSH-form URLs to use the alias before invoking git, so openssh selects the
matching `IdentityFile`. The rewrite is sticky: the cloned `origin` URL
contains the alias, so subsequent `git pull` / `push` in that repo
automatically use the right key.

## Security model

The only place `Command::new("git")` is constructed is `src/git.rs`. Every
call takes its arguments as a `&[impl AsRef<OsStr>]` — no shell is ever
invoked, no strings are concatenated into commands. A URL, branch name, or
namespace containing `;` / `&&` / `$( … )` cannot inject extra commands; git
sees the whole thing as a single argument and fails its own validation.

## Project layout

```
src/
├── main.rs          # clap entry; dispatches to cmd::*
├── git.rs           # the only place Command::new("git") lives
├── config.rs        # Config / Profile, resolve(url), rewrite_ssh_host
└── cmd/
    ├── mod.rs       # shared `pick_profile` helper
    ├── switch.rs    # branch picker (default invocation)
    ├── clone.rs     # contextual clone
    └── new_repo.rs  # init + push to new remote
```

## Status

Pre-1.0. The CLI surface is small but stable in shape. Things deliberately
out of scope: GPG / signing-key handling, GitHub-side repo creation (use
`gh`), worktree management, anything beyond `switch` / `clone` / `new-repo`.
