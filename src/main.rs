use std::sync::Arc;

use warp::{Filter, Reply};
use serde::{Deserialize, Serialize};

use sqlx::{migrate::MigrateDatabase, Sqlite, SqlitePool, query, query_as};

use tracing::{Level, info, error};
use tracing_subscriber::FmtSubscriber;

mod device;
use device::{Device, DeviceType, DeviceCreate};

mod subscription;
use subscription::SubscriptionChanges;

mod episode;
use episode::{EpisodeChanges, EpisodeChangeWithDevice, EpisodeChangeRaw};

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

    let db = Arc::new(db);

    let login = warp::path!("api" / "2" / "auth" / String / "login.json")
        .and(warp::post())
        .and(warp::header("authorization"))
        .map(|username, auth: String| {
            eprintln!("todo: auth or {username}: {auth}");

            warp::reply::with_status(
                warp::reply(),
                warp::http::StatusCode::OK // UNAUTHORIZED
            )
        });

    let devices = {
        let for_user = warp::path!("api" / "2" / "devices" / String)
            .and(warp::get())
            .then({
                let db = Arc::clone(&db);
                move |username_format: String| {
                    let db = Arc::clone(&db);

                    async move {
                        // let (username, format) = split_format(username_format)?; // FIXME: ? -> return 40?
                        let (username, format) = match username_format.split_once('.') {
                            Some(tup) => tup,
                            None => return warp::reply::with_status(
                                warp::reply(),
                                warp::http::StatusCode::BAD_REQUEST
                                ).into_response(),
                        };

                        if format != "json" {
                            return warp::reply::with_status(
                                warp::reply(),
                                warp::http::StatusCode::UNPROCESSABLE_ENTITY,
                                ).into_response();
                        }

                        let query = query_as!(
                            Device,
                            r#"
                            SELECT id, caption, type as "type: _", subscriptions, username
                            FROM devices
                            WHERE username = ?
                            "#,
                            username,
                        )
                            .fetch_all(&*db)
                            .await;

                        let devices = match query {
                            Ok(d) => d,
                            Err(e) => {
                                error!("select error: {:?}", e);

                                return warp::reply::with_status(
                                    warp::reply(),
                                    warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                                ).into_response();
                            }
                        };

                        warp::reply::json(&devices).into_response()
                    }
                }
            });

        let create = warp::path!("api" / "2" / "devices" / String / String)
            .and(warp::post())
            .and(warp::body::json()) // TODO: this may just be an empty string
            .then({
                let db = Arc::clone(&db);
                move |username, device_name, new_device: DeviceCreate| {
                    let db = Arc::clone(&db);
                    async move {
                        // device_name is device id
                        // FIXME: use device_name
                        println!("got device creation {device_name} for {username}: {new_device:?}");

                        let caption = new_device.caption.as_deref().unwrap_or("");
                        let r#type = new_device.r#type.unwrap_or(DeviceType::Unknown);

                        let query = query!(
                            "INSERT INTO devices
                            (caption, type, username, subscriptions)
                            VALUES
                            (?, ?, ?, ?)",
                            caption,
                            r#type,
                            username,
                            0,
                        )
                            .execute(&*db)
                            .await;

                        match query {
                            Ok(_) => warp::reply().into_response(),
                            Err(e) => {
                                // FIXME: handle EEXIST (and others?)
                                error!("insert error: {:?}", e);

                                warp::reply::with_status(
                                    warp::reply(),
                                    warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                                    ).into_response()
                            }
                        }
                    }
                }
            });

        for_user.or(create)
    };

    let subscriptions = {
        let get = warp::path!("api" / "2" / "subscriptions" / String / String) // FIXME: merge this
                                                                               // with the below path (same for /episodes)
            .and(warp::get())
            .map(|username, deviceid_format| {
                println!("got subscriptions for {deviceid_format} for {username}");

                warp::reply::json(&SubscriptionChanges::empty())
            });

        let upload = warp::path!("api" / "2" / "subscriptions" / String / String)
            .and(warp::post())
            .and(warp::body::json())
            .then({
                let db = Arc::clone(&db);
                move |username, deviceid_format: String, sub_changes: SubscriptionChanges| {
                    let db = Arc::clone(&db);

                    async move {
                        let (device_id, _format /*FIXME: check*/) = match deviceid_format.split_once('.') {
                            Some(tup) => tup,
                            None => return warp::reply::with_status(
                                warp::reply(),
                                warp::http::StatusCode::BAD_REQUEST
                            ).into_response(),
                        };

                        println!("got urls for {username}'s device {device_id}, timestamp {:?}:", sub_changes.timestamp);

                        let mut tx = match db.begin().await {
                            Ok(tx) => tx,
                            Err(e) => {
                                error!("transaction begin: {:?}", e);

                                return warp::reply::with_status(
                                    warp::reply(),
                                    warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                                    ).into_response()
                            }
                        };

                        for url in &sub_changes.remove {
                            let query = query!(
                                "DELETE FROM subscriptions WHERE url = ?",
                                url
                            )
                                .execute(&mut tx)
                                .await;

                            if let Err(e) = query {
                                error!("transaction addition: {:?}", e);

                                return warp::reply::with_status(
                                    warp::reply(),
                                    warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                                ).into_response();
                            }
                        }

                        for url in &sub_changes.add {
                            let query = query!(
                                "
                                INSERT INTO subscriptions
                                (url, username, device)
                                VALUES
                                (?, ?, ?)
                                ON CONFLICT (url, username, device)
                                DO NOTHING
                                ",
                                url,
                                username,
                                device_id,
                            )
                                .execute(&mut tx)
                                .await;

                            if let Err(e) = query {
                                error!("transaction addition: {:?}", e);

                                return warp::reply::with_status(
                                    warp::reply(),
                                    warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                                ).into_response();
                            }
                        }

                        match tx.commit().await {
                            Ok(()) => {
                                warp::reply::json(
                                    &UpdatedUrls {
                                        timestamp: 0, // TODO
                                        update_urls: vec![], // unused by client
                                    })
                                .into_response()
                            }
                            Err(e) => {
                                error!("transaction commit: {:?}", e);

                                warp::reply::with_status(
                                    warp::reply(),
                                    warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                                ).into_response()
                            }
                        }
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
                let db = Arc::clone(&db);

                move |username_format: String, query: QuerySince| {
                    let _db = Arc::clone(&db);

                    async move {
                        // podcast, episode, action, timestamp?, guid?, ...{started,position,total}?
                        println!("episodes GET for {username_format} since {query:?}");

                        let (_username, format) = match username_format.split_once('.') {
                            Some(tup) => tup,
                            None => return warp::reply::with_status(
                                warp::reply(),
                                warp::http::StatusCode::BAD_REQUEST
                            ).into_response(),
                        };

                        if format != "json" {
                            return warp::reply::with_status(
                                warp::reply(),
                                warp::http::StatusCode::UNPROCESSABLE_ENTITY,
                                ).into_response();
                        }

                        // let query = query_as!(
                        //     EpisodeChangeRaw,
                        //     r#"
                        //     SELECT podcast, episode, timestamp, guid, action, started, position, total
                        //     FROM episodes
                        //     WHERE username = ?
                        //     "#,
                        //     username,
                        // )
                        //     .fetch_all(&*db)
                        //     .await;

                        warp::reply::json(&EpisodeChanges::empty_at(0)).into_response()
                    }
                }
            });

        let upload = warp::path!("api" / "2" / "episodes" / String)
            .and(warp::post())
            .and(warp::body::json())
            .then({
                let db = Arc::clone(&db);
                move |username_format: String, body: Vec<EpisodeChangeWithDevice>| {
                    let db = Arc::clone(&db);

                    async move {
                        // podcast, episode, guid: optional
                        println!("episodes POST for {username_format}");

                        let (username, format) = match username_format.split_once('.') {
                            Some(tup) => tup,
                            None => return warp::reply::with_status(
                                warp::reply(),
                                warp::http::StatusCode::BAD_REQUEST
                            ).into_response(),
                        };

                        if format != "json" {
                            return warp::reply::with_status(
                                warp::reply(),
                                warp::http::StatusCode::UNPROCESSABLE_ENTITY,
                                ).into_response();
                        }

                        let mut tx = match db.begin().await {
                            Ok(tx) => tx,
                            Err(e) => {
                                error!("transaction begin: {:?}", e);

                                return warp::reply::with_status(
                                    warp::reply(),
                                    warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                                    ).into_response()
                            }
                        };

                        for EpisodeChangeWithDevice { change, device } in body {
                            println!("updating: {change:?} device={device}");

                            let EpisodeChangeRaw {
                                podcast, episode, timestamp, guid,
                                action, started, position, total,
                            } = change.into();

                            let query = query!(
                                "
                                INSERT INTO episodes
                                (
                                    username, device, podcast,
                                    episode, timestamp, guid,
                                    action,
                                    started, position, total
                                )
                                VALUES
                                (
                                    ?, ?, ?,
                                    ?, ?, ?,
                                    ?,
                                    ?, ?, ?
                                )
                                ON CONFLICT (username, device, podcast, episode)
                                DO
                                    UPDATE SET
                                        timestamp = ?,
                                        guid = ?,
                                        action = ?,
                                        started = ?,
                                        position = ?,
                                        total = ?
                                ",
                                username, device, podcast,
                                episode, timestamp, guid,
                                action,
                                started, position, total,
                                //
                                timestamp, guid,
                                action,
                                started, position, total
                            )
                                .execute(&mut tx)
                                .await;

                            if let Err(e) = query {
                                error!("transaction addition: {:?}", e);

                                return warp::reply::with_status(
                                    warp::reply(),
                                    warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                                ).into_response();
                            }
                        }

                        match tx.commit().await {
                            Ok(()) => {
                                warp::reply::json(
                                    &UpdatedUrls { // FIXME: rename struct
                                        timestamp: 0, // FIXME: timestamping
                                        update_urls: vec![], // unused by client
                                    }
                                ).into_response()
                            }
                            Err(e) => {
                                error!("transaction commit: {:?}", e);

                                warp::reply::with_status(
                                    warp::reply(),
                                    warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                                ).into_response()
                            }
                        }
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

#[derive(Debug, Deserialize, Serialize)]
struct QuerySince {
    since: u32,
}

#[derive(Debug, Deserialize, Serialize)]
struct UpdatedUrls {
    timestamp: u32,
    update_urls: Vec<[String; 2]>,
}
