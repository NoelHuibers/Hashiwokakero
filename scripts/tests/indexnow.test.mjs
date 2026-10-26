// node --test scripts/tests/: which URLs scripts/indexnow.mjs submits, and how

import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { changedOn, changedSince, choose, keyOf, parseSitemap, payload } from '../indexnow.mjs';

const root = path.resolve(import.meta.dirname, '../..');

// The shape of the site's sitemap (src/seo.rs): alternates, lastmod only on the daily pages
const sitemap = (date) => `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml">
  <url>
    <loc>https://hashi.rs/</loc>
    <xhtml:link rel="alternate" hreflang="de" href="https://hashi.rs/de"/>
    <lastmod>${date}</lastmod>
    <changefreq>daily</changefreq>
  </url>
  <url>
    <loc>https://hashi.rs/de</loc>
    <xhtml:link rel="alternate" hreflang="en" href="https://hashi.rs/"/>
    <lastmod>${date}</lastmod>
  </url>
  <url>
    <loc>https://hashi.rs/rules</loc>
    <changefreq>monthly</changefreq>
  </url>
  <url>
    <loc>https://hashi.rs/ranks?a=1&amp;b=2</loc>
  </url>
</urlset>
`;

test('reads the URLs of a sitemap with their lastmod', () => {
  assert.deepEqual(parseSitemap(sitemap('2026-10-26')), [
    { loc: 'https://hashi.rs/', lastmod: '2026-10-26' },
    { loc: 'https://hashi.rs/de', lastmod: '2026-10-26' },
    { loc: 'https://hashi.rs/rules', lastmod: null },
    { loc: 'https://hashi.rs/ranks?a=1&b=2', lastmod: null },
  ]);
  assert.deepEqual(parseSitemap(''), []);
  // Not the alternates, only <loc>
  assert.equal(parseSitemap(sitemap('2026-10-26')).length, 4);
});

test("by default, today's pages", () => {
  const entries = parseSitemap(sitemap('2026-10-26'));
  assert.deepEqual(changedOn(entries, '2026-10-26'), ['https://hashi.rs/', 'https://hashi.rs/de']);
  assert.deepEqual(changedOn(entries, '2026-10-27'), []);
  // A timestamp counts for its day
  const stamped = [{ loc: 'https://hashi.rs/x', lastmod: '2026-10-26T00:00:00+00:00' }];
  assert.deepEqual(changedOn(stamped, '2026-10-26'), ['https://hashi.rs/x']);
});

test('after a deploy, what is new or has another date', () => {
  const before = parseSitemap(sitemap('2026-10-26'));
  // The same sitemap: nothing
  assert.deepEqual(changedSince(before, before), []);
  // Past midnight: the daily pages
  const after = parseSitemap(sitemap('2026-10-27'));
  assert.deepEqual(changedSince(before, after), ['https://hashi.rs/', 'https://hashi.rs/de']);
  // A new page
  const added = [...before, { loc: 'https://hashi.rs/new', lastmod: null }];
  assert.deepEqual(changedSince(before, added), ['https://hashi.rs/new']);
  // An empty sitemap before (it couldn't be fetched): today's pages, not everything
  assert.deepEqual(choose(before, { before: [], today: '2026-10-26' }), [
    'https://hashi.rs/',
    'https://hashi.rs/de',
  ]);
  assert.deepEqual(choose(after, { before, today: '2026-10-27' }), [
    'https://hashi.rs/',
    'https://hashi.rs/de',
  ]);
  assert.equal(choose(before, { all: true, today: '2026-10-26' }).length, 4);
});

test('submits URLs of its own host with the key file', () => {
  const key = '3f2b8a9c-4d1e-4f6a-9b7c-0e5d1a2b3c4d';
  assert.deepEqual(
    payload('https://hashi.rs', key, [
      'https://hashi.rs/',
      'https://hashi.rs/de',
      'https://hashi.rs/de',
      'https://example.com/',
      'https://www.hashi.rs/',
      'not a url',
    ]),
    {
      host: 'hashi.rs',
      key,
      keyLocation: `https://hashi.rs/${key}.txt`,
      urlList: ['https://hashi.rs/', 'https://hashi.rs/de'],
    },
  );
  const many = Array.from({ length: 10_050 }, (_, i) => `https://hashi.rs/${i}`);
  assert.equal(payload('https://hashi.rs', key, many).urlList.length, 10_000);
});

test('the key comes from fly.toml, none while it is empty', () => {
  assert.equal(keyOf('[env]\n  HASHI_INDEXNOW_KEY = "abcd1234efgh"\n'), 'abcd1234efgh');
  assert.equal(keyOf('[env]\n  HASHI_INDEXNOW_KEY = ""\n'), null);
  assert.equal(keyOf('[env]\n  # HASHI_INDEXNOW_KEY = "commented"\n'), null);
  assert.equal(keyOf('[env]\n'), null);
  // The real one parses (empty until the owner sets it, docs/seo.md)
  const fly = fs.readFileSync(path.join(root, 'fly.toml'), 'utf8');
  const key = keyOf(fly);
  assert.ok(key === null || /^[A-Za-z0-9-]{8,128}$/.test(key), key);
});
