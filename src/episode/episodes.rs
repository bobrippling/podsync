use serde::{Deserialize, Serialize};

use crate::time::Timestamp;
use super::Episode;

#[derive(Debug, Deserialize, Serialize)]
pub struct Episodes {
    pub timestamp: Timestamp,
    pub actions: Vec<Episode>,
}
