//! Labeled synthetic mutations. Each one is applied to a real saved page.
//! Breaking ones hit what the extractor reads; benign ones touch everything else.
use crate::profile::{is_ad, parse, Field};
use kuchikiki::NodeRef;
use regex::Regex;

pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Self { Self(seed.wrapping_mul(0x9E3779B97F4A7C15) | 1) }
    pub fn next(&mut self) -> u64 { let mut x = self.0; x ^= x << 13; x ^= x >> 7; x ^= x << 17; self.0 = x; x }
    pub fn below(&mut self, n: usize) -> usize { (self.next() % n.max(1) as u64) as usize }
}

pub const BREAKING: &[&str] = &["class_rename", "remove_field", "tag_swap", "value_format", "js_shell", "truncate", "placeholder"];
pub const BENIGN: &[&str] = &["text_edit", "ad_churn", "reorder", "tracking_attrs", "new_section", "fewer_items", "script_churn", "restyle"];

fn sel(doc: &NodeRef, s: &str) -> Vec<NodeRef> {
    doc.select(s).map(|it| it.map(|m| m.as_node().clone()).collect()).unwrap_or_default()
}
fn frag(html: &str) -> Vec<NodeRef> {
    let d = parse(&format!("<html><body>{html}</body></html>"));
    let b = d.select_first("body").unwrap();
    let kids: Vec<NodeRef> = b.as_node().children().collect();
    for k in &kids { k.detach(); }
    kids
}
fn all_elements(doc: &NodeRef) -> Vec<NodeRef> {
    doc.descendants().filter(|n| n.as_element().is_some()).collect()
}
fn ancestors(n: &NodeRef) -> Vec<NodeRef> { n.ancestors().collect() }
/// Rows of the list the matches live in: children of the lowest common ancestor of the first two matches.
fn rows(ms: &[NodeRef]) -> Option<(NodeRef, Vec<NodeRef>)> {
    if ms.len() < 2 { return None; }
    let a0 = std::iter::once(ms[0].clone()).chain(ancestors(&ms[0])).collect::<Vec<_>>();
    let a1 = std::iter::once(ms[1].clone()).chain(ancestors(&ms[1])).collect::<Vec<_>>();
    let lca = a0.iter().find(|x| a1.iter().any(|y| y == *x))?.clone();
    let kids: Vec<NodeRef> = lca.children().filter(|c| c.as_element().is_some()).collect();
    Some((lca, kids))
}
fn class_tokens(selector: &str) -> Vec<String> {
    Regex::new(r"\.([A-Za-z0-9_-]+)").unwrap().captures_iter(selector).map(|c| c[1].to_string()).collect()
}
fn rename_classes(doc: &NodeRef, f: impl Fn(&str) -> Option<String>) -> usize {
    let mut n = 0;
    for e in all_elements(doc) {
        let el = e.as_element().unwrap();
        let mut a = el.attributes.borrow_mut();
        if let Some(c) = a.get("class").map(|s| s.to_string()) {
            let new: Vec<String> = c.split_whitespace().map(|t| match f(t) { Some(x) => { n += 1; x } None => t.to_string() }).collect();
            a.insert("class", new.join(" "));
        }
    }
    n
}
fn last_compound_has_tag(selector: &str) -> Option<String> {
    let last = selector.split(|c: char| c.is_whitespace() || c == '>' || c == '+' || c == '~').filter(|s| !s.is_empty()).last()?;
    let tag: String = last.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
    if tag.is_empty() { None } else { Some(tag) }
}
const KEEP: &[&str] = &["january","february","march","april","may","june","july","august","september","october","november","december","jan","feb","mar","apr","jun","jul","aug","sep","sept","oct","nov","dec","minute","minutes","hour","hours","day","days","week","weeks","month","months","year","years","ago","points","point"];

/// Returns the mutated page, or None if the mutation does not apply to this page.
pub fn apply(name: &str, html: &str, fields: &[Field], seed: u64) -> Option<String> {
    let mut rng = Rng::new(seed);
    let doc = parse(html);
    let primary = &fields[0];
    let ms = sel(&doc, &primary.selector);
    if ms.is_empty() { return None; }
    match name {
        "class_rename" => {
            let toks = class_tokens(&primary.selector);
            if toks.is_empty() { return None; }
            if rename_classes(&doc, |t| toks.iter().any(|x| x == t).then(|| format!("{t}-new"))) == 0 { return None; }
        }
        "remove_field" => for m in ms { m.detach() },
        "tag_swap" => {
            let tag = last_compound_has_tag(&primary.selector)?;
            let to = if tag == "a" { "span" } else if tag == "span" { "b" } else { "span" };
            for m in ms {
                let el = m.as_element().unwrap();
                let attrs: String = el.attributes.borrow().map.iter().map(|(k, v)| format!(" {}=\"{}\"", k.local, v.value.replace('"', "&quot;"))).collect();
                let new = frag(&format!("<{to}{attrs}></{to}>")).into_iter().find(|n| n.as_element().is_some())?;
                for c in m.children().collect::<Vec<_>>() { new.append(c); }
                m.insert_before(new);
                m.detach();
            }
        }
        "value_format" => {
            let f = fields.iter().find(|f| f.kind != "text").unwrap_or(primary);
            let ms = sel(&doc, &f.selector);
            if ms.is_empty() { return None; }
            for m in ms {
                if let Some(a) = &f.attr { m.as_element().unwrap().attributes.borrow_mut().insert(a.as_str(), "javascript:void(0)".into()); continue; }
                for c in m.children().collect::<Vec<_>>() { c.detach(); }
                m.append(NodeRef::new_text(if f.kind == "text" { " " } else { "See details" }));
            }
        }
        "js_shell" => {
            let b = doc.select_first("body").ok()?;
            for c in b.as_node().children().collect::<Vec<_>>() { c.detach(); }
            for n in frag(r#"<div id="root"></div><noscript>You need to enable JavaScript to run this app.</noscript><script src="/static/app.3f9a.js"></script>"#) { b.as_node().append(n); }
        }
        "placeholder" => {
            // skeleton loader: every row is there, every value says Loading
            if ms.len() < 3 { return None; }
            for m in ms { for c in m.children().collect::<Vec<_>>() { c.detach(); } m.append(NodeRef::new_text("Loading...")); }
        }
        "truncate" => {
            if ms.len() < 5 { return None; }
            let keep = ms.len() / 5;
            for m in ms.into_iter().skip(keep) { m.detach(); }
        }
        "text_edit" => {
            let words = Regex::new(r"[A-Za-z]{4,}|\d+").unwrap();
            let year = Regex::new(r"^(19|20)\d\d$").unwrap();
            let k = 1 + rng.below(8) as u8;
            for n in doc.descendants() {
                let Some(t) = n.as_text() else { continue };
                if n.parent().and_then(|p| p.as_element().map(|e| ["script", "style"].contains(&e.name.local.as_ref()))).unwrap_or(false) { continue; }
                let s = t.borrow().clone();
                let out = words.replace_all(&s, |c: &regex::Captures| {
                    let w = &c[0];
                    if KEEP.contains(&w.to_ascii_lowercase().as_str()) || year.is_match(w) { return w.to_string(); }
                    w.chars().map(|ch| if ch.is_ascii_digit() { (b'0' + (ch as u8 - b'0' + k) % 10) as char }
                        else if ch.is_ascii_lowercase() { (b'a' + (ch as u8 - b'a' + k) % 26) as char }
                        else if ch.is_ascii_uppercase() { (b'A' + (ch as u8 - b'A' + k) % 26) as char } else { ch }).collect()
                }).to_string();
                *t.borrow_mut() = out;
            }
        }
        "ad_churn" => {
            for e in all_elements(&doc) { if is_ad(&e) { e.detach(); } }
            let els = sel(&doc, "body div, body p, body table");
            let ad = |i: u64| format!(r#"<div class="ad-slot" data-ad-slot="{i}"><iframe src="https://ads.example.com/f?{i}"></iframe><span class="sponsored-label">Sponsored</span></div><ins class="adsbygoogle" data-ad-client="ca-pub-{i}"></ins>"#);
            for i in 0..3 {
                let at = if els.is_empty() { doc.select_first("body").ok()?.as_node().clone() } else { els[rng.below(els.len())].clone() };
                for n in frag(&ad(rng.next() % 100000 + i)) { at.insert_after(n); }
            }
        }
        "reorder" => {
            let (_lca, kids) = rows(&ms)?;
            if kids.len() < 3 { return None; }
            let parent = kids[0].parent()?;
            for k in kids.iter().rev() { k.detach(); parent.append(k.clone()); }
        }
        "fewer_items" => {
            let (_lca, kids) = rows(&ms)?;
            if kids.len() < 8 { return None; }
            let drop = kids.len() / 4;
            for k in kids.iter().rev().take(drop) { k.detach(); }
        }
        "tracking_attrs" => {
            for e in all_elements(&doc) {
                let el = e.as_element().unwrap();
                let h = format!("{:06x}", rng.next() & 0xffffff);
                let mut a = el.attributes.borrow_mut();
                a.insert(format!("data-v-{h}").as_str(), String::new());
                let c = a.get("class").unwrap_or("").to_string();
                a.insert("class", format!("{c} css-{h}").trim().to_string());
            }
        }
        "new_section" => {
            let b = doc.select_first("body").ok()?;
            let b = b.as_node();
            let top = frag(r#"<div class="cookie-banner"><p>We use cookies to improve this site.</p><button class="cookie-ok">OK</button></div>"#);
            for n in top.into_iter().rev() { b.prepend(n); }
            for n in frag(r#"<section class="newsletter"><h3>Subscribe</h3><form><input type="email"><button>Join</button></form></section>"#) { b.append(n); }
        }
        "script_churn" => {
            for s in sel(&doc, "script, style") { for c in s.children().collect::<Vec<_>>() { c.detach(); } s.append(NodeRef::new_text(format!("/* build {} */", rng.next()))); }
            let b = doc.select_first("body").ok()?;
            for n in frag(r#"<script>window.__x={"t":1}</script><style>.x{color:red}</style><script src="https://cdn.example.com/analytics.js"></script>"#) { b.as_node().append(n); }
        }
        "restyle" => {
            let keep: Vec<String> = fields.iter().flat_map(|f| class_tokens(&f.selector)).collect();
            rename_classes(&doc, |t| (!keep.iter().any(|k| k == t)).then(|| format!("{t}-r")));
        }
        _ => return None,
    }
    Some(doc.to_string())
}
