# Search engines

How hashi.rs presents itself to search engines, what the 2026-10-26 audit found, and
what the owner does by hand in the search consoles. The code is in `src/seo.rs`
(page metadata, structured data, sitemap), `src/server/search.rs` (verification
files, IndexNow key) and `src/server/router/` (routes, redirects); the settings are
in [deployment.md](deployment.md#search-engines).

## What the site does

- **Crawling**: `robots.txt` allows everything and names the sitemap. `/api/` is
  not disallowed on purpose: Google renders pages with their server-function calls,
  a disallowed API would leave the rendered daily empty. `http://` redirects to
  `https://` (Fly, `force_https`); `www.hashi.rs` has no DNS record, so there is
  no second host to consolidate.
- **Sitemap**: `/sitemap.xml`, one file (180 URLs, far below the 50,000 URL and
  50 MB limits, so no sitemap index). Every indexable page in all 20 languages,
  each `<url>` with `xhtml:link` alternates for every language, itself among them,
  and `x-default`. No noindex page is in it. `<lastmod>` only on the pages that
  change at midnight UTC (the daily and the ranking): Google uses `lastmod` only
  while it is consistently right, and the other pages change with some deploys and
  not others. `changefreq` and `priority` are ignored by Google; they stay for
  other crawlers.
- **One URL per page**: every page has a self-referencing canonical link,
  `hreflang` alternates for all languages and `x-default` (the English URL without
  prefix). `/rules/` and `/en/rules` redirect permanently to `/rules`.
- **No index**: accounts, groups, invites, replays, players' pages, the Dojo's
  entries, the admin page and the 404 page have `<meta name="robots"
  content="noindex">` and no canonical link.
- **Titles and descriptions**: per page and language, unique within a language,
  titles at most 60 and descriptions at most 160 characters (a test checks it,
  `seo::tests::titles_and_descriptions_fit_in_search_results`).
- **Structured data** (JSON-LD): the home page has the `WebSite` (site name,
  alternate names), the game as `VideoGame` + `WebApplication` (free `Offer`,
  `GameApplication`) and its author; the rules page a `FAQPage`. Google stopped
  showing FAQ rich results in May 2026 and shows software rich results only with
  ratings, which the site has none of: the markup still tells Google, Bing and
  AI search what the pages are, and costs nothing. No `HowTo` (Google dropped it in
  2023).
- **Social previews**: Open Graph and Twitter tags on every page, an image per
  language (`/og/<lang>.png`), the home page today's daily (`/og/daily/<date>/`).
- **Languages**: `<html lang dir>` per page (`dir="rtl"` for Arabic), the rules
  ("how to play Hashiwokakero") in every language at `/<lang>/rules`.
- **Verification files** for Google, Bing and Yandex, and the IndexNow key file,
  from settings (empty until the tokens exist, see below).
- **IndexNow**: `scripts/indexnow.mjs`, run by `.github/workflows/indexnow.yml`,
  submits to Bing, Yandex and the other IndexNow engines the sitemap URLs a deploy
  added or changed, and after midnight UTC the pages of the new daily (the URLs
  whose `lastmod` is today). Not every deploy's URLs: a deploy changes code, rarely
  the pages, and IndexNow asks for changed URLs only. Skipped while
  `HASHI_INDEXNOW_KEY` is empty. Google doesn't take part in IndexNow.

## Audit, 2026-10-26

Checked on the live site: all 180 sitemap URLs fetched and their head compared
(status, canonical, reciprocal hreflang, `lang`, titles, descriptions, `h1`, image
alt texts), redirects, the 404 page, `robots.txt`, the manifest, and Lighthouse
(mobile, simulated slow 4G) on `/`, `/ja`, `/rules` and `/practice`.

Good already: canonical and hreflang were consistent and reciprocal on every URL,
every page had one `h1`, no image without alt text (the boards are SVG with
labels), no untranslated title or description, 404s return 404 with noindex,
Lighthouse SEO 100 and Best Practices 100 on all four pages.

Fixed:

- `/rules/`, `/de/` (a trailing slash) answered 404 with the full page; now 301.
- `/en/...` answered 200 with the same page as `/...`; now 301.
- `sitemap.xml` had no `lastmod`; now the daily pages have one.
- The game's structured data was a `VideoGame` alone, which Google doesn't read as
  software, with `applicationCategory: Game` (not one of Google's values) and
  `codeRepository` (not a property of a game); the `WebSite`'s URL wasn't the
  canonical home page URL.
- A Dojo entry's page (`/dojo/<id>`) had the Dojo's canonical link and alternates,
  as if it were the Dojo; now noindex like players' pages.
- Nine descriptions were longer than 160 characters (the benchmark page in eight
  languages, the season archive in Spanish).
- No way to verify the site in the consoles without editing code; now settings.

Recommended, not done:

- **Core Web Vitals of the home page.** Lighthouse mobile: performance 61 (`/`) and
  57 (`/ja`), LCP 21 s, against 96 on `/rules`. The largest element of the home
  page is the daily's cover, which the browser renders only after the WASM
  (12.3 MB, 3.7 MB with Brotli) has loaded and `daily_today` has answered; the
  server sends "Loading…" in its place. Rendering the cover on the server (its size
  only needs the day) would make the home page as fast as the rules page. A smaller
  WASM (route-level code splitting for the Lab, Dojo and workshop, `wasm-opt -Oz`)
  would help every page's interactivity (TBT 240–370 ms). Field data (Search
  Console's Core Web Vitals report) appears once there is enough traffic.
- **Accessibility findings** of Lighthouse (score 88–92): colour contrast of the
  muted text, touch target sizes, table headers in the ranking, names of progress
  bars. Not ranking factors, but worth a pass.
- **Past dailies as pages.** Every past daily could be a page (`/daily/2026-10-25`:
  the puzzle to play, its ranking, the solution), new long-tail URLs every day and a
  natural use of IndexNow. Little search demand for single days, so only worth it
  as a feature for players (an archive), not for SEO alone.
- **Home page heading.** The `h1` is "Daily bridge #10"; "Hashiwokakero" first
  appears in the `h2` further down.
- **Links from elsewhere** matter more than anything above for a new site: puzzle
  communities (r/puzzles, r/logicpuzzles), a "Show HN" for the SAT Dojo,
  directories of daily games, and the store listings once the apps are live.

## Registering the site (owner checklist)

The settings below are in `fly.toml` (`[env]`), public, and take effect with the
next deploy (a PR to `main`). After it, `curl -i https://hashi.rs/<file>` must
answer 200 before you press "Verify".

### Google Search Console

1. Open https://search.google.com/search-console and add a property:
   - **Domain** `hashi.rs` (recommended: covers http, https and every subdomain).
     Google shows a TXT record; add it at the domain's DNS provider, then press
     Verify. No setting needed.
   - Or **URL prefix** `https://hashi.rs/` with the "HTML file" method: copy the
     file's name (`google<token>.html`) into `HASHI_GOOGLE_SITE_VERIFICATION`,
     deploy, check `https://hashi.rs/google<token>.html`, press Verify. Keep the
     setting afterwards, Google checks again from time to time.
2. Sitemaps: submit `https://hashi.rs/sitemap.xml`. Status "Success" with 180
   discovered pages.
3. URL inspection: "Request indexing" for `https://hashi.rs/`, `/rules`,
   `/practice` and a few languages (`/de`, `/ja`, `/es`); the daily quota is small.
4. After a few days, Indexing, Pages: "Excluded by noindex tag" (accounts,
   players, groups, …) and "Page with redirect" (`/en/…`, trailing slashes) are
   expected. "Duplicate without user-selected canonical" or "Alternate page with
   proper canonical tag" on a sitemap URL would be a problem.
5. Languages: Search Console has no hreflang report any more (the International
   Targeting report was retired in 2022). Spot-check with the URL inspection of
   `/de` (Google-selected canonical = `https://hashi.rs/de`) and, if wanted, an
   external hreflang checker on a few URLs.
6. Structured data: https://validator.schema.org on `https://hashi.rs/` (no errors
   expected); the Rich Results Test lists the software app as not eligible because
   it has no ratings, which is expected.
7. Settings, Users and permissions: add anyone else who should see the data.

### Bing Webmaster Tools

1. Open https://www.bing.com/webmasters and choose **Import from Google Search
   Console**: it takes the verified site and its sitemap, no token needed.
   Without GSC: add `https://hashi.rs/`, pick "XML file", put the code in its
   `<user>` into `HASHI_BING_SITE_AUTH`, deploy, check
   `https://hashi.rs/BingSiteAuth.xml`, verify.
2. Sitemaps: check that `https://hashi.rs/sitemap.xml` is listed (submit it if not).
3. IndexNow (Bing, Yandex, Seznam, Naver and Yep share submissions): generate a key
   (`uuidgen`), put it into `HASHI_INDEXNOW_KEY`, deploy, check
   `https://hashi.rs/<key>.txt` shows the key. Then submit every URL once:
   `gh workflow run indexnow.yml -f all=true` (or Actions, IndexNow, Run workflow,
   "all"); from then on the deploys and the nightly run submit what changes. The
   submissions appear in Bing Webmaster Tools under IndexNow.

### Yandex Webmaster (optional, for the Russian and Ukrainian pages)

1. Open https://webmaster.yandex.com, add `https://hashi.rs`, choose "HTML file",
   put the file's name (`yandex_<token>.html`) into `HASHI_YANDEX_VERIFICATION`,
   deploy, check the file, verify.
2. Indexing, Sitemap files: add `https://hashi.rs/sitemap.xml`.

### Others (optional)

- Naver Search Advisor (Korean): https://searchadvisor.naver.com, verify with a
  meta tag or file (not built in; IndexNow already reaches Naver).
- Baidu is not worth it without a Chinese host and ICP licence.

### Is a "Google sitemap" still needed?

The site already has the one sitemap Google, Bing and the others read
(`/sitemap.xml`, the sitemaps.org format), named in `robots.txt`, so crawlers find it
without being told. There is no separate Google format. Submitting it once in
Search Console and Bing Webmaster Tools is still worth it for their reports (how
many URLs were read, errors). The old "ping" URLs
(`google.com/ping?sitemap=…`, Bing's) were retired in 2023; IndexNow replaces
them for Bing and the engines that share it, Google has no equivalent and recrawls
on its own.
