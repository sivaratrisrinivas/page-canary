# page-canary

A small Rust service that re-checks public pages and raises an alert when a layout or schema change would break an extractor, before the pipeline sees a gap.

Built by Srinivas Sivaratri as a demo for Datamint's Rust Systems Engineer role. Not affiliated with Datamint.

Live: https://sivaratrisrinivas.github.io/page-canary/ (rebuilt by a GitHub Action that runs one polite check every 6 hours) · Video (75 s): [site/demo.mp4](site/demo.mp4), also on the [live site](https://sivaratrisrinivas.github.io/page-canary/demo.mp4) · Full results: [eval/RESULTS.md](eval/RESULTS.md)

## Why

The Datamint post says teams "end up spending 30% of their sprints fixing broken crawlers", and the site promises that schema drift "is detected on the next run and flagged before your pipeline sees a gap". This repo is my take on that check, small enough to read in one sitting.

## Headline numbers

Measured by `cargo run --release -- eval`, replayed in CI from saved pages, no network.

| set | cases | breaks caught | false BREAK alerts on benign | benign that still got a WARN |
|---|---|---|---|---|
| Real archived before/after pairs (8 sites) | 14 | 4 of 4 | 0 of 10 | 1 of 10 |
| Labeled synthetic mutations on real pages | 173 | 49 of 49 | 0 of 124 | 8 of 124 |

Read these with care:
- The real pairs are few. The 4 real breaks are HN `a.storylink` becoming `.titleline > a` (2022), Wikipedia's Vector 2022 TOC (2023), GitHub Trending moving repo names from `h1` to `h2`, and Lobsters moving vote counts to `.upvoter`. I labeled each pair by hand: is the data still on the page, and does the old extractor still return it?
- The synthetic breaking mutations were written by me against the extractor's own selectors, so high recall there is expected. Their job is to check that every break type is caught and named, and to measure false alarms on benign churn (text edits, ad slots added or removed, reordered rows, tracking attributes, hashed CSS classes, new banners, fewer rows, script churn).
- WARN is the soft tier. All 8 synthetic WARNs are the "restyle" mutation (every class except the extractor's renamed), and the 1 real WARN is GitHub Trending 2020 to 2022 (new wrapper classes, selectors still fine). That is the cost of the DOM-shape signal: it flags big redesigns even when the extractor survives.

## How it works

For each target there is a URL and a list of fields (CSS selector, expected kind: text, int, url or date). Each check builds a profile and compares it to the last accepted baseline:

- **Selector survival.** A field that matched before and matches nothing now, or lost more than half its matches, is a BREAK.
- **Schema drift.** Values that stop looking like their kind (a price that now says "See details"), or that collapse to one repeated value (a skeleton loader saying "Loading..."), are a BREAK.
- **DOM-shape fingerprint.** The set of 3-level `tag.class` paths in the body, ignoring text, scripts, styles, iframes, ad slots and hashed CSS-module classes. Jaccard below 0.5 against the baseline is a WARN. So is a field whose matches now sit under a different container.

A clean check becomes the new baseline. An alert keeps the old one, so a break stays visible until someone fixes the extractor. Alerts go to stderr and, if `ALERT_WEBHOOK` is set, to that URL as JSON. The live demo has no webhook set.

## Polite crawling only

- Reads robots.txt for every host and obeys `Disallow` and `Crawl-delay` (HN asks for 30 s).
- Identifies itself: `page-canary/0.1.0 (+https://github.com/sivaratrisrinivas/page-canary)`.
- At least 10 s between requests to one host. The live demo checks each target once every 6 hours.
- On 401, 403, 429, 503 or a challenge page it stops and leaves the host alone for 6 hours. No retries, no other user agent, no headless browser, no proxies.

You can see this on the live page: Lobsters disallows unknown bots in robots.txt, so it is skipped. PyPI sometimes answers with a JavaScript client challenge; when it does, the monitor reports "bot wall: stopped" and moves on. `tests/polite.rs` checks both behaviours against a local server.

## Run it

```
cargo test --release            # replay tests on saved snapshots + politeness tests
cargo run --release -- eval     # rewrites eval/RESULTS.md and eval/results.json
cargo run --release -- serve    # dashboard on :8080, checks targets.json every hour (CHECK_MINUTES)
cargo run --release -- check state/live.json   # one pass, baselines kept in a file
cargo run --release -- site public state/live.json   # static dashboard, as deployed
cargo run --release -- snap     # re-download fixtures/real/plan.json from the Wayback Machine, politely
```

The dashboard also has a mutation replay section that replays any mutation on any saved page and shows what the extractor sees before and after.

The live demo is GitHub Pages plus a scheduled Action ([.github/workflows/pages.yml](.github/workflows/pages.yml)), baselines carried between runs in the Actions cache. A `Dockerfile` is included for running `serve` anywhere.

## Built vs simulated

- Real: the saved pages (Wayback Machine `id_` snapshots, 2020 to 2026), the 4 real layout breaks, the live checks.
- Simulated: the synthetic mutations, and the hand-picked thresholds (0.5 shape, 0.34 anchor, 50% match drop, 70% validity), which were not tuned on a held-out set.
- Not covered: pages that need JavaScript to render, changes that keep the selector and the value kind but return the wrong thing (for example a title selector that now hits a sidebar headline), and alert routing beyond one webhook.
