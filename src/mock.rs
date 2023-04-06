use sqlx::{migrate::MigrateDatabase, Pool, Sqlite, SqlitePool};

pub async fn create_db() -> Pool<Sqlite> {
    let url = ":memory:";

    Sqlite::create_database(url).await.unwrap();

    let db = SqlitePool::connect(url).await.unwrap();

    sqlx::migrate!("./migrations").run(&db).await.unwrap();

    db
}
