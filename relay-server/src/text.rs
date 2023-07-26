use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use lay::{
    text::{Post, PostRequest},
    Error, Signed,
};
use rbatis::RBatis;
use rbs::to_value;
use serde_json::json;

pub async fn get_text(
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

    if let Ok(last_req) = db
        .query_decode::<u64>(
            "select lastrequest from users where key='?';",
            vec![to_value!(req.key)],
        )
        .await
    {
        if req.timestamp <= last_req {
            let error = serde_json::to_value(Error {
                status: "IMPOSSIBLE_TIMESTAMP".to_string(),
                message: "Non-unique timestamp for request!".to_string(),
                details: None,
            })
            .unwrap();

            return (StatusCode::BAD_REQUEST, Json(error));
        }
    }

    // TODO: Add proper filters.
    let messages: Vec<Signed<Post>> = db
        .query_decode("select * from posts;", vec![])
        .await
        .unwrap();

    (
        StatusCode::OK,
        Json(serde_json::to_value(messages).unwrap()),
    )
}

pub async fn post_text(
    State(db): State<RBatis>,
    Json(req): Json<Signed<Post>>,
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

    db.exec(
        "insert into posts (key, server, timestamp, channel, content, signature) values (?1, ?2, ?3, ?4, ?5, ?6);",
        vec![
            to_value!(req.key),
            to_value!(req.server),
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
