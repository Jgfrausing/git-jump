use std::fmt;

/// An error that maps to a specific exit code. Everything else exits 1.
#[derive(Debug)]
pub struct Exit {
    pub code: i32,
    pub msg: String,
}

impl fmt::Display for Exit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.msg)
    }
}

impl std::error::Error for Exit {}

pub const USAGE: i32 = 2;
pub const REFUSED: i32 = 4;
pub const CANCELLED: i32 = 130;

pub fn usage(msg: impl Into<String>) -> anyhow::Error {
    Exit { code: USAGE, msg: msg.into() }.into()
}

pub fn refused(msg: impl Into<String>) -> anyhow::Error {
    Exit { code: REFUSED, msg: msg.into() }.into()
}
