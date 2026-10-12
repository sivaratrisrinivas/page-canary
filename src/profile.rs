//! Page profiles: a DOM-shape fingerprint plus what the extractor sees.
use kuchikiki::traits::TendrilSink;
use kuchikiki::NodeRef;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub selector: String,
    /// text | int | url | date
    pub kind: String,
    #[serde(default)]
    pub attr: Option<String>,
    #[serde(default = "yes")]
    pub required: bool,
}
fn yes() -> bool { true }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Target {
    pub id: String,
    pub url: String,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct FieldProfile {
    pub count: usize,
    /// share of extracted values that look like `kind`
    pub valid: f64,
    /// ancestor signatures (3 levels) of the matched nodes
    pub anchors: BTreeSet<String>,
    pub sample: Vec<String>,
    /// number of distinct extracted values
    #[serde(default)]
    pub distinct: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Profile {
    /// distinct tag.class paths, 3 levels deep, text and ads ignored
    pub skeleton: BTreeSet<String>,
    pub fields: BTreeMap<String, FieldProfile>,
}

fn re(p: &str) -> Regex { Regex::new(p).unwrap() }
fn ad_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| re(r"(?i)^(ad|ads|adv|advert\w*|ad[-_].*|.*[-_]ad|.*[-_]ads|.*[-_]ad[-_].*|adsbygoogle|sponsor\w*|gpt-?ad.*|dfp.*|taboola.*|outbrain.*|promo[-_]?slot.*|banner[-_]?ad.*)$"))
}
fn hashy_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    // css-modules / styled-components / emotion style generated tokens
    R.get_or_init(|| re(r"^(css|sc|jsx|emotion|tw|svelte)-[A-Za-z0-9_-]+$|^[A-Za-z]+_[A-Za-z0-9]{5,}$|^[a-z]{1,3}[0-9][a-z0-9]{3,}$|.*\d{3,}.*"))
}

pub const SKIP_TAGS: &[&str] = &["script", "style", "noscript", "template", "iframe", "ins", "svg", "link", "meta", "head"];

pub fn is_ad(node: &NodeRef) -> bool {
    let Some(el) = node.as_element() else { return false };
    let a = el.attributes.borrow();
    if a.get("data-ad").is_some() || a.get("data-ad-slot").is_some() || a.get("data-google-query-id").is_some() { return true; }
    if let Some(l) = a.get("aria-label") { if l.eq_ignore_ascii_case("advertisement") { return true; } }
    let id = a.get("id").unwrap_or("");
    if !id.is_empty() && ad_re().is_match(id) { return true; }
    a.get("class").unwrap_or("").split_whitespace().any(|c| ad_re().is_match(c))
}

fn sig(node: &NodeRef) -> Option<String> {
    let el = node.as_element()?;
    let tag = el.name.local.to_string();
    let a = el.attributes.borrow();
    let mut cls: Vec<&str> = a.get("class").unwrap_or("").split_whitespace().filter(|c| !hashy_re().is_match(c)).collect();
    cls.sort();
    cls.truncate(3);
    Some(if cls.is_empty() { tag } else { format!("{tag}.{}", cls.join(".")) })
}

fn path(node: &NodeRef, depth: usize) -> String {
    let mut parts = vec![];
    let mut cur = Some(node.clone());
    while let Some(n) = cur {
        if parts.len() == depth { break; }
        match sig(&n) { Some(s) => parts.push(s), None => break }
        cur = n.parent();
    }
    parts.reverse();
    parts.join(">")
}

fn skeleton(doc: &NodeRef) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Ok(body) = doc.select_first("body") else { return out };
    walk(body.as_node(), &mut out);
    out
}
fn walk(n: &NodeRef, out: &mut BTreeSet<String>) {
    for c in n.children() {
        if let Some(el) = c.as_element() {
            let t = el.name.local.as_ref();
            if SKIP_TAGS.contains(&t) || is_ad(&c) { continue; }
            out.insert(path(&c, 3));
            walk(&c, out);
        }
    }
}

pub fn looks_like(kind: &str, v: &str) -> bool {
    static INT: OnceLock<Regex> = OnceLock::new();
    static DATE: OnceLock<Regex> = OnceLock::new();
    let v = v.trim();
    if v.is_empty() { return false; }
    match kind {
        "int" => INT.get_or_init(|| re(r"^\D{0,12}\d[\d,.]*\s*[kKmM]?\D{0,12}$")).is_match(v),
        "url" => v.starts_with("http") || v.starts_with('/') || v.starts_with("item?") || v.starts_with("./") || v.contains('/'),
        "date" => DATE.get_or_init(|| re(r"(?i)\b(19|20)\d\d\b|\b(jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)[a-z]*\b|(\d+|an?|one)\s+(second|minute|hour|day|week|month|year)s?\s+ago")).is_match(v),
        _ => v.chars().any(|c| c.is_alphanumeric()) && v.len() < 600,
    }
}

pub fn parse(html: &str) -> NodeRef { kuchikiki::parse_html().one(html) }

pub fn profile(html: &str, fields: &[Field]) -> Profile {
    let doc = parse(html);
    let mut p = Profile { skeleton: skeleton(&doc), fields: BTreeMap::new() };
    for f in fields {
        let mut fp = FieldProfile::default();
        let mut ok = 0;
        let mut seen = std::collections::HashSet::new();
        if let Ok(it) = doc.select(&f.selector) {
            for m in it {
                let n = m.as_node();
                let v = match &f.attr { Some(a) => m.attributes.borrow().get(a.as_str()).unwrap_or("").to_string(), None => n.text_contents() };
                let v = v.split_whitespace().collect::<Vec<_>>().join(" ");
                if looks_like(&f.kind, &v) { ok += 1; }
                seen.insert(v.clone());
                if fp.sample.len() < 3 { fp.sample.push(v.chars().take(80).collect()); }
                fp.anchors.insert(path(n, 3));
                fp.count += 1;
            }
        }
        fp.distinct = seen.len();
        fp.valid = if fp.count == 0 { 0.0 } else { ok as f64 / fp.count as f64 };
        p.fields.insert(f.name.clone(), fp);
    }
    p
}

pub fn jaccard(a: &BTreeSet<String>, b: &BTreeSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() { return 1.0; }
    a.intersection(b).count() as f64 / a.union(b).count() as f64
}
