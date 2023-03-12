use serde::{Deserialize, Serialize};

use crate::time::Timestamp;

#[derive(Debug, Serialize)]
pub struct SubscriptionChangesToClient {
    pub add: Vec<String>,
    pub remove: Vec<String>,
    pub timestamp: Timestamp,
}

#[derive(Debug, Deserialize)]
pub struct SubscriptionChangesFromClient {
    pub add: Vec<String>,
    pub remove: Vec<String>,
}
