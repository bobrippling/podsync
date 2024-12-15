#[derive(Debug)]
pub enum FindError {
    NotFound,
    Internal,
}

#[cfg(feature = "backend-sql")]
mod backend_sql;
#[cfg(feature = "backend-sql")]
pub use backend_sql::*;

#[cfg(not(feature = "backend-sql"))]
mod backend_file;
#[cfg(not(feature = "backend-sql"))]
pub use backend_file::*;
