#[derive(Debug)]
#[cfg_attr(feature = "backend-sql", derive(sqlx::Type))]
pub struct User {
    pub username: String,
    pub pwhash: String,
    pub session_id: Option<String>,
}
