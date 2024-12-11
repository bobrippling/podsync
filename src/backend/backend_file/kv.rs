use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};

use log::error;

use super::{FindError, User};

pub type KeyValues = HashMap<String, String>;

pub fn read(input: impl Read, keys: &[&str]) -> Result<KeyValues, FindError> {
    let mut kv = HashMap::new();

    for line in BufReader::new(input).lines() {
        let line = line.map_err(|e| {
            error!("couldn't read line: {e}");
            FindError::Internal
        })?;

        let parts = line.split_once(':').ok_or_else(|| {
            error!("invalid line, can't split");
            FindError::Internal
        })?;

        kv.insert(parts.0.into(), parts.1.into());
    }

    Ok(kv)
}

pub fn write(
    mut output: impl Write,
    keyvalues: &HashMap<String, String>,
) -> Result<(), std::io::Error> {
    for (k, v) in keyvalues {
        writeln!(output, "{}: {}", k, v)?;
    }
    Ok(())
}
