use std::env::{self, VarError};

#[allow(unreachable_code)]
fn main() {
    #[cfg(not(feature = "backend-sql"))]
    return;

    let use_db = |_url| {
        // println!("cargo:rustc-env=DATABASE_URL={}", url);
        println!("cargo:rerun-if-changed=migrations");
    };

    let use_json = || println!("cargo:rustc-env=SQLX_OFFLINE=true");

    println!("cargo:rerun-if-env-changed=DATABASE_URL");
    match env::var("DATABASE_URL") {
        Ok(url) => use_db(url),
        Err(VarError::NotPresent) => {
            println!("cargo:warning={}", "using `sqlx-data.json` for schema");
            use_json()
        }
        Err(e) => panic!("$DATABASE_URL: {e:?}"),
    }
}
