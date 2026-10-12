use page_canary::{fetch::Polite, eval};
use std::time::Duration;

mod server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("snap") => snap(args.get(2).map(|s| s.as_str()).unwrap_or("fixtures/real/plan.json")).await,
        Some("eval") => { let r = eval::run_all("fixtures")?; eval::write(&r, "eval")?; println!("{}", eval::summary_md(&r)); Ok(()) }
        Some("check") => server::check_cmd(args.get(2).map(|s| s.as_str()).unwrap_or("state/live.json")).await,
        Some("site") => server::site_cmd(args.get(2).map(|s| s.as_str()).unwrap_or("public"), args.get(3).map(|s| s.as_str())),
        Some("serve") | None => server::serve().await,
        Some(x) => anyhow::bail!("unknown command {x}; use serve | check | site | eval | snap"),
    }
}

/// Save archived snapshots through the same polite fetcher the monitor uses.
async fn snap(plan: &str) -> anyhow::Result<()> {
    #[derive(serde::Deserialize)]
    struct Item { url: String, file: String }
    let items: Vec<Item> = serde_json::from_str(&std::fs::read_to_string(plan)?)?;
    let p = Polite::new(Duration::from_secs(5));
    for it in items {
        if std::path::Path::new(&it.file).exists() { continue; }
        let mut res = p.get(&it.url).await;
        for _ in 0..3 {
            if matches!(&res, Err(page_canary::fetch::FetchError::Other(m)) if m.starts_with("HTTP 5")) { tokio::time::sleep(Duration::from_secs(20)).await; res = p.get(&it.url).await; } else { break }
        }
        match res {
            Ok(body) => {
                std::fs::create_dir_all(std::path::Path::new(&it.file).parent().unwrap())?;
                std::fs::write(&it.file, body)?;
                println!("saved {}", it.file);
            }
            Err(e) => println!("skip {}: {e}", it.url),
        }
    }
    Ok(())
}
