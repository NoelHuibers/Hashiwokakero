// The store listings' checks (store/validate.mjs, store/assemble.mjs): the repository's listings
// pass, and each rule catches what it is there for.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { deflateSync } from 'node:zlib';

import { checkImage, pngInfo } from '../../store/assemble.mjs';
import { APPLE, PLAY, SITE_LANGUAGES, siteUrl } from '../../store/locales.mjs';
import { loadListings, validate } from '../../store/validate.mjs';

const repo = loadListings();

test('the listings in the repository have no problems', () => {
  assert.deepEqual(validate(repo), []);
});

test('every language of the site has a listing in both stores, in its own words', () => {
  const sites = fs.readdirSync(new URL('../../locales', import.meta.url)).sort();
  assert.deepEqual([...SITE_LANGUAGES].sort(), sites);
  for (const site of SITE_LANGUAGES) {
    assert.ok(PLAY.some((l) => l.site === site && !l.same), `Google Play in ${site}`);
    assert.ok(APPLE.some((l) => l.site === site && !l.same), `App Store in ${site}`);
  }
  // Not English copied into the others
  const en = repo.apple['en-US'].description;
  for (const [locale, listing] of Object.entries(repo.apple)) {
    if (locale !== 'en-US') assert.notEqual(listing.description, en, locale);
  }
});

/** The repository's listings with one change, checked. */
function problems(change) {
  const listings = structuredClone(repo);
  change(listings);
  return validate(listings);
}

const has = (list, pattern) => assert.ok(list.some((p) => pattern.test(p)), `expected ${pattern} in:\n${list.join('\n')}`);

test('the stores limits', () => {
  has(problems((l) => (l.play['en-US'].title = 'Hashi: Daily Bridges Logic Puzzle')), /en-US: title has 33 characters, at most 30/);
  has(problems((l) => (l.play['de-DE'].short_description = 'x'.repeat(81))), /de-DE: short_description has 81/);
  has(problems((l) => (l.play['ja-JP'].full_description = '橋'.repeat(4001))), /ja-JP: full_description has 4001/);
  has(problems((l) => (l.apple['en-US'].subtitle = 'Hashiwokakero Logic Puzzle Game')), /en-US: subtitle has 31/);
  has(problems((l) => (l.apple['fr-FR'].promotional_text = 'é'.repeat(171))), /fr-FR: promotional_text has 171/);
  // Keywords count bytes: 34 kanji are 102
  has(problems((l) => (l.apple.ja.keywords = '脳'.repeat(34))), /ja: keywords has 102 bytes, at most 100/);
});

test('keywords: commas without spaces, nothing twice, nothing from the name, no other brands', () => {
  has(problems((l) => (l.apple['en-US'].keywords = 'brain, teaser')), /no spaces around the commas/);
  has(problems((l) => (l.apple['en-US'].keywords = 'brain,,teaser')), /an empty keyword/);
  has(problems((l) => (l.apple['en-US'].keywords = 'brain,Brain')), /"brain" twice/);
  has(problems((l) => (l.apple['en-US'].keywords = 'brain,daily puzzle')), /"daily puzzle": "daily" is in the name/);
  has(problems((l) => (l.apple['en-US'].keywords = 'brain,sudoku')), /"sudoku": no other brands/);
  has(problems((l) => (l.apple['en-US'].keywords = 'brain,games')), /category names/);
  // Without spaces between words: compared as text
  has(problems((l) => (l.apple['zh-Hans'].keywords = '益智,数桥')), /"数桥" is in the name or subtitle/);
});

test('names and titles: no emoji, promotion, other brands or shouting', () => {
  has(problems((l) => (l.play['en-US'].title = 'Hashi 🌉 Bridges')), /title: no emoji/);
  has(problems((l) => (l.apple['en-US'].name = 'Hashi: #1 Bridges')), /name: no promotional wording/);
  has(problems((l) => (l.apple['de-DE'].name = 'Hashi: Gratis Brückenrätsel')), /name: no promotional wording \(gratis\)/);
  has(problems((l) => (l.play['en-US'].title = 'Hashi: Best Bridges')), /no promotional wording/);
  has(problems((l) => (l.play['en-US'].title = 'Hashi: Nikoli Bridges')), /no other brands \(nikoli\)/);
  has(problems((l) => (l.play['en-US'].title = 'HASHI BRIDGES PUZZLE')), /no words in capitals/);
  has(problems((l) => (l.play['en-US'].short_description = 'Play now: bridges!')), /promotional wording in the short description/);
  has(problems((l) => (l.apple['en-US'].description += '\nAlso on Android.')), /mentions another platform \(android\)/);
  has(problems((l) => (l.play['en-US'].full_description += '\nAlso on the iPhone.')), /mentions another platform \(iphone\)/);
});

test('locales: every one there, none unknown, copies not kept twice', () => {
  has(problems((l) => delete l.apple['ko']), /App Store ko: missing/);
  has(problems((l) => (l.play['xx-YY'] = { ...l.play['en-US'] })), /xx-YY: a folder for a locale that store\/locales.mjs does not list/);
  has(problems((l) => (l.apple['en-GB'] = { ...l.apple['en-US'] })), /en-GB: takes the texts of en-US, remove its folder/);
  has(problems((l) => (l.play['th'].title = undefined)), /th: title.txt is missing or empty/);
  assert.ok(validate(repo, { play: [...PLAY, { locale: 'xx', site: 'en' }] }).some((p) => /xx: not a locale the store/.test(p)));
});

test('the URLs are the site in the listing language', () => {
  assert.equal(siteUrl('en', '/kontakt'), 'https://hashi.rs/kontakt');
  assert.equal(siteUrl('zh-Hant', '/'), 'https://hashi.rs/zh-Hant');
  assert.equal(repo.apple['de-DE'].privacy_url, 'https://hashi.rs/de/datenschutz');
  has(problems((l) => (l.apple['de-DE'].support_url = 'https://hashi.rs/kontakt')), /de-DE: support_url is https:\/\/hashi.rs\/kontakt, expected https:\/\/hashi.rs\/de\/kontakt/);
});

test('captions: every scene in every language, short', () => {
  has(problems((l) => delete l.captions.vi.night), /captions.json vi: no caption for night/);
  has(problems((l) => (l.captions.de.daily.title = 'x'.repeat(35))), /de: daily: caption over 34/);
  // The *accent* marks don't count
  assert.deepEqual(problems((l) => (l.captions.de.daily.title = `*${'x'.repeat(34)}*`)), []);
});

/** A PNG with nothing but its header and an empty image: enough for pngInfo. */
function png(width, height, colorType) {
  const chunk = (type, data) => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    return Buffer.concat([len, Buffer.from(type, 'latin1'), data, Buffer.alloc(4)]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8;
  ihdr[9] = colorType;
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(Buffer.alloc(0))),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

test('images: their size, and no alpha channel in the screenshots', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'store-png-'));
  try {
    const file = (name, ...args) => {
      const f = path.join(dir, name);
      fs.writeFileSync(f, png(...args));
      return f;
    };
    const rgb = file('rgb.png', 1320, 2868, 2);
    assert.deepEqual(pngInfo(rgb), { width: 1320, height: 2868, bitDepth: 8, colorType: 2 });
    assert.deepEqual(checkImage(rgb, { width: 1320, height: 2868 }), []);
    assert.match(checkImage(rgb, { width: 1080, height: 1920 })[0], /is 1320×2868, expected 1080×1920/);
    const rgba = file('rgba.png', 1024, 500, 6);
    assert.match(checkImage(rgba, { width: 1024, height: 500 })[0], /has an alpha channel/);
    const icon = file('icon.png', 512, 512, 6);
    assert.deepEqual(checkImage(icon, { width: 512, height: 512, alpha: 'required' }), []);
    assert.match(checkImage(rgb, { width: 1320, height: 2868, alpha: 'required' })[0], /32-bit PNG/);
    assert.match(checkImage(path.join(dir, 'gone.png'), { width: 1, height: 1 })[0], /is missing/);
    // The app icon Play gets
    assert.deepEqual(checkImage(new URL('../../public/icon-512.png', import.meta.url).pathname, { width: 512, height: 512, alpha: 'required' }), []);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});
