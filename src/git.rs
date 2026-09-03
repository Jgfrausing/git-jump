use anyhow::{Context, Result, bail};
use std::ffi::OsStr;
use std::io::{self, Read, Write};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

fn build<I, S>(dir: Option<&Path>, args: I) -> Command
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut cmd = Command::new("git");
    if let Some(dir) = dir {
        cmd.arg("-C").arg(dir);
    }
    cmd.args(args);
    cmd
}

/// Runs git with every stream inherited. For commands whose stdout the user
/// should see as-is (merge can open an editor, so it needs the real tty).
pub fn run_tty_in<I, S>(dir: &Path, args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let status = build(Some(dir), args)
        .status()
        .context("failed to spawn git")?;
    if !status.success() {
        bail!("git exited with status {}", status);
    }
    Ok(())
}

/// Runs git with its stdout forwarded to our stderr. gj's own stdout is the
/// "move here" channel when GJ_CD_FILE is unset, so no git step may write to it.
pub fn run_in<I, S>(dir: &Path, args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut child = build(Some(dir), args)
        .stdout(Stdio::piped())
        .spawn()
        .context("failed to spawn git")?;
    if let Some(mut out) = child.stdout.take() {
        let mut buf = Vec::new();
        out.read_to_end(&mut buf).ok();
        io::stderr().write_all(&buf).ok();
    }
    let status = child.wait().context("failed to wait for git")?;
    if !status.success() {
        bail!("git exited with status {}", status);
    }
    Ok(())
}

pub fn capture<I, S>(args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    capture_cmd(build(None, args))
}

pub fn capture_in<I, S>(dir: &Path, args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    capture_cmd(build(Some(dir), args))
}

fn capture_cmd(mut cmd: Command) -> Result<String> {
    let output = cmd.output().context("failed to spawn git")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git failed: {}", stderr.trim());
    }
    String::from_utf8(output.stdout).context("git output was not utf-8")
}

/// True when git exits 0. Both streams discarded.
pub fn ok<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    ok_cmd(build(None, args))
}

pub fn ok_in<I, S>(dir: &Path, args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    ok_cmd(build(Some(dir), args))
}

fn ok_cmd(mut cmd: Command) -> bool {
    cmd.stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn in_repo() -> bool {
    ok(["rev-parse", "--git-dir"])
}

/// Replaces this process with `git <args>`. Only returns if exec itself fails.
pub fn exec<I, S>(args: I) -> !
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let err = build(None, args).exec();
    eprintln!("error: failed to exec git: {err}");
    std::process::exit(1);
}
