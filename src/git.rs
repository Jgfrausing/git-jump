use anyhow::{Context, Result, bail};
use std::ffi::OsStr;
use std::process::Command;

fn build<I, S>(args: I) -> Command
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut cmd = Command::new("git");
    cmd.args(args);
    cmd
}

pub fn run<I, S>(args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let status = build(args).status().context("failed to spawn git")?;
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
    let output = build(args).output().context("failed to spawn git")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git failed: {}", stderr.trim());
    }
    String::from_utf8(output.stdout).context("git output was not utf-8")
}

pub fn in_repo() -> bool {
    build(["rev-parse", "--git-dir"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
