// The store listings' checks (store/validate.ts, store/assemble.ts): the repository's listings
// pass, and each rule catches what it is there for.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { deflateSync } from 'node:zlib';

import {
  checkImage,
  pngInfo,
  RELEASE_NOTES_LIMIT,
  releaseNotesFile,
  writeReleaseNotes,
} from '../../store/assemble.ts';
import { APPLE, PLAY, SITE_LANGUAGES, siteUrl } from '../../store/locales.ts';

/** The default order of sort(), as a comparison */
const byCodeUnits = (a: string, b: string) => (a < b ? -1 : 1);

import {
  APPLE_OTHER_PLATFORMS,
  type Listings,
  loadListings,
  mentions,
  validate,
} from '../../store/validate.ts';

const repo = loadListings();

/** record[key], which the test expects to be there. */
function at<T>(record: Record<string, T>, key: string): T {
  const value = record[key];
  assert.ok(value !== undefined, `no ${key}`);
  return value;
}

test('the listings in the repository have no problems', () => {
  assert.deepEqual(validate(repo), []);
});

test('every language of the site has a listing in both stores, in its own words', () => {
  const sites = fs.readdirSync(new URL('../../locales', import.meta.url)).sort();
  assert.deepEqual([...SITE_LANGUAGES].sort(byCodeUnits), sites);
  for (const site of SITE_LANGUAGES) {
    assert.ok(
      PLAY.some((l) => l.site === site && !l.same),
      `Google Play in ${site}`,
    );
    assert.ok(
      APPLE.some((l) => l.site === site && !l.same),
      `App Store in ${site}`,
    );
  }
  // Not English copied into the others
  const en = at(repo.apple, 'en-US').description;
  for (const [locale, listing] of Object.entries(repo.apple)) {
    if (locale !== 'en-US') assert.notEqual(listing.description, en, locale);
  }
});

/** The repository's listings with one change, checked. */
function problems(change: (listings: Listings) => void) {
  const listings = structuredClone(repo);
  change(listings);
  return validate(listings);
}

const has = (list: string[], pattern: RegExp) =>
  assert.ok(
    list.some((p) => pattern.test(p)),
    `expected ${pattern} in:\n${list.join('\n')}`,
  );

test('the stores limits', () => {
  has(
    problems((l) => (at(l.play, 'en-US').title = 'Hashi: Daily Bridges Logic Puzzle')),
    /en-US: title has 33 characters, at most 30/,
  );
  has(
    problems((l) => (at(l.play, 'de-DE').short_description = 'x'.repeat(81))),
    /de-DE: short_description has 81/,
  );
  has(
    problems((l) => (at(l.play, 'ja-JP').full_description = '橋'.repeat(4001))),
    /ja-JP: full_description has 4001/,
  );
  has(
    problems((l) => (at(l.apple, 'en-US').subtitle = 'Hashiwokakero Logic Puzzle Game')),
    /en-US: subtitle has 31/,
  );
  has(
    problems((l) => (at(l.apple, 'fr-FR').promotional_text = 'é'.repeat(171))),
    /fr-FR: promotional_text has 171/,
  );
  // Keywords count bytes: 34 kanji are 102
  has(
    problems((l) => (at(l.apple, 'ja').keywords = '脳'.repeat(34))),
    /ja: keywords has 102 bytes, at most 100/,
  );
});

test('keywords: commas without spaces, nothing twice, nothing from the name, no other brands', () => {
  has(
    problems((l) => (at(l.apple, 'en-US').keywords = 'brain, teaser')),
    /no spaces around the commas/,
  );
  has(
    problems((l) => (at(l.apple, 'en-US').keywords = 'brain,,teaser')),
    /an empty keyword/,
  );
  has(
    problems((l) => (at(l.apple, 'en-US').keywords = 'brain,Brain')),
    /"brain" twice/,
  );
  has(
    problems((l) => (at(l.apple, 'en-US').keywords = 'brain,daily puzzle')),
    /"daily puzzle": "daily" is in the name/,
  );
  has(
    problems((l) => (at(l.apple, 'en-US').keywords = 'brain,sudoku')),
    /"sudoku": no other brands/,
  );
  has(
    problems((l) => (at(l.apple, 'en-US').keywords = 'brain,games')),
    /category names/,
  );
  // Without spaces between words: compared as text
  has(
    problems((l) => (at(l.apple, 'zh-Hans').keywords = '益智,数桥')),
    /"数桥" is in the name or subtitle/,
  );
});

test('names and titles: no emoji, promotion, other brands or shouting', () => {
  has(
    problems((l) => (at(l.play, 'en-US').title = 'Hashi 🌉 Bridges')),
    /title: no emoji/,
  );
  has(
    problems((l) => (at(l.apple, 'en-US').name = 'Hashi: #1 Bridges')),
    /name: no promotional wording/,
  );
  has(
    problems((l) => (at(l.apple, 'de-DE').name = 'Hashi: Gratis Brückenrätsel')),
    /name: no promotional wording \(gratis\)/,
  );
  has(
    problems((l) => (at(l.play, 'en-US').title = 'Hashi: Best Bridges')),
    /no promotional wording/,
  );
  has(
    problems((l) => (at(l.play, 'en-US').title = 'Hashi: Nikoli Bridges')),
    /no other brands \(nikoli\)/,
  );
  has(
    problems((l) => (at(l.play, 'en-US').title = 'HASHI BRIDGES PUZZLE')),
    /no words in capitals/,
  );
  has(
    problems((l) => (at(l.play, 'en-US').short_description = 'Play now: bridges!')),
    /promotional wording in the short description/,
  );
  has(
    problems((l) => (at(l.apple, 'en-US').description += '\nAlso on Android.')),
    /mentions another platform \(android\)/,
  );
  has(
    problems((l) => (at(l.play, 'en-US').full_description += '\nAlso on the iPhone.')),
    /mentions another platform \(iphone\)/,
  );
  has(
    problems((l) => (at(l.apple, 'de-DE').promotional_text = 'Mit Play Games-Erfolgen.')),
    /de-DE: mentions another platform \(play games\)/,
  );
  assert.deepEqual(mentions('Now with achievements in Google Play Games.', APPLE_OTHER_PLATFORMS), [
    'google play',
    'play games',
  ]);
  assert.deepEqual(mentions('Puzzles to play, games to share.', APPLE_OTHER_PLATFORMS), []);
});

/** A whatsnew folder with these files and an App Store metadata tree, for the release notes. */
function withNotes(files: Record<string, string>, run: (whatsnew: string, meta: string) => void) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'store-notes-'));
  try {
    const whatsnew = path.join(dir, 'whatsnew');
    const meta = path.join(dir, 'metadata');
    fs.mkdirSync(whatsnew);
    for (const [name, text] of Object.entries(files))
      fs.writeFileSync(path.join(whatsnew, name), text);
    for (const { locale } of APPLE) fs.mkdirSync(path.join(meta, locale), { recursive: true });
    run(whatsnew, meta);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

const notesOf = (meta: string, locale: string) => {
  const file = path.join(meta, locale, 'release_notes.txt');
  return fs.existsSync(file) ? fs.readFileSync(file, 'utf8') : undefined;
};

test("release notes: each App Store locale's own, Play's of its language, or English", () => {
  const apple = (locale: string) => {
    const l = APPLE.find((a) => a.locale === locale);
    assert.ok(l, locale);
    return l;
  };
  withNotes(
    {
      'whatsnew-en-US': 'Hashi 1.2: a pause button.\n',
      'whatsnew-de-DE': 'Hashi 1.2: eine Pause-Taste.',
      'whatsnew-it-IT': 'Hashi 1.2: un tasto di pausa.',
      'whatsnew-es-ES': 'Hashi 1.2: un botón de pausa.',
      'whatsnew-zh-Hans': 'Hashi 1.2：暂停按钮。',
    },
    (whatsnew, meta) => {
      const file = (locale: string) =>
        path.basename(releaseNotesFile(whatsnew, apple(locale)) ?? '');
      assert.equal(file('de-DE'), 'whatsnew-de-DE');
      // Play's code of the language: it-IT for the App Store's it
      assert.equal(file('it'), 'whatsnew-it-IT');
      // Its own file before Play's (zh-CN)
      assert.equal(file('zh-Hans'), 'whatsnew-zh-Hans');
      // Play has two Spanish listings (es-ES, es-419): es-MX isn't guessed
      assert.equal(file('es-MX'), 'whatsnew-en-US');
      assert.equal(file('en-GB'), 'whatsnew-en-US');
      assert.equal(file('ja'), 'whatsnew-en-US');

      const { written, notes } = writeReleaseNotes(meta, { whatsnew });
      assert.equal(written, APPLE.length);
      assert.deepEqual(notes, []);
      assert.equal(notesOf(meta, 'en-US'), 'Hashi 1.2: a pause button.');
      assert.equal(notesOf(meta, 'en-AU'), 'Hashi 1.2: a pause button.');
      assert.equal(notesOf(meta, 'it'), 'Hashi 1.2: un tasto di pausa.');
      assert.equal(notesOf(meta, 'es-ES'), 'Hashi 1.2: un botón de pausa.');
      assert.equal(notesOf(meta, 'ko'), 'Hashi 1.2: a pause button.');
    },
  );
  // No notes at all, or an empty file: nothing, App Store Connect's text stays
  withNotes({}, (whatsnew, meta) => {
    assert.equal(releaseNotesFile(whatsnew, apple('en-US')), undefined);
    assert.deepEqual(writeReleaseNotes(meta, { whatsnew }), { written: 0, notes: [] });
  });
  withNotes({ 'whatsnew-en-US': '\n' }, (whatsnew, meta) => {
    assert.deepEqual(writeReleaseNotes(meta, { whatsnew }), { written: 0, notes: [] });
    assert.equal(notesOf(meta, 'en-US'), undefined);
  });
});

test('release notes: none where they name another platform, refused over 4000 characters', () => {
  withNotes(
    {
      'whatsnew-en-US': 'Hashi 1.1: your stamps are achievements in Google Play Games.',
      'whatsnew-de-DE': 'Hashi 1.1: neue Gemeinschaftsregeln.',
    },
    (whatsnew, meta) => {
      const { written, notes } = writeReleaseNotes(meta, { whatsnew });
      assert.equal(written, 1);
      assert.equal(notesOf(meta, 'de-DE'), 'Hashi 1.1: neue Gemeinschaftsregeln.');
      assert.equal(notesOf(meta, 'en-US'), undefined);
      assert.equal(notesOf(meta, 'fr-FR'), undefined);
      assert.equal(notes.length, 1);
      assert.match(notes[0] ?? '', /whatsnew-en-US names google play, play games/);
      assert.match(notes[0] ?? '', /stays as it is in en-US, en-GB, en-AU, en-CA, fr-FR, /);
      assert.doesNotMatch(notes[0] ?? '', /de-DE/);
    },
  );
  withNotes({ 'whatsnew-en-US': 'Now on Android too.' }, (whatsnew, meta) => {
    assert.match(
      writeReleaseNotes(meta, { whatsnew }).notes[0] ?? '',
      /names android: .* in every locale$/,
    );
  });
  // Characters, not bytes: 4000 Japanese ones fit
  withNotes({ 'whatsnew-ja-JP': 'あ'.repeat(RELEASE_NOTES_LIMIT) }, (whatsnew, meta) => {
    writeReleaseNotes(meta, { whatsnew });
    assert.equal([...(notesOf(meta, 'ja') ?? '')].length, RELEASE_NOTES_LIMIT);
  });
  withNotes({ 'whatsnew-en-US': 'x'.repeat(RELEASE_NOTES_LIMIT + 1) }, (whatsnew, meta) => {
    assert.throws(
      () => writeReleaseNotes(meta, { whatsnew }),
      /whatsnew-en-US has 4001 characters, the App Store takes 4000/,
    );
  });
});

test('locales: every one there, none unknown, copies not kept twice', () => {
  has(
    problems((l) => delete l.apple.ko),
    /App Store ko: missing/,
  );
  has(
    problems((l) => (l.play['xx-YY'] = { ...at(l.play, 'en-US') })),
    /xx-YY: a folder for a locale that store\/locales.ts does not list/,
  );
  has(
    problems((l) => (l.apple['en-GB'] = { ...at(l.apple, 'en-US') })),
    /en-GB: takes the texts of en-US, remove its folder/,
  );
  has(
    problems((l) => (at(l.play, 'th').title = undefined)),
    /th: title.txt is missing or empty/,
  );
  assert.ok(
    validate(repo, { play: [...PLAY, { locale: 'xx', site: 'en' }] }).some((p) =>
      /xx: not a locale the store/.test(p),
    ),
  );
});

test('the URLs are the site in the listing language', () => {
  assert.equal(siteUrl('en', '/kontakt'), 'https://hashi.rs/kontakt');
  assert.equal(siteUrl('zh-Hant', '/'), 'https://hashi.rs/zh-Hant');
  assert.equal(at(repo.apple, 'de-DE').privacy_url, 'https://hashi.rs/de/datenschutz');
  has(
    problems((l) => (at(l.apple, 'de-DE').support_url = 'https://hashi.rs/kontakt')),
    /de-DE: support_url is https:\/\/hashi.rs\/kontakt, expected https:\/\/hashi.rs\/de\/kontakt/,
  );
});

test('captions: every scene in every language, short', () => {
  has(
    problems((l) => delete at(l.captions, 'vi').night),
    /captions.json vi: no caption for night/,
  );
  has(
    problems((l) => (at(at(l.captions, 'de'), 'daily').title = 'x'.repeat(35))),
    /de: daily: caption over 34/,
  );
  // The *accent* marks don't count
  assert.deepEqual(
    problems((l) => (at(at(l.captions, 'de'), 'daily').title = `*${'x'.repeat(34)}*`)),
    [],
  );
});

/** A PNG with nothing but its header and an empty image: enough for pngInfo. */
function png(width: number, height: number, colorType: number) {
  const chunk = (type: string, data: Buffer) => {
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
    const file = (name: string, width: number, height: number, colorType: number) => {
      const f = path.join(dir, name);
      fs.writeFileSync(f, png(width, height, colorType));
      return f;
    };
    const rgb = file('rgb.png', 1320, 2868, 2);
    assert.deepEqual(pngInfo(rgb), { width: 1320, height: 2868, bitDepth: 8, colorType: 2 });
    assert.deepEqual(checkImage(rgb, { width: 1320, height: 2868 }), []);
    assert.match(
      checkImage(rgb, { width: 1080, height: 1920 })[0] ?? '',
      /is 1320×2868, expected 1080×1920/,
    );
    const rgba = file('rgba.png', 1024, 500, 6);
    assert.match(checkImage(rgba, { width: 1024, height: 500 })[0] ?? '', /has an alpha channel/);
    const icon = file('icon.png', 512, 512, 6);
    assert.deepEqual(checkImage(icon, { width: 512, height: 512, alpha: 'required' }), []);
    assert.match(
      checkImage(rgb, { width: 1320, height: 2868, alpha: 'required' })[0] ?? '',
      /32-bit PNG/,
    );
    assert.match(
      checkImage(path.join(dir, 'gone.png'), { width: 1, height: 1 })[0] ?? '',
      /is missing/,
    );
    // The app icon Play gets
    assert.deepEqual(
      checkImage(new URL('../../public/icon-512.png', import.meta.url).pathname, {
        width: 512,
        height: 512,
        alpha: 'required',
      }),
      [],
    );
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});
