#!/usr/bin/env node
// Tells the IndexNow search engines (Bing, Yandex, Seznam, Naver, Yep: they share what one
// receives) which pages of the site changed, so they crawl them soon instead of whenever.
// docs/seo.md; run by .github/workflows/indexnow.yml after every deploy and after midnight UTC.
//
//   node scripts/indexnow.mjs [--site https://hashi.rs] [--before sitemap.xml] [--all] [--dry-run]
//
// Which URLs of the site's sitemap:
//   default         the ones whose <lastmod> is today (UTC): the pages of the new daily
//   --before FILE   the ones new or with another <lastmod> than in FILE, the sitemap before a
//                   deploy (a new page, a new language); with an empty FILE, the default
//   --all           every one, once after registering or after changing many pages
// --dry-run lists them without submitting.
//
// The key is the site's HASHI_INDEXNOW_KEY: from the environment, or from fly.toml (it is
// public, the site serves it as /<key>.txt). Without a key nothing is submitted. Before
// submitting, the script checks that the site serves the key file, or the engines would reject
// every URL.

import fs from 'node:fs';
import path from 'node:path';

const ENDPOINT = 'https://api.indexnow.org/indexnow';
/** IndexNow takes at most 10,000 URLs per request. */
const MAX_URLS = 10_000;

/** The `<url>` entries of a sitemap: `{ loc, lastmod }`, `lastmod` null without one. */
export function parseSitemap(xml) {
  return [...xml.matchAll(/<url>([\s\S]*?)<\/url>/g)].map(([, entry]) => ({
    loc: decode(entry.match(/<loc>\s*([^<]*?)\s*<\/loc>/)?.[1] ?? ''),
    lastmod: entry.match(/<lastmod>\s*([^<]*?)\s*<\/lastmod>/)?.[1] ?? null,
  })).filter(({ loc }) => loc !== '');
}

function decode(text) {
  return text
    .replaceAll('&lt;', '<')
    .replaceAll('&gt;', '>')
    .replaceAll('&quot;', '"')
    .replaceAll('&apos;', "'")
    .replaceAll('&amp;', '&');
}

/** The URLs whose `<lastmod>` is `date` (`YYYY-MM-DD`, or a full timestamp on that day). */
export function changedOn(entries, date) {
  return entries.filter(({ lastmod }) => lastmod?.slice(0, 10) === date).map(({ loc }) => loc);
}

/** The URLs of `after` that `before` doesn't have, or has with another `<lastmod>`. */
export function changedSince(before, after) {
  const old = new Map(before.map(({ loc, lastmod }) => [loc, lastmod]));
  return after
    .filter(({ loc, lastmod }) => !old.has(loc) || old.get(loc) !== lastmod)
    .map(({ loc }) => loc);
}

/** `HASHI_INDEXNOW_KEY` of a fly.toml, null when it isn't there or is empty. */
export function keyOf(flyToml) {
  const key = flyToml.match(/^\s*HASHI_INDEXNOW_KEY\s*=\s*"([^"]*)"/m)?.[1]?.trim();
  return key ? key : null;
}

/** What `--before`, `--all` and the default choose (see the top of this file). */
export function choose(entries, { all = false, before = null, today }) {
  if (all) return entries.map(({ loc }) => loc);
  if (before !== null && before.length > 0) return changedSince(before, entries);
  return changedOn(entries, today);
}

/** The JSON body IndexNow wants for `urls` of `site`. Only URLs of the site's own host. */
export function payload(site, key, urls) {
  const { host, origin } = new URL(site);
  const own = urls.filter((url) => {
    try {
      return new URL(url).host === host;
    } catch {
      return false;
    }
  });
  return {
    host,
    key,
    keyLocation: `${origin}/${key}.txt`,
    urlList: [...new Set(own)].slice(0, MAX_URLS),
  };
}

function options(argv) {
  const opts = { site: 'https://hashi.rs', before: null, all: false, dryRun: false };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--site') opts.site = argv[++i];
    else if (arg === '--before') opts.before = argv[++i];
    else if (arg === '--all') opts.all = true;
    else if (arg === '--dry-run') opts.dryRun = true;
    else throw new Error(`unknown argument ${arg}`);
  }
  return opts;
}

async function text(url) {
  const res = await fetch(url, { redirect: 'manual' });
  if (!res.ok) throw new Error(`${url}: ${res.status}`);
  return res.text();
}

async function main() {
  const opts = options(process.argv.slice(2));
  const root = path.resolve(import.meta.dirname, '..');
  const key =
    process.env.HASHI_INDEXNOW_KEY?.trim() ||
    keyOf(fs.readFileSync(path.join(root, 'fly.toml'), 'utf8'));
  if (!key) {
    console.log('No HASHI_INDEXNOW_KEY (fly.toml), nothing submitted');
    return;
  }
  const site = opts.site.replace(/\/+$/, '');
  const entries = parseSitemap(await text(`${site}/sitemap.xml`));
  const before = opts.before === null ? null : parseSitemap(fs.readFileSync(opts.before, 'utf8'));
  const today = new Date().toISOString().slice(0, 10);
  const body = payload(site, key, choose(entries, { all: opts.all, before, today }));
  if (body.urlList.length === 0) {
    console.log(`No changed URLs among the sitemap's ${entries.length}, nothing submitted`);
    return;
  }
  console.log(`${body.urlList.length} URLs:\n${body.urlList.join('\n')}`);
  if (opts.dryRun) return;

  const served = (await text(body.keyLocation)).trim();
  if (served !== key) {
    throw new Error(`${body.keyLocation} doesn't serve the key (deployed yet?)`);
  }
  const res = await fetch(ENDPOINT, {
    method: 'POST',
    headers: { 'content-type': 'application/json; charset=utf-8' },
    body: JSON.stringify(body),
  });
  // 200: submitted, 202: accepted, the key is checked later
  if (res.status !== 200 && res.status !== 202) {
    throw new Error(`IndexNow answered ${res.status}: ${await res.text()}`);
  }
  console.log(`Submitted, IndexNow answered ${res.status}`);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  main().catch((e) => {
    console.error(e.message);
    process.exit(1);
  });
}
