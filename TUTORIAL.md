# gj in ten minutes

A walkthrough of one feature branch, start to finish. It assumes gj is
installed and the zsh wrapper from the [README](./README.md#install) is in
place, so that a gj command can move your shell.

## 0. The mental model

The repo directory is main. It is always main. Every other branch gets its own
directory under `.worktrees/`, and "switching branches" means moving to that
directory. There is no stash step, because nothing you do on a branch ever
touches the checkout in the root.

```
~/code/lazer_trader/                 <- main, always
~/code/lazer_trader/.worktrees/
    feat-price-cap/                  <- branch feat/price-cap
    fix-ercot-timeout/               <- branch fix/ercot-timeout
```

`.worktrees/` is excluded from `git status`, and each directory is a full
checkout with its own build cache, so two branches can compile side by side.

## 1. Start a branch

```sh
cd ~/code/lazer_trader
gj b price cap
```

gj fetches, fast-forwards main in the root, and creates a branch named from
the words joined with dashes, `price-cap`, the same rule as the old `git b`
alias. It lives in `.worktrees/price-cap`, and your shell is now in there:

```
$ pwd
/Users/jfr/code/lazer_trader/.worktrees/price-cap
$ git this
price-cap
```

If you want a slash in the name, type it: `gj b feat/price-cap` gives branch
`feat/price-cap` in `.worktrees/feat-price-cap`. `gj co -b feat/price-cap`
does the same thing.

## 2. Work as usual

Everything that is not a branch operation is plain git, aliases included:

```sh
gj s                       # git status -sb
gj cp "cap prices at 3k"   # commit -am + push
gj publish
gj pr
```

You can type `git` here just as well. gj only matters for the commands that
move between branches.

## 3. Pull main into the branch

Main moved while you worked. Instead of `co main; fapp; co -; merge -`:

```sh
gj up
```

It fetches, fast-forwards main in the root, and runs `git merge main` right
here in the worktree. A conflict is left for you to resolve, as with any
merge.

## 4. Jump somewhere else

```sh
gj co main              # shell moves to the root
gj co fix/ercot-timeout # shell moves to .worktrees/fix-ercot-timeout,
                        # creating it if this is the first visit
gj co -                 # back where you were
gj                      # picker: main first, then branches, · wt marks
                        # the ones that already have a directory
```

`gj co ercot` with no branch of that name opens the picker with `ercot`
already typed as the filter. The old `co mian` typo lands in the picker too.

A remote-only branch works the same: `gj co colleague/thing` creates the
worktree and a tracking branch in one step.

## 5. Finish the branch

After the PR merges:

```sh
gj fapp
```

Fetch, prune, fast-forward main, and every branch whose upstream is gone is
deleted together with its directory. If your shell was standing in one of
them, it is moved back to the root.

For a branch that never got a PR:

```sh
gj branch -d price-cap     # refuses if unmerged or dirty
gj branch -D price-cap     # force, directory and all
```

## 6. When you used plain git and the root moved

You ran `git checkout feat/x` in the root out of habit. Nothing breaks. The
next `gj co main`, `gj b` or `gj adopt` moves `feat/x` into `.worktrees/` and
puts the root back on main, provided the root is clean. If it is dirty, gj
exits 4 and tells you to commit or stash there first.

## 7. Migrating an existing repo

Any repo works from the first command; there is nothing to initialise. A repo
that currently sits on a feature branch needs one `gj adopt` to move that
branch out of the root.

For a repo where you want the old in-place behaviour (a build tree, say):

```sh
git config gj.worktrees false
```

For a repo whose main branch gj guesses wrong (`gj wt main` shows it):

```sh
git config gj.main develop
```

## Cheat sheet

| Gesture | Command |
|---|---|
| new branch off fresh main | `gj b <words>` |
| new branch off the current branch | `gj b <words> --here` |
| go to a branch | `gj co <b>`, or `gj` for the picker |
| back to main | `gj co main` |
| previous directory | `gj co -` |
| main into this branch | `gj up` |
| clean up merged branches | `gj fapp` |
| delete one branch | `gj branch -d <b>` |
| where is everything | `gj wt ls` |
| anything else | `gj <git args>` |
