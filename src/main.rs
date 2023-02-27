use warp::{Filter, Reply};
use serde::{Deserialize, Serialize};

use tracing::{Level, info, error};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber).unwrap();

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
            .map(|username_format: String| {
                println!("get devices for {username_format}");

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

                let devices: [Device; 1] = [
                    Device {
                        id: "test".into(),
                        caption: "test".into(),
                        r#type: DeviceType::Mobile,
                        subscriptions: 1,
                    },
                ];

                warp::reply::json(&devices).into_response()
            });

        let create = warp::path!("api" / "2" / "devices" / String / String)
            .and(warp::post())
            .and(warp::body::json())
            .map(|username, device_name, device: DeviceCreate| {
                // device_name is device id
                println!("got device creation {device_name} for {username}: {device:?}");

                warp::reply()
            });

        for_user.or(create)
    };

    let subscriptions = {
        let get = warp::path!("api" / "2" / "subscriptions" / String / String)
            .and(warp::get())
            .map(|username, deviceid_format| {
                println!("got subscriptions for {deviceid_format} for {username}");

                warp::reply::json(&SubscriptionChanges {
                    add: vec![],
                    remove: vec![],
                    timestamp: Some(0),
                })
            });

        let upload = warp::path!("api" / "2" / "subscriptions" / String / String)
            .and(warp::post())
            .and(warp::body::json())
            .map(|username, deviceid_format, sub_changes: SubscriptionChanges| {
                println!("got urls for {username}'s device {deviceid_format}, timestamp {:?}:", sub_changes.timestamp);

                // println!("add:");
                // for url in &sub_changes.add {
                //     println!("  {url}");
                // }
                // println!("remove:");
                // for url in &sub_changes.remove {
                //     println!("  {url}");
                // }

                warp::reply::json(
                    &UpdatedUrls {
                        timestamp: 0,
                        update_urls: sub_changes.add.into_iter().map(|url| [url.clone(), url]).collect()
                    })
                    .into_response()
            });

        get.or(upload)
    };

    let episodes = {
        let get = warp::path!("api" / "2" / "episodes" / String)
            .and(warp::get())
            .and(warp::query())
            .map(|username_format, query: QuerySince| {
                println!("episodes GET for {username_format} since {query:?}");

                warp::reply::json(
                    &EpisodeChanges {
                        timestamp: 0,
                        actions: vec![],
                    })
            });

        let upload = warp::path!("api" / "2" / "episodes" / String)
            .and(warp::post())
            .and(warp::body::json())
            .map(|username_format, body: Vec<EpisodeActionUpload>| {
                println!("episodes POST for {username_format}");

                for action in &body {
                    println!("  {:?}", action);
                }

                warp::reply::json(
                    &UpdatedUrls { // FIXME: rename struct
                        timestamp: 0, // FIXME: timestamping
                        update_urls: vec![], // unused by client
                    })
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
struct Device {
    id: String,
    caption: String,
    r#type: DeviceType,
    subscriptions: i64,
}

#[derive(Debug, Deserialize, Serialize)] // FIXME: drop Serialize
struct DeviceCreate { // FIXME: allow "" to deserialise to this
    caption: Option<String>,
    r#type: Option<DeviceType>,
}

#[derive(Debug, Deserialize, Serialize)]
enum DeviceType {
    #[serde(rename = "mobile")]
    Mobile,
}

#[derive(Debug, Deserialize, Serialize)]
struct DeviceId(String);

#[derive(Debug, Deserialize, Serialize)]
struct Subscription {
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
// let subscriptions: [&'static str; 1] = [
//     "http://test.com",
//     // Subscription {
//     //     url: "http://test.com".into(),
//     //     title: "test pod".into(),
//     //     author: "rob".into(),
//     //     description: "a test podcast".into(),
//     //     subscribers: 2,
//     //     logo_url: "https://avatars.githubusercontent.com/u/205673?s=40&v=4".into(),
//     //     scaled_logo_url: "https://avatars.githubusercontent.com/u/205673?s=40&v=4".into(),
//     //     website: "https://github.com/bobrippling".into(),
//     //     mygpo_link: "https://github.com/bobrippling".into(),
//     // },
// ];

#[derive(Debug, Deserialize, Serialize)]
struct SubscriptionChanges {
    add: Vec<String>, // TODO: make these &str?
    remove: Vec<String>,
    timestamp: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize)]
struct EpisodeChanges {
    timestamp: u32,
    actions: Vec<EpisodeAction>,
}

#[derive(Debug, Deserialize, Serialize)]
struct QuerySince {
    since: u32,
}

#[derive(Debug, Deserialize, Serialize)]
struct EpisodeAction {
    podcast: String,
    episode: String,
    #[serde(with = "time::serde::rfc3339")]
    timestamp: time::OffsetDateTime, // yyyy-MM-dd'T'HH:mm:ss;
    guid: Option<String>,
    action: EpisodeActionE,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase", untagged)]
enum EpisodeActionE {
    New,
    Download,
    Play {
        started: u32,
        position: u32,
        total: u32,
    },
    Delete,
}

#[derive(Debug, Deserialize, Serialize)]
struct EpisodeActionUpload {
    podcast: String,
    episode: String,
    #[serde(with = "time_custom")]
    timestamp: time::PrimitiveDateTime,
    guid: Option<String>,
    #[serde(flatten)] // TODO: use this to combine common fields across types
    action: EpisodeActionE, // FIXME: spread EpisodeActionE into this type
    device: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct UpdatedUrls {
    timestamp: u32,
    update_urls: Vec<[String; 2]>,
}

time::serde::format_description!( // FIXME: swap to chrono & Utc.datetime_from_str(<time>, <fmt>) ?
    time_custom,
    PrimitiveDateTime,
    "[year]-[month]-[day]T[hour]:[minute]:[second]" // yyyy-MM-dd'T'HH:mm:ss
);
