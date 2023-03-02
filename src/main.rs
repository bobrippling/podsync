use std::sync::Arc;

use warp::{Filter, Reply};
use serde::{Deserialize, Serialize};

use sqlx::{migrate::MigrateDatabase, Sqlite, SqlitePool};

use tracing::{Level, info};
use tracing_subscriber::FmtSubscriber;

mod device;

mod subscription;
use subscription::SubscriptionChanges;

mod episode;
use episode::EpisodeChangeWithDevice;

mod podsync;
use podsync::PodSync;

static DB_URL: &str = "sqlite://pod.sql";

#[tokio::main]
async fn main() {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber).unwrap();

    match Sqlite::create_database(DB_URL).await {
        Ok(()) => {
            info!("Created database {}", DB_URL);
        }
        Err(e) => {
            let sqlx::Error::Database(db_err) = e else {
                panic!("error creating database: {e}");
            };

            panic!("sql db error: {db_err:?}");//.code()
        }
    }

    let db = SqlitePool::connect(DB_URL)
        .await
        .expect("DB connection");

    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .expect("migration");

    let podsync = Arc::new(PodSync::new(db));

    let login = warp::path!("api" / "2" / "auth" / String / "login.json")
        .and(warp::post())
        .and(warp::header("authorization"))
        .then({
            let podsync = Arc::clone(&podsync);
            move |username: String, auth: String| {
                let podsync = Arc::clone(&podsync);
                async move {
                    map_into_status(podsync.login(username, auth).await)
                }
            }
        });

    let devices = {
        let for_user = warp::path!("api" / "2" / "devices" / String)
            .and(warp::get())
            .then({
                let podsync = Arc::clone(&podsync);
                move |username_format: String| {
                    let podsync = Arc::clone(&podsync);
                    async move {
                        map_into_json(podsync.devices(username_format).await)
                    }
                }
            });

        let create = warp::path!("api" / "2" / "devices" / String / String)
            .and(warp::post())
            .and(warp::body::json()) // TODO: this may just be an empty string
            .then({
                let podsync = Arc::clone(&podsync);
                move |username, device_name, device| {
                    let podsync = Arc::clone(&podsync);
                    async move {
                        map_into_status(podsync.create_device(username, device_name, device).await)
                    }
                }
            });

        for_user.or(create)
    };

    let subscriptions = {
        let get = warp::path!("api" / "2" / "subscriptions" / String / String) // FIXME: merge this
                                                                               // with the below path (same for /episodes)
            .and(warp::get())
            .then({
                let podsync = Arc::clone(&podsync);
                move |username, deviceid_format| {
                    let podsync = Arc::clone(&podsync);
                    async move {
                        map_into_json(podsync.subscriptions(username, deviceid_format).await)
                    }
                }
            });

        let upload = warp::path!("api" / "2" / "subscriptions" / String / String)
            .and(warp::post())
            .and(warp::body::json())
            .then({
                let podsync = Arc::clone(&podsync);
                move |username, deviceid_format: String, changes: SubscriptionChanges| {
                    let podsync = Arc::clone(&podsync);
                    async move {
                        map_into_json(podsync.update_subscriptions(username, deviceid_format, changes).await)
                    }
                }
            });

        get.or(upload)
    };

    let episodes = {
        let get = warp::path!("api" / "2" / "episodes" / String)
            .and(warp::get())
            .and(warp::query())
            .then({
                let podsync = Arc::clone(&podsync);
                move |username_format: String, query: QuerySince| {
                    let podsync = Arc::clone(&podsync);
                    async move {
                        map_into_json(podsync.episodes(username_format, query).await)
                    }
                }
            });

        let upload = warp::path!("api" / "2" / "episodes" / String)
            .and(warp::post())
            .and(warp::body::json())
            .then({
                let podsync = Arc::clone(&podsync);
                move |username_format: String, body: Vec<EpisodeChangeWithDevice>| {
                    let podsync = Arc::clone(&podsync);
                    async move {
                        map_into_json(podsync.update_episodes(username_format, body).await)
                    }
                }
            });

        get.or(upload)
    };

    let routes = login
        .or(devices)
        .or(subscriptions)
        .or(episodes)
        .with(warp::trace::request());

    warp::serve(routes)
        .run(([0, 0, 0, 0], 8080))
        .await;
}

fn map_into_status(result: Result<(), warp::http::StatusCode>) -> impl warp::Reply {
    let status = match result {
        Ok(()) => warp::http::StatusCode::OK,
        Err(status) => status,
    };

    warp::reply::with_status(warp::reply(), status)
}

fn map_into_json<S>(result: Result<S, warp::http::StatusCode>) -> impl warp::Reply
where
    S: Serialize,
{
    match result {
        Ok(s) => warp::reply::json(&s)
            .into_response(),

        Err(status) => warp::reply::with_status(warp::reply(), status)
            .into_response()
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct QuerySince {
    since: u32,
}
