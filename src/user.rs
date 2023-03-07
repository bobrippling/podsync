#[derive(Debug)]
#[derive(sqlx::Type)]
pub struct User {
    pub username: String,
    pub pwhash: String,
    pub session_id: Option<String>,
}
