#!/usr/bin/env bash
# Scripted checks for gj's worktree mode. Builds a bare remote and clones
# under a temp dir; never touches a real repo. Not wired into `cargo test`,
# run it by hand:  tests/wt.sh
set -u
cd "$(dirname "$0")/.." || exit 1
cargo build -q || exit 1
GJ=$PWD/target/debug/gj

T=$(mktemp -d -t gjtest.XXXXXX) || exit 1
T=$(cd "$T" && pwd -P)
trap 'rm -rf "$T"' EXIT

unset GJ_CD_FILE
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t
export GIT_PAGER=cat PAGER=cat GIT_EDITOR=true

pass=0 fail=0
ok()  { pass=$((pass + 1)); echo "ok   $1"; }
nok() { fail=$((fail + 1)); echo "FAIL $1"; }
skip() { echo "skip $1"; }
check() { local d=$1; shift; if "$@"; then ok "$d"; else nok "$d"; fi; }
eq() { [[ "$1" == "$2" ]] || { echo "     expected: $2"; echo "     got:      $1"; return 1; }; }
has() { [[ "$1" == *"$2"* ]] || { echo "     wanted substring: $2"; echo "     got: $1"; return 1; }; }
branch_of() { git -C "$1" symbolic-ref -q --short HEAD; }
# Renders a command under a pty and cancels the prompt, so the picker's output
# can be asserted. Escapes are stripped and \r split so `has` can match a row.
# BSD script takes the command as argv, GNU script needs -c. Both tcgetattr
# their own stdin, which is why the Escape arrives down a pipe: the suite's
# inherited stdin may be a socket, and BSD script exits 1 on one.
pty() {
  if script --version >/dev/null 2>&1; then
    printf '\033' | script -qec "$*" /dev/null 2>&1
  else
    printf '\033' | script -q /dev/null "$@" 2>&1
  fi | tr '\r' '\n' | sed -e $'s/\x1b\\[[0-9;?]*[a-zA-Z]//g'
}
wt_count() { git -C "$1" worktree list --porcelain | grep -c '^worktree '; }

# ---- fixture: seed -> bare remote -> clone -------------------------------
cd "$T"
git init -q -b main seed
( cd seed && echo hi >README && git add README && git commit -q -m init )
git clone -q --bare seed remote.git
git clone -q remote.git repo 2>/dev/null
R=$T/repo
git -C "$R" config alias.s 'status -sb'
git -C "$R" config alias.co checkout
git -C "$T/remote.git" config alias.co checkout
W=$R/.worktrees

# ---- 1. gj b -------------------------------------------------------------
cd "$R"
out=$("$GJ" b my new thing 2>/dev/null); rc=$?
check "1 gj b prints the worktree path" eq "$out" "$W/my-new-thing"
check "1 exit 0" eq "$rc" 0
check "1 exclude entry present" grep -qx '.worktrees/' "$R/.git/info/exclude"
check "1 root on main" eq "$(branch_of "$R")" main
check "1 root clean" eq "$(git -C "$R" status --porcelain)" ""
check "1 worktree on the branch" eq "$(branch_of "$W/my-new-thing")" my-new-thing
check "1 new branch has no upstream" bash -c "! git -C '$W/my-new-thing' rev-parse -q --verify @{u} >/dev/null 2>&1"

# ---- 2. co -b from a worktree, default base vs --here --------------------
cd "$W/my-new-thing"
echo a >a && git add a && git commit -q -m a
main_sha=$(git -C "$R" rev-parse main)
here_sha=$(git rev-parse HEAD)
out=$("$GJ" co -b feat/b 2>/dev/null)
check "2 co -b prints slug path" eq "$out" "$W/feat-b"
check "2 default base is main" eq "$(git -C "$W/feat-b" rev-parse HEAD)" "$main_sha"
out=$("$GJ" co -b feat/c --here 2>/dev/null)
check "2 --here bases on current HEAD" eq "$(git -C "$W/feat-c" rev-parse HEAD)" "$here_sha"
out=$("$GJ" switch -c feat/d "$here_sha" 2>/dev/null)
check "2 switch -c with explicit start" eq "$(git -C "$W/feat-d" rev-parse HEAD)" "$here_sha"

# ---- 3. co existing branch from another worktree ------------------------
"$GJ" b feat/a >/dev/null 2>&1
cd "$W/feat-b"
n=$(wt_count "$R")
out=$("$GJ" co feat/a 2>/dev/null)
check "3 co existing prints its path" eq "$out" "$W/feat-a"
check "3 worktree count unchanged" eq "$(wt_count "$R")" "$n"

# ---- 4. remote-only branch ------------------------------------------------
cd "$R"
git branch -q remote-only main
git push -q -u origin remote-only 2>/dev/null
git branch -q -D remote-only
out=$("$GJ" switch remote-only 2>/dev/null)
check "4 remote-only creates worktree" eq "$out" "$W/remote-only"
check "4 tracks origin" eq "$(git -C "$W/remote-only" rev-parse --abbrev-ref '@{u}' 2>/dev/null)" origin/remote-only

# ---- 5. co main from a worktree -------------------------------------------
cd "$W/feat-a"
out=$("$GJ" co main 2>/dev/null)
check "5 co main prints root" eq "$out" "$R"

# ---- 6. pass-through ------------------------------------------------------
cd "$W/feat-a"
same() {
  local d=$1; shift
  local g_out g_rc j_out j_rc
  g_out=$(git "$@" 2>/dev/null); g_rc=$?
  ${RESET:-:}
  j_out=$("$GJ" "$@" 2>/dev/null); j_rc=$?
  ${RESET:-:}
  if [[ "$g_out" == "$j_out" && $g_rc == "$j_rc" ]]; then ok "6 pass-through: $d"; else
    nok "6 pass-through: $d (git rc=$g_rc gj rc=$j_rc)"; echo "     git: $g_out"; echo "     gj:  $j_out"; fi
}
same "alias s" s
same "log -1" log -1
same "co <file>" co README
same "checkout origin/main <file>" checkout origin/main README
same "branch" branch
RESET="git reset -q" same "rm --cached" rm --cached README
same "worktree list" worktree list
sha=$(git rev-parse HEAD)
same "co <sha>" co "$sha"
check "6 co <sha> left worktree detached (git did it)" eq "$(branch_of "$W/feat-a" || echo detached)" detached
out=$("$GJ" log -1 --format=%H 2>/dev/null)
check "6 stdout not swallowed" eq "$out" "$sha"

# ---- 7. co notabranch without a tty --------------------------------------
g=$(git co notabranch 2>&1 </dev/null); g_rc=$?
j=$("$GJ" co notabranch 2>&1 </dev/null); j_rc=$?
check "7 no tty: git's own error" eq "$j" "$g"
check "7 no tty: git's exit code" eq "$j_rc" "$g_rc"

# ---- 8. repair a detached slug worktree ----------------------------------
cd "$R"
out=$("$GJ" co feat/a 2>/dev/null)
check "8 repaired path printed" eq "$out" "$W/feat-a"
check "8 worktree back on branch" eq "$(branch_of "$W/feat-a")" feat/a

# ---- 9. branch -D from inside the worktree -------------------------------
cd "$W/feat-b"
out=$("$GJ" branch -D feat/b 2>/dev/null); rc=$?
check "9 prints root when cwd removed" eq "$out" "$R"
cd "$R"
check "9 dir gone" bash -c "[[ ! -e '$W/feat-b' ]]"
check "9 branch gone" bash -c "! git -C '$R' show-ref -q --verify refs/heads/feat/b"

# ---- 10. branch -d unmerged -----------------------------------------------
cd "$R"
"$GJ" b unmerged >/dev/null 2>&1
( cd "$W/unmerged" && echo u >u && git add u && git commit -q -m u )
err=$("$GJ" branch -d unmerged 2>&1 >/dev/null); rc=$?
check "10 -d refused, exit 1" eq "$rc" 1
check "10 git's not fully merged message" has "$err" "not fully merged"
check "10 worktree still present on branch" eq "$(branch_of "$W/unmerged")" unmerged
out=$("$GJ" branch -D unmerged 2>/dev/null); rc=$?
check "10 -D removes it" bash -c "[[ ! -e '$W/unmerged' ]] && ! git -C '$R' show-ref -q --verify refs/heads/unmerged"
check "10 -D from root prints nothing" eq "$out" ""

# ---- 11. fapp removes gone branches ---------------------------------------
cd "$R"
"$GJ" b gone >/dev/null 2>&1
git -C "$W/gone" push -q -u origin gone 2>/dev/null
git push -q origin -d gone 2>/dev/null
cd "$W/gone"
out=$("$GJ" fapp 2>/dev/null); rc=$?
check "11 fapp from inside gone worktree prints root" eq "$out" "$R"
cd "$R"
check "11 gone worktree removed" bash -c "[[ ! -e '$W/gone' ]]"
check "11 gone branch removed" bash -c "! git -C '$R' show-ref -q --verify refs/heads/gone"
cd "$R"
"$GJ" b gone2 >/dev/null 2>&1
git -C "$W/gone2" push -q -u origin gone2 2>/dev/null
git push -q origin -d gone2 2>/dev/null
out=$("$GJ" fapp 2>/dev/null); rc=$?
check "11 fapp from root prints nothing" eq "$out" ""
check "11 exit 0" eq "$rc" 0
check "11 gone2 removed" bash -c "[[ ! -e '$W/gone2' ]]"

# ---- 12. up ---------------------------------------------------------------
cd "$R"
"$GJ" b upb >/dev/null 2>&1
git commit -q --allow-empty -m "main moves"
git push -q origin main 2>/dev/null
cd "$W/upb"
"$GJ" up >/dev/null 2>&1; rc=$?
check "12 up exit 0" eq "$rc" 0
check "12 main merged into worktree" git merge-base --is-ancestor main HEAD
cd "$R"
err=$("$GJ" up 2>&1 >/dev/null); rc=$?
check "12 up in root is an error" eq "$rc" 1
check "12 up in root says already on main" has "$err" "already on 'main'"

# ---- 13. adopt via co main -----------------------------------------------
cd "$T"
git clone -q remote.git repo2 2>/dev/null
cd repo2
git config alias.co checkout
git switch -q -c pre
echo p >p && git add p && git commit -q -m p
out=$("$GJ" co main 2>/dev/null); rc=$?
check "13 co main from root on pre prints root" eq "$out" "$T/repo2"
check "13 pre moved to worktree" eq "$(branch_of "$T/repo2/.worktrees/pre")" pre
check "13 root on main" eq "$(branch_of "$T/repo2")" main
cd "$T"
git clone -q remote.git repo3 2>/dev/null
cd repo3
git switch -q -c pre
touch x
out=$("$GJ" co main 2>/dev/null); rc=$?
check "13 dirty root: exit 4" eq "$rc" 4
check "13 dirty root: nothing moved" bash -c "[[ '$(branch_of "$T/repo3")' == pre && ! -e '$T/repo3/.worktrees' ]]"
err=$("$GJ" adopt 2>&1 >/dev/null); rc=$?
check "13 adopt on dirty root: exit 4" eq "$rc" 4
check "13 adopt message" has "$err" "commit or stash there first"

# ---- 14. gj.worktrees false -------------------------------------------------
cd "$T/repo2"
git config gj.worktrees false
git branch -q feat/a main
out=$("$GJ" co feat/a 2>/dev/null); rc=$?
check "14 disabled: nothing printed" eq "$out" ""
check "14 disabled: in-place switch" eq "$(branch_of "$T/repo2")" feat/a
git switch -q main
git config --unset gj.worktrees

# ---- 15. main detection -----------------------------------------------------
cd "$T"
git init -q --initial-branch=master m1 && ( cd m1 && git commit -q --allow-empty -m i )
check "15 master fallback" eq "$(cd m1 && "$GJ" wt main 2>/dev/null)" master
git init -q --initial-branch=main m2 && ( cd m2 && git commit -q --allow-empty -m i )
check "15 local main without origin/HEAD" eq "$(cd m2 && "$GJ" wt main 2>/dev/null)" main
( cd m2 && git config gj.main other )
check "15 gj.main wins" eq "$(cd m2 && "$GJ" wt main 2>/dev/null)" other
check "15 origin/HEAD" eq "$(cd "$R" && "$GJ" wt main 2>/dev/null)" main

# ---- 16. GJ_CD_FILE ---------------------------------------------------------
cd "$R"
out=$(GJ_CD_FILE="$T/cd" "$GJ" co feat/a 2>/dev/null)
check "16 stdout empty with GJ_CD_FILE" eq "$out" ""
check "16 path in the file" eq "$(cat "$T/cd")" "$W/feat-a"

# ---- 17. picker markers -----------------------------------------------------
cd "$R"
out=$(pty "$GJ")
if [[ "$out" != *"? checkout"* ]]; then
  skip "17 picker markers (no pty: $(head -1 <<<"$out"))"
else
  check "17 root worktree marked" has "$out" "main  · root"
  check "17 linked worktree marked" has "$out" "feat/a  · wt"
  check "17 branch without a worktree is bare" bash -c "! grep -qE 'feat/b +·' <<<\"$out\""
  out=$(cd "$T/repo3" && pty "$GJ")
  check "17 root off main marked with its branch" has "$out" "pre  · root"
  check "17 main unmarked when it has no worktree" bash -c "! grep -qE 'main +·' <<<\"$out\""
  # git puts one branch in several trees with --force and
  # --ignore-other-worktrees, so the markers are a tally, not one of three
  # states.
  git worktree add -q --force "$W/dup" main
  out=$(pty "$GJ")
  check "17 branch in root and a worktree shows both" has "$out" "main  · root  · wt"
  git worktree add -q --force "$W/dup2" main
  out=$(pty "$GJ")
  check "17 several linked worktrees are counted" has "$out" "main  · root  · wt ×2"
  git worktree remove --force "$W/dup"
  git worktree remove --force "$W/dup2"
fi

# ---- extras -----------------------------------------------------------------
cd "$R"
out=$("$GJ" --help 2>/dev/null | head -1); rc=${PIPESTATUS[0]}
check "x --help exits 0" eq "$rc" 0
check "x --help prints usage" eq "$out" "gj: git, with one worktree per branch"
same "help <topic> goes to git" help --all
"$GJ" b 2>/dev/null; rc=$?
check "x gj b without words exits 2" eq "$rc" 2
"$GJ" b 'bad..name' 2>/dev/null; rc=$?
check "x invalid branch name exits 2" eq "$rc" 2
"$GJ" branch -d main 2>/dev/null; rc=$?
check "x refuses to delete main" eq "$rc" 1
out=$("$GJ" wt path feat/a 2>/dev/null)
check "x wt path" eq "$out" "$W/feat-a"
out=$("$GJ" wt root 2>/dev/null)
check "x wt root" eq "$out" "$R"
out=$("$GJ" wt ls 2>/dev/null)
check "x wt ls lists root" has "$out" "$R  main  · root"
check "x wt ls lists a worktree" has "$out" "$W/feat-a  feat/a"
out=$("$GJ" wt ls --tsv 2>/dev/null | head -1)
check "x wt ls --tsv root first" eq "$out" "$(printf '%s\tmain' "$R")"
check "x wt ls --tsv has a worktree line" has "$("$GJ" wt ls --tsv 2>/dev/null)" "$(printf '%s\tfeat/a' "$W/feat-a")"
"$GJ" wt rm feat/c 2>/dev/null; rc=$?
check "x wt rm refuses unmerged without -f" bash -c "[[ $rc == 1 && -e '$W/feat-c' ]]"
"$GJ" wt rm feat/c -f 2>/dev/null; rc=$?
check "x wt rm -f" bash -c "[[ $rc == 0 && ! -e '$W/feat-c' ]]"
cd "$W/feat-a" && echo dirty >dirty
"$GJ" branch -d feat/a 2>/dev/null; rc=$?
check "x -d on dirty worktree exits 4" eq "$rc" 4
rm dirty

echo
echo "passed $pass, failed $fail"
[[ $fail == 0 ]]
