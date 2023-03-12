use crate::podsync::{Error, Result};

pub fn split_format_json(s: &str) -> Result<&str> {
    let (a, b) = s.split_once('.').ok_or(Error::BadRequest)?;

    err_unless_json(b)?;

    Ok(a)
}

fn err_unless_json(s: &str) -> Result<()> {
    (s == "json").then_some(()).ok_or(Error::BadRequest)
}
