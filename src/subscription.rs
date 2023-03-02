use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Subscription {
    url: String,
    title: String,
    author: String,
    description: String,
    subscribers: u32,
    logo_url: String,
    scaled_logo_url: String,
    website: String,
    mygpo_link: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SubscriptionChanges {
    pub add: Vec<String>, // TODO: make these &str?
    pub remove: Vec<String>,
    pub timestamp: Option<u32>,
}

impl SubscriptionChanges {
    pub fn empty() -> Self {
        Self {
            add: vec![],
            remove: vec![],
            timestamp: Some(0),
        }
    }
}
