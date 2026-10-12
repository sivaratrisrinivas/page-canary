# Demo video script

75.5 s, 1280x720, captions burned in, Kokoro af_heart voice at speed 0.95, about -16 LUFS. Recorded with headless Playwright against the live site.

| start | end | line |
|---|---|---|
| 0.5 | 10.3 | Datamint's hiring post says data teams lose thirty percent of their sprints to broken crawlers, and promises that drift is flagged before the pipeline sees a gap. |
| 10.6 | 16.9 | page-canary is a small Rust service that re-checks public pages for exactly those changes. |
| 17.2 | 24.1 | Each check asks three things. Do the selectors still match? Do the values still look right? Has the DOM shape moved? |
| 24.4 | 35.0 | It crawls politely: robots.txt, an honest user agent, spaced out requests, and a hard stop at bot walls. Lobsters says no, so it is skipped. |
| 35.3 | 46.8 | Here is a real break from the Wayback Machine. In 2022, Hacker News changed its story link markup. The old extractor goes empty, and page-canary names the field. |
| 47.2 | 50.0 | Rename one class, and the title selector dies. |
| 50.8 | 53.5 | Move the ad slots around, and nothing fires. |
| 54.3 | 57.6 | A skeleton loader is caught too, because the values collapse. |
| 58.2 | 69.2 | Fourteen real pairs and a hundred and seventy-three mutations: every break caught, and zero false breaks on benign changes. Big restyles still raise a soft warning. |
| 69.6 | 74.7 | The code and the results are on GitHub, at sivaratrisrinivas slash page-canary. |
