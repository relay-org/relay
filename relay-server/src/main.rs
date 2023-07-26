use std::net::SocketAddr;

use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::get, Json, Router};
use lay::{
    text::{Post, PostRequest},
    Error, Signed,
};
use rbatis::RBatis;
use rbs::to_value;
use serde_json::json;

async fn get_text(
    State(db): State<RBatis>,
    Json(req): Json<Signed<PostRequest>>,
) -> impl IntoResponse {
    if !req.verify() {
        let error = serde_json::to_value(Error {
            status: "FAILED_VERIFY_SIGNATURE".to_string(),
            message: "Signature verification failed!".to_string(),
            details: None,
        })
        .unwrap();

        return (StatusCode::BAD_REQUEST, Json(error));
    }

    let messages: Vec<Signed<Post>> = db
        .query_decode("select * from posts;", vec![])
        .await
        .unwrap();

    (
        StatusCode::OK,
        Json(serde_json::to_value(messages).unwrap()),
    )
}

async fn post_text(State(db): State<RBatis>, Json(req): Json<Signed<Post>>) -> impl IntoResponse {
    if !req.verify() {
        let error = serde_json::to_value(Error {
            status: "FAILED_VERIFY_SIGNATURE".to_string(),
            message: "Signature verification failed!".to_string(),
            details: None,
        })
        .unwrap();

        return (StatusCode::BAD_REQUEST, Json(error));
    }

    println!("{req:?}");

    db.exec(
        "insert into posts (key, timestamp, channel, content, signature) values (?1, ?2, ?3, ?4, ?5);",
        vec![
            to_value!(req.key),
            to_value!(req.timestamp),
            to_value!(req.data.channel),
            to_value!(req.data.content),
            to_value!(req.signature),
        ],
    )
    .await
    .unwrap();

    (StatusCode::OK, Json(json!({})))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let _domain = std::env::var_os("RELAY_DOMAIN")
        .expect("Relay domain must be set via environment variable 'RELAY_DOMAIN'")
        .into_string()
        .unwrap();
    let db_url = std::env::var_os("DATABASE_URL")
        .expect("Relay db url must be set via environment variable 'RELAY_DB'")
        .into_string()
        .unwrap();

    let db = RBatis::new();
    db.init(rbdc_sqlite::driver::SqliteDriver {}, &db_url)
        .unwrap();
    db.get_pool().unwrap().resize(5);

    let app = Router::new()
        .route("/text", get(get_text).post(post_text))
        .with_state(db);

    let addr = SocketAddr::from(([0, 0, 0, 0], 80));

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
