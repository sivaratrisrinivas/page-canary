use page_canary::eval::{run_all, Results};

#[test]
fn replay_matches_committed_results() {
    let r = run_all("fixtures").unwrap();
    let saved: Results = serde_json::from_str(&std::fs::read_to_string("eval/results.json").unwrap()).unwrap();
    assert_eq!(r.scores, saved.scores, "eval drifted; rerun `cargo run --release -- eval` and review eval/RESULTS.md");
}

#[test]
fn headline_floors() {
    let r = run_all("fixtures").unwrap();
    let s = &r.scores["all/break_only"];
    assert!(s.recall() >= 0.95, "recall {}", s.recall());
    assert!(s.false_alarm() <= 0.05, "false alarms {}", s.false_alarm());
    let real = &r.scores["real/break_only"];
    assert_eq!(real.fn_, 0, "missed a real break");
}

#[test]
fn every_real_break_names_a_field() {
    let r = run_all("fixtures").unwrap();
    for c in r.cases.iter().filter(|c| c.set == "real" && c.label_break) {
        assert!(c.report.signals.iter().any(|s| s.field.is_some()), "{} {}", c.config, c.after);
    }
}
