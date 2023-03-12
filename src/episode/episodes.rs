use serde::{Deserialize, Serialize};

use super::Episode;
use crate::time::Timestamp;

#[derive(Debug, Deserialize, Serialize)]
pub struct Episodes {
    pub timestamp: Timestamp,
    pub actions: Vec<Episode>,
}
