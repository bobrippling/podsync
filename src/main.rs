use std::{
    sync::Arc,
    future::Future,
};

use warp::{Filter, Reply};
use serde::{Deserialize, Serialize};

use sqlx::{migrate::MigrateDatabase, Sqlite, SqlitePool};

use tracing::{Level, info};
use tracing_subscriber::FmtSubscriber;

mod auth;
use auth::Auth;

mod user;

mod device;

mod subscription;
use subscription::SubscriptionChanges;

mod episode;
use episode::EpisodeChangeWithDevice;

mod podsync;
use podsync::PodSync;

mod path_format;
use path_format::split_format_json;

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

    let login =
        warp::post()
        .and(warp::path!("api" / "2" / "auth" / String / "login.json"))
        .and(warp::header("authorization"))
        .then({
            let podsync = Arc::clone(&podsync);
            move |username: String, auth: Auth| {
                let podsync = Arc::clone(&podsync);

                result_to_response(async move {
                    let credentials = auth.with_user(username)?;

                    podsync.authenticate(&credentials)
                        .await?
                        .login()
                        .await
                })
            }
        });

    let devices = {
        let for_user = warp::path!("api" / "2" / "devices" / String)
            .and(warp::get())
            .and(warp::header("authorization"))
            .then({
                let podsync = Arc::clone(&podsync);
                move |username_format: String, auth: Auth| {
                    let podsync = Arc::clone(&podsync);

                    result_to_response(async move {
                        let username = split_format_json(&username_format)?;
                        let credentials = auth.with_user(username.to_string())?;

                        podsync.authenticate(&credentials)
                            .await?
                            .devices(username_format)
                            .await
                    })
                }
            });

        let create = warp::path!("api" / "2" / "devices" / String / String)
            .and(warp::post())
            .and(warp::header("authorization"))
            .and(warp::body::json()) // TODO: this may just be an empty string
            .then({
                let podsync = Arc::clone(&podsync);
                move |username, device_name, auth: Auth, device| {
                    let podsync = Arc::clone(&podsync);

                    result_to_response(async move {
                        let credentials = auth.with_user(username)?;

                        podsync.authenticate(&credentials)
                            .await?
                            .create_device(device_name, device)
                            .await
                    })
                }
            });

        for_user.or(create)
    };

    let subscriptions = {
        let get = warp::path!("api" / "2" / "subscriptions" / String / String)
            // FIXME: merge this ^
            // with the below path (same for /episodes)
            .and(warp::get())
            .and(warp::header("authorization"))
            .then({
                let podsync = Arc::clone(&podsync);
                move |username, deviceid_format: String, auth: Auth| {
                    let podsync = Arc::clone(&podsync);

                    result_to_response(async move {
                        let device_id = split_format_json(&deviceid_format)?;
                        let credentials = auth.with_user(username)?;

                        podsync.authenticate(&credentials)
                            .await?
                            .subscriptions(device_id)
                            .await
                    })
                }
            });

        let upload = warp::path!("api" / "2" / "subscriptions" / String / String)
            .and(warp::post())
            .and(warp::header("authorization"))
            .and(warp::body::json())
            .then({
                let podsync = Arc::clone(&podsync);
                move |username, deviceid_format: String, auth: Auth, changes: SubscriptionChanges| {
                    let podsync = Arc::clone(&podsync);

                    result_to_response(async move {
                        let credentials = auth.with_user(username)?;

                        podsync.authenticate(&credentials)
                            .await?
                            .update_subscriptions(deviceid_format, changes)
                            .await
                    })
                }
            });

        get.or(upload)
    };

    let episodes = {
        let get = warp::path!("api" / "2" / "episodes" / String)
            .and(warp::get())
            .and(warp::query())
            .and(warp::header("authorization"))
            .then({
                let podsync = Arc::clone(&podsync);
                move |username_format: String, query: QuerySince, auth: Auth| {
                    let podsync = Arc::clone(&podsync);

                    result_to_response(async move {
                        let username = split_format_json(&username_format)?;
                        let credentials = auth.with_user(username.to_string())?;

                        podsync.authenticate(&credentials)
                            .await?
                            .episodes(query)
                            .await
                    })
                }
            });

        let upload = warp::path!("api" / "2" / "episodes" / String)
            .and(warp::post())
            .and(warp::header("authorization"))
            .and(warp::body::json())
            .then({
                let podsync = Arc::clone(&podsync);
                move |username_format: String, auth: Auth, body: Vec<EpisodeChangeWithDevice>| {
                    let podsync = Arc::clone(&podsync);

                    result_to_response(async move {
                        let username = split_format_json(&username_format)?;
                        let credentials = auth.with_user(username.to_string())?;

                        podsync.authenticate(&credentials)
                            .await?
                            .update_episodes(body)
                            .await
                    })
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

async fn result_to_response<F, S>(f: F) -> impl warp::Reply
where
    F: Future<Output = podsync::Result<S>>,
    S: Serialize,
{
    match f.await {
        Ok(s) => warp::reply::json(&s)
            .into_response(),
        Err(e) => warp::reply::with_status(warp::reply(), e.into())
            .into_response(),
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct QuerySince {
    since: u32,
}
