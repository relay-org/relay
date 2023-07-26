mod profile;
mod text;

use std::net::SocketAddr;

use axum::{routing::get, Router};
use profile::{get_profile, post_profile};
use rbatis::RBatis;
use text::{get_text, post_text};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // NOTE: This is used for HTTPS later, disabled for now.
    /*let _domain = std::env::var_os("RELAY_DOMAIN")
    .expect("Relay domain must be set via environment variable 'RELAY_DOMAIN'")
    .into_string()
    .unwrap();*/
    let db_url = std::env::var_os("DATABASE_URL")
        .expect("Relay db url must be set via environment variable 'RELAY_DB'")
        .into_string()
        .unwrap();

    let db = RBatis::new();
    db.init(rbdc_sqlite::driver::SqliteDriver {}, &db_url)
        .unwrap();
    db.get_pool().unwrap().resize(5);

    // setup db
    db.exec("create table if not exists posts (key varchar(48) not null, server varchar(48) not null, timestamp bigint not null, channel text not null, content text, signature varchar(96) primary key);", vec![]).await.unwrap();
    db.exec("create table if not exists users (key varchar(48) primary key, lastrequest bigint not null)", vec![]).await.unwrap();
    db.exec("create table if not exists profiles (key varchar(48) primary key, server varchar(48) not null, timestamp bigint not null, name varchar(255) not null, signature varchar(96) not null)", vec![]).await.unwrap();

    let app = Router::new()
        .route("/text", get(get_text).post(post_text))
        .route("/profile", get(get_profile).post(post_profile))
        .with_state(db);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
