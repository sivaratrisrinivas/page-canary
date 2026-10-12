use axum::{extract::{Query, State}, response::{Html, IntoResponse, Response}, routing::get, Json, Router};
use page_canary::detect::{compare, Report, Thresholds, Verdict};
use page_canary::eval::Config;
use page_canary::fetch::{Polite, UA};
use page_canary::profile::{profile, Profile};
use page_canary::{mutate, eval};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

#[derive(Serialize, Clone, Default)]
struct Live {
    key: String, url: String,
    baseline_at: Option<String>, last_check: Option<String>,
    status: String, report: Option<Report>, error: Option<String>,
    checks: usize, alerts: usize,
    #[serde(skip)] baseline: Option<Profile>,
}

#[derive(Clone)]
struct App { live: Arc<RwLock<Vec<Live>>>, configs: Arc<Vec<Config>>, results: Arc<eval::Results>, fixtures: Arc<HashMap<String, String>> }

fn now() -> String { chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string() }

pub async fn serve() -> anyhow::Result<()> {
    let configs: Vec<Config> = serde_json::from_str(&std::fs::read_to_string("targets.json")?)?;
    let results: eval::Results = serde_json::from_str(&std::fs::read_to_string("eval/results.json")?)?;
    #[derive(serde::Deserialize)] struct B { config: String, file: String }
    #[derive(serde::Deserialize)] struct Fx { configs: Vec<Config>, synthetic_bases: Vec<B> }
    let fx: Fx = serde_json::from_str(&std::fs::read_to_string("fixtures/cases.json")?)?;
    let mut fixtures = HashMap::new();
    let mut all_cfg = configs.clone();
    for b in &fx.synthetic_bases { fixtures.insert(b.config.clone(), b.file.clone()); }
    all_cfg.extend(fx.configs.into_iter().filter(|c| fixtures.contains_key(&c.key)));
    let live = configs.iter().map(|c| Live { key: c.key.clone(), url: c.url.clone(), status: "pending".into(), ..Default::default() }).collect();
    let app = App { live: Arc::new(RwLock::new(live)), configs: Arc::new(all_cfg), results: Arc::new(results), fixtures: Arc::new(fixtures) };
    let minutes: u64 = std::env::var("CHECK_MINUTES").ok().and_then(|s| s.parse().ok()).unwrap_or(60);
    if std::env::var("NO_CRAWL").is_err() { tokio::spawn(monitor(app.clone(), configs, minutes)); }
    let r = Router::new()
        .route("/", get(index)).route("/api/status", get(status)).route("/api/eval", get(eval_json))
        .route("/try", get(try_mutation)).route("/health", get(|| async { "ok" })).route("/demo.mp4", get(video))
        .with_state(app);
    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".into());
    let l = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    println!("page-canary on :{port}, rechecking every {minutes} min as {UA}");
    axum::serve(l, r).await?;
    Ok(())
}

async fn monitor(app: App, configs: Vec<Config>, minutes: u64) {
    let polite = Polite::new(Duration::from_secs(10));
    let hook = std::env::var("ALERT_WEBHOOK").ok();
    loop {
        for (i, c) in configs.iter().enumerate() {
            let res = polite.get(&c.url).await;
            let mut live = app.live.write().await;
            let l = &mut live[i];
            l.last_check = Some(now());
            match res {
                Err(e) => { l.status = match e { page_canary::fetch::FetchError::Disallowed => "robots.txt: no".into(), page_canary::fetch::FetchError::Blocked(_) => "bot wall: stopped".into(), _ => "fetch error".into() }; l.error = Some(e.to_string()); }
                Ok(html) => {
                    l.error = None; l.checks += 1;
                    let p = profile(&html, &c.fields);
                    match &l.baseline {
                        None => { l.baseline = Some(p); l.baseline_at = Some(now()); l.status = "baseline set".into(); l.report = None; }
                        Some(b) => {
                            let rep = compare(b, &p, &c.fields, Thresholds::default());
                            if rep.verdict == Verdict::Ok { l.baseline = Some(p); l.baseline_at = Some(now()); l.status = "ok".into(); }
                            else {
                                l.alerts += 1;
                                l.status = format!("{:?}", rep.verdict).to_uppercase();
                                let msg = serde_json::json!({"target": c.key, "url": c.url, "report": rep});
                                eprintln!("ALERT {msg}");
                                if let Some(h) = &hook { let _ = reqwest::Client::new().post(h).json(&msg).send().await; }
                            }
                            l.report = Some(rep);
                        }
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(minutes * 60)).await;
    }
}

async fn status(State(a): State<App>) -> impl IntoResponse { Json(a.live.read().await.clone()) }
async fn eval_json(State(a): State<App>) -> impl IntoResponse { Json(a.results.scores.clone()) }
async fn video() -> Response {
    match tokio::fs::read("site/demo.mp4").await {
        Ok(b) => ([("content-type", "video/mp4"), ("cache-control", "public, max-age=86400")], b).into_response(),
        Err(_) => (axum::http::StatusCode::NOT_FOUND, "no video").into_response(),
    }
}

fn esc(s: &str) -> String { s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;") }
fn badge(v: &str) -> String {
    let c = match v { "BREAK" => "brk", "WARN" => "wrn", "ok" | "OK" | "baseline set" => "ok", _ => "mut" };
    format!("<span class='b {c}'>{}</span>", esc(v))
}
fn signals(r: &Report) -> String {
    if r.signals.is_empty() { return "<span class=mut>no signals</span>".into(); }
    r.signals.iter().map(|s| format!("<div>{} <code>{}</code>{} {}</div>", badge(&format!("{:?}", s.level).to_uppercase()), s.kind, s.field.as_ref().map(|f| format!(" <b>{}</b>", esc(f))).unwrap_or_default(), esc(&s.detail))).collect()
}

const CSS: &str = "body{font:15px/1.5 system-ui,sans-serif;max-width:1080px;margin:28px auto;padding:0 18px;color:#1d2330;background:#fbfaf7}h1{font-size:26px;margin:0}h2{font-size:18px;margin:30px 0 8px}table{border-collapse:collapse;width:100%;font-size:13.5px;background:#fff}td,th{border:1px solid #e3e0d8;padding:6px 8px;text-align:left;vertical-align:top}th{background:#f2efe8}code{background:#f2efe8;padding:0 4px;border-radius:3px}.b{display:inline-block;padding:1px 7px;border-radius:9px;font-size:12px;font-weight:600}.brk{background:#fbd5d0;color:#8a1c10}.wrn{background:#fdecc0;color:#7a5200}.ok{background:#d5f0dc;color:#145c2a}.mut{color:#7a7f8a}.big{display:flex;gap:14px;margin:16px 0}.big div{background:#fff;border:1px solid #e3e0d8;border-radius:10px;padding:12px 16px;flex:1}.big b{font-size:26px;display:block}a{color:#1f5fbf}select,button{font-size:14px;padding:4px 8px}";

async fn index(State(a): State<App>) -> Html<String> {
    let s = &a.results.scores;
    let g = |k: &str| s.get(k).cloned().unwrap_or_default();
    let syn = g("synthetic/break_only"); let real = g("real/break_only"); let any = g("synthetic/any_alert");
    let mut h = format!("<!doctype html><meta charset=utf-8><meta name=viewport content='width=device-width'><title>page-canary</title><style>{CSS}</style>
<h1>page-canary</h1><p>Re-checks public pages and flags layout or schema changes that would break an extractor, before the pipeline sees a gap.
Polite only: obeys robots.txt and Crawl-delay, sends <code>{}</code>, spaces requests per host, and stops on any bot wall. <a href='https://github.com/sivaratrisrinivas/page-canary'>Source</a> · <a href='/api/status'>status JSON</a> · <a href='/demo.mp4'>demo video</a></p>
<div class=big><div><b>{}/{}</b>synthetic breaks caught (BREAK alerts)</div><div><b>{}/{}</b>false BREAK alerts on benign mutations</div><div><b>{}/{}</b>real archived pairs labeled correctly</div><div><b>{}/{}</b>benign mutations that still raised a WARN</div></div>",
        esc(UA), syn.tp, syn.tp + syn.fn_, syn.fp, syn.fp + syn.tn, real.tp + real.tn, real.tp + real.tn + real.fp + real.fn_, any.fp, any.fp + any.tn);
    h += "<h2>Live checks</h2><table><tr><th>target</th><th>status</th><th>last check</th><th>signals</th></tr>";
    for l in a.live.read().await.iter() {
        h += &format!("<tr><td><b>{}</b><br><a class=mut href='{}'>{}</a></td><td>{}</td><td>{}<br><span class=mut>{} checks, {} alerts</span></td><td>{}</td></tr>", esc(&l.key), esc(&l.url), esc(&l.url), badge(&l.status), l.last_check.clone().unwrap_or("-".into()), l.checks, l.alerts,
            l.error.as_ref().map(|e| esc(e)).or(l.report.as_ref().map(signals)).unwrap_or_else(|| "<span class=mut>first fetch becomes the baseline</span>".into()));
    }
    h += "</table><h2>Try a mutation on a saved page</h2><form action=/try><select name=config>";
    let mut keys: Vec<&String> = a.fixtures.keys().collect(); keys.sort();
    for k in keys { h += &format!("<option>{}</option>", esc(k)); }
    h += "</select> <select name=m>";
    for m in mutate::BREAKING.iter().chain(mutate::BENIGN) { h += &format!("<option>{m}</option>"); }
    h += "</select> <button>replay</button></form>";
    h += "<h2>Real archived pairs</h2><table><tr><th>page</th><th>before → after</th><th>hand label</th><th>verdict</th><th>signals</th></tr>";
    for c in a.results.cases.iter().filter(|c| c.set == "real") {
        h += &format!("<tr><td>{}</td><td>{} → {}</td><td>{}</td><td>{}</td><td>{}<div class=mut>{}</div></td></tr>", esc(&c.config), c.before.rsplit('/').next().unwrap().trim_end_matches(".html"), c.after.rsplit('/').next().unwrap().trim_end_matches(".html"), if c.label_break { "break" } else { "benign" }, badge(&format!("{:?}", c.report.verdict).to_uppercase().replace("OK", "ok")), signals(&c.report), esc(&c.note));
    }
    h += "</table>";
    Html(h)
}

#[derive(serde::Deserialize)]
struct TryQ { config: String, m: String }
async fn try_mutation(State(a): State<App>, Query(q): Query<TryQ>) -> Html<String> {
    let (Some(file), Some(cfg)) = (a.fixtures.get(&q.config), a.configs.iter().find(|c| c.key == q.config)) else { return Html("unknown config".into()) };
    let Ok(html) = std::fs::read_to_string(format!("fixtures/{file}")) else { return Html("missing fixture".into()) };
    let label = if mutate::BREAKING.contains(&q.m.as_str()) { "breaking" } else { "benign" };
    let Some(out) = mutate::apply(&q.m, &html, &cfg.fields, 1) else { return Html(format!("<style>{CSS}</style><p>{} does not apply to this page. <a href=/>back</a>", esc(&q.m))) };
    let b = profile(&html, &cfg.fields); let n = profile(&out, &cfg.fields);
    let r = compare(&b, &n, &cfg.fields, Thresholds::default());
    let mut h = format!("<!doctype html><meta charset=utf-8><title>replay</title><style>{CSS}</style><p><a href=/>← page-canary</a></p><h1>{} on {}</h1><p>Saved page <code>{}</code>, mutation label: <b>{label}</b>. Verdict: {}</p><h2>Signals</h2>{}<h2>What the extractor sees</h2><table><tr><th>field</th><th>selector</th><th>before</th><th>after</th></tr>",
        esc(&q.m), esc(&q.config), esc(file), badge(&format!("{:?}", r.verdict).to_uppercase().replace("OK", "ok")), signals(&r));
    for f in &cfg.fields {
        let (x, y) = (&b.fields[&f.name], &n.fields[&f.name]);
        h += &format!("<tr><td>{}</td><td><code>{}</code></td><td>{} matches, {:.0}% valid<br><span class=mut>{}</span></td><td>{} matches, {:.0}% valid<br><span class=mut>{}</span></td></tr>", esc(&f.name), esc(&f.selector), x.count, x.valid * 100.0, esc(&x.sample.join(" | ")), y.count, y.valid * 100.0, esc(&y.sample.join(" | ")));
    }
    h += &format!("</table><p>DOM shape similarity: <b>{:.2}</b> (warn below {:.2})</p>", r.shape_similarity, Thresholds::default().shape);
    Html(h)
}
