use axum::{routing::get, Router, http::StatusCode};
use page_canary::fetch::{FetchError, Polite};
use std::time::{Duration, Instant};

async fn spawn() -> String {
    let app = Router::new()
        .route("/robots.txt", get(|| async { "User-agent: *\nDisallow: /private\nCrawl-delay: 1\n" }))
        .route("/ok", get(|| async { "<html><body><p>hi</p></body></html>" }))
        .route("/private/x", get(|| async { "secret" }))
        .route("/wall", get(|| async { (StatusCode::FORBIDDEN, "no") }))
        .route("/challenge", get(|| async { "<html><div id=cf-challenge>checking your browser</div></html>" }));
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    format!("http://{addr}")
}

#[tokio::test]
async fn obeys_robots_and_crawl_delay() {
    let base = spawn().await;
    let p = Polite::new(Duration::from_millis(10));
    assert!(matches!(p.get(&format!("{base}/private/x")).await, Err(FetchError::Disallowed)));
    let t = Instant::now();
    if let Err(e) = p.get(&format!("{base}/ok")).await { panic!("{e}") }
    p.get(&format!("{base}/ok")).await.unwrap();
    assert!(t.elapsed() >= Duration::from_millis(950), "Crawl-delay ignored: {:?}", t.elapsed());
}

#[tokio::test]
async fn stops_at_bot_walls() {
    let base = spawn().await;
    let p = Polite::new(Duration::from_millis(10));
    assert!(matches!(p.get(&format!("{base}/challenge")).await, Err(FetchError::Blocked(_))));
    // after a block the whole host is left alone, no retries
    assert!(matches!(p.get(&format!("{base}/ok")).await, Err(FetchError::Blocked(_))));
    let p2 = Polite::new(Duration::from_millis(10));
    assert!(matches!(p2.get(&format!("{base}/wall")).await, Err(FetchError::Blocked(_))));
}
