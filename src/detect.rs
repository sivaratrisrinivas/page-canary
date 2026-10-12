//! Compare a fresh profile with the last accepted baseline.
use crate::profile::{jaccard, Field, Profile};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verdict { Ok, Warn, Break }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Signal { pub level: Verdict, pub kind: String, pub field: Option<String>, pub detail: String }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Report { pub verdict: Verdict, pub shape_similarity: f64, pub signals: Vec<Signal> }

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Thresholds {
    /// whole-page skeleton Jaccard below this is a layout warning
    pub shape: f64,
    /// a field's anchor Jaccard below this (while it still matches) is a warning
    pub anchor: f64,
    /// list fields losing more than this share of matches break
    pub count_drop: f64,
    /// value validity falling below this breaks (if the baseline was healthy)
    pub valid: f64,
}
impl Default for Thresholds {
    fn default() -> Self { Self { shape: 0.5, anchor: 0.34, count_drop: 0.5, valid: 0.7 } }
}

pub fn compare(base: &Profile, now: &Profile, fields: &[Field], t: Thresholds) -> Report {
    let mut s = vec![];
    for f in fields {
        let (Some(b), Some(n)) = (base.fields.get(&f.name), now.fields.get(&f.name)) else { continue };
        let lvl = if f.required { crate::detect::Verdict::Break } else { Verdict::Warn };
        if b.count > 0 && n.count == 0 {
            s.push(Signal { level: lvl, kind: "selector_died".into(), field: Some(f.name.clone()), detail: format!("`{}` matched {} before, 0 now", f.selector, b.count) });
            continue;
        }
        if b.count >= 4 && (n.count as f64) < (1.0 - t.count_drop) * b.count as f64 {
            s.push(Signal { level: lvl, kind: "matches_dropped".into(), field: Some(f.name.clone()), detail: format!("{} -> {} matches", b.count, n.count) });
        }
        if b.valid >= 0.9 && n.valid < t.valid {
            s.push(Signal { level: lvl, kind: "schema_drift".into(), field: Some(f.name.clone()), detail: format!("{:.0}% of values look like {} (was {:.0}%), e.g. {:?}", n.valid * 100.0, f.kind, b.valid * 100.0, n.sample.first().cloned().unwrap_or_default()) });
        }
        if b.count >= 3 && b.distinct * 2 >= b.count && n.count >= 3 && n.distinct == 1 {
            s.push(Signal { level: lvl, kind: "values_collapsed".into(), field: Some(f.name.clone()), detail: format!("{} matches, all {:?} (was {} distinct)", n.count, n.sample.first().cloned().unwrap_or_default(), b.distinct) });
        }
        let aj = jaccard(&b.anchors, &n.anchors);
        if b.count > 0 && aj < t.anchor {
            s.push(Signal { level: Verdict::Warn, kind: "field_moved".into(), field: Some(f.name.clone()), detail: format!("matches sit in a new container (anchor overlap {aj:.2})") });
        }
    }
    let sim = jaccard(&base.skeleton, &now.skeleton);
    if sim < t.shape {
        s.push(Signal { level: Verdict::Warn, kind: "layout_changed".into(), field: None, detail: format!("DOM shape similarity {sim:.2} < {:.2}", t.shape) });
    }
    let verdict = s.iter().map(|x| x.level).max().unwrap_or(Verdict::Ok);
    Report { verdict, shape_similarity: sim, signals: s }
}
