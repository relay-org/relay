use lay::{
    crypto::KeyPair,
    text::{Post, PostRequest},
    Signed,
};

#[tokio::main]
async fn main() {
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();

    let pkcs8 = KeyPair::generate_pkcs8().unwrap();
    let key_pair = KeyPair::from_pkcs8(&pkcs8).unwrap();

    let client = reqwest::Client::new();

    let post = Signed::new(
        &key_pair,
        0,
        Post {
            channel: "general".to_string(),
            content: input,
            metadata: None,
        },
    )
    .unwrap();

    let resp = client
        .post("http://0.0.0.0:3000/text")
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&post).unwrap())
        .send()
        .await
        .unwrap();

    println!("POST: {}", resp.status());

    let req = Signed::new(
        &key_pair,
        0,
        PostRequest {
            channel: "general".to_string(),
            metadata: None,
        },
    )
    .unwrap();

    let resp = client
        .get("http://0.0.0.0:3000/text")
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&req).unwrap())
        .send()
        .await
        .unwrap();

    println!("GET: {}", resp.status());

    let resp = resp.text().await.unwrap();
    let messages: Vec<Signed<Post>> = serde_json::from_str(&resp).unwrap();

    println!("Messages ({}):", messages.len());

    for message in &messages {
        println!(
            "{} ({}): {}",
            message.key,
            if message.verify() { "$" } else { "X" },
            message.data.content
        );
    }
}
