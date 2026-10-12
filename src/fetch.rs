//! A polite fetcher: robots.txt, an honest user agent, per-host spacing,
//! and a hard stop on bot walls (we report them, we never work around them).
use std::collections::HashMap;
use std::time::{Duration, Instant};
use texting_robots::Robot;
use tokio::sync::Mutex;
use url::Url;

pub const UA: &str = concat!("page-canary/", env!("CARGO_PKG_VERSION"), " (+https://github.com/sivaratrisrinivas/page-canary)");
pub const UA_TOKEN: &str = "page-canary";

#[derive(Debug)]
pub enum FetchError {
    /// robots.txt says no for our agent
    Disallowed,
    /// 401/403/429/503 or a challenge page. We stop and report.
    Blocked(String),
    Other(String),
}
impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self { Self::Disallowed => write!(f, "disallowed by robots.txt"), Self::Blocked(s) => write!(f, "blocked: {s}"), Self::Other(s) => write!(f, "{s}") }
    }
}

struct Host { robot: Option<Robot>, next_ok: Instant, delay: Duration, blocked_until: Option<Instant> }

pub struct Polite { client: reqwest::Client, min_delay: Duration, hosts: Mutex<HashMap<String, Host>> }

impl Polite {
    pub fn new(min_delay: Duration) -> Self {
        let client = reqwest::Client::builder().user_agent(UA).timeout(Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::limited(5)).build().unwrap();
        Self { client, min_delay, hosts: Mutex::new(HashMap::new()) }
    }

    async fn robots(&self, u: &Url) -> Host {
        let robots_url = format!("{}/robots.txt", u.origin().ascii_serialization());
        let robot = match self.client.get(&robots_url).send().await {
            Ok(r) if r.status().is_success() => r.bytes().await.ok().and_then(|b| Robot::new(UA_TOKEN, &b).ok()),
            // 4xx: no rules, everything allowed. 5xx / network: be careful, allow nothing.
            Ok(r) if r.status().is_client_error() => Robot::new(UA_TOKEN, b"").ok(),
            _ => Robot::new(UA_TOKEN, b"User-agent: *\nDisallow: /").ok(),
        };
        let crawl = robot.as_ref().and_then(|r| r.delay).map(|d| Duration::from_secs_f32(d)).unwrap_or_default();
        Host { robot, next_ok: Instant::now(), delay: self.min_delay.max(crawl), blocked_until: None }
    }

    pub async fn get(&self, url: &str) -> Result<String, FetchError> {
        let u = Url::parse(url).map_err(|e| FetchError::Other(e.to_string()))?;
        let key = u.origin().ascii_serialization();
        let wait = {
            let mut hosts = self.hosts.lock().await;
            if !hosts.contains_key(&key) { let h = self.robots(&u).await; hosts.insert(key.clone(), h); }
            let h = hosts.get_mut(&key).unwrap();
            if let Some(t) = h.blocked_until { if Instant::now() < t { return Err(FetchError::Blocked("backing off after an earlier block".into())); } }
            if !h.robot.as_ref().map(|r| r.allowed(url)).unwrap_or(false) { return Err(FetchError::Disallowed); }
            let now = Instant::now();
            let start = h.next_ok.max(now);
            h.next_ok = start + h.delay;
            start - now
        };
        tokio::time::sleep(wait).await;
        let r = self.client.get(url).header("Accept", "text/html").send().await.map_err(|e| FetchError::Other(e.to_string()))?;
        let st = r.status().as_u16();
        if [401, 403, 429, 503].contains(&st) {
            self.back_off(&key).await;
            return Err(FetchError::Blocked(format!("HTTP {st}")));
        }
        if !r.status().is_success() { return Err(FetchError::Other(format!("HTTP {st}"))); }
        let body = r.text().await.map_err(|e| FetchError::Other(e.to_string()))?;
        let low = body[..body.len().min(20000)].to_ascii_lowercase();
        if ["cf-challenge", "challenge-platform", "client challenge", "<title>just a moment", "captcha", "are you a robot", "access denied", "enable javascript to proceed"].iter().any(|m| low.contains(m)) && body.len() < 60000 {
            self.back_off(&key).await;
            return Err(FetchError::Blocked("challenge page".into()));
        }
        Ok(body)
    }

    async fn back_off(&self, key: &str) {
        if let Some(h) = self.hosts.lock().await.get_mut(key) {
            h.blocked_until = Some(Instant::now() + Duration::from_secs(6 * 3600));
        }
    }
}
