use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use lay::{
    profile::{Profile, ProfileRequest},
    Error, Signed,
};
use rbatis::RBatis;
use rbs::to_value;
use serde_json::json;

pub async fn get_profile(
    State(db): State<RBatis>,
    Json(req): Json<Signed<ProfileRequest>>,
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

    let Ok(profile) = db.query_decode::<Signed<Profile>>("select * from profiles where key=?;", vec![to_value!(req.data.target_key)]).await else {
        let error = serde_json::to_value(Error {
            status: "PROFILE_NOT_FOUND".to_string(),
            message: "Requested profile does not exist!".to_string(),
            details: None,
        })
        .unwrap();

        return (StatusCode::BAD_REQUEST, Json(error));
    };

    (
        StatusCode::OK,
        Json(serde_json::to_value(&profile).unwrap()),
    )
}

pub async fn post_profile(
    State(db): State<RBatis>,
    Json(req): Json<Signed<Profile>>,
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

    if let Ok(_) = db
        .query_decode::<String>(
            "select name from profiles where key=?1;",
            vec![to_value!(req.key.clone())],
        )
        .await
    {
        db.exec(
            "update profiles set name=?1 where key=?2;",
            vec![to_value!(req.data.name), to_value!(req.key)],
        )
        .await
        .unwrap();
    } else {
        db.exec(
        "insert into profiles (key, server, timestamp, name, signature) values (?1, ?2, ?3, ?4, ?5);",
        vec![
            to_value!(req.key),
            to_value!(req.server),
            to_value!(req.timestamp),
            to_value!(req.data.name),
            to_value!(req.signature),
        ],
    )
    .await
    .unwrap();
    }

    (StatusCode::OK, Json(json!({})))
}
