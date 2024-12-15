use crate::podsync::{Error, Result};
use log::error;

pub fn split_format_json(s: &str) -> Result<&str> {
    let (a, b) = s.split_once('.').ok_or(Error::BadRequest).map_err(|e| {
        error!("couldn't split json {s:?} on '.'");
        e
    })?;

    err_unless_json(b).map_err(|e| {
        error!("\"json\" not found in {b:?}");
        e
    })?;

    Ok(a)
}

fn err_unless_json(s: &str) -> Result<()> {
    (s == "json").then_some(()).ok_or(Error::BadRequest)
}
