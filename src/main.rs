use warp::{Filter, Reply};
use serde::{Deserialize, Serialize};

use tracing::Level;
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
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
            .map(|username_dot_json| {
                println!("get devices for {username_dot_json}");

                let devices: [Device; 1] = [
                    Device {
                        caption: "test".into(),
                        r#type: DeviceType::Mobile,
                    },
                ];

                warp::reply::json(&devices)
            });

        let create = warp::path!("api" / "2" / "devices" / String / String)
            .and(warp::post())
            .and(warp::body::json())
            .map(|username, device_name, device: Device| {
                println!("got device {device_name} for {username}: {device:?}");

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

                #[derive(Debug, Deserialize, Serialize)]
                struct SubscriptionUpdates {
                    timestamp: u32,
                    update_urls: Vec<[String; 2]>,
                }

                warp::reply::json(
                    &SubscriptionUpdates {
                        timestamp: 0,
                        update_urls: sub_changes.add.into_iter().map(|url| [url.clone(), url]).collect()
                    })
                    .into_response()
            });

        get.or(upload)
    };

    let routes = login
        .or(devices)
        .or(subscriptions)
        .with(warp::trace::request());

    warp::serve(routes)
        .run(([0, 0, 0, 0], 8080))
        .await;
}

#[derive(Debug, Deserialize, Serialize)]
struct Device {
    caption: String,
    r#type: DeviceType,
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
