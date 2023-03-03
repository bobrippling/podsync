use serde::{Deserialize, Serialize};
use sha256::digest;

#[derive(Debug, Deserialize, Serialize)]
#[derive(sqlx::Type)]
pub struct User {
    pub username: String,
    pub pwhash: String,
}

impl User {
    pub fn accept_password(&self, pass: &str) -> bool {
        let pwhash = digest(pass);
        self.pwhash == pwhash
    }
}
