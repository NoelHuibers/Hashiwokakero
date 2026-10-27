// The stamps as Game Center and Play Games achievements, and the leaderboards (app/README.md,
// Game achievements): what store/achievements/apple.mjs and play.mjs upload, in every store
// locale of store/locales.mjs.
//
// - model.json: ids, points, steps, hidden, kanji. Written by the test in
//   src/model/achievements.rs from the Rust model the app reports with, never by hand.
// - locales/<language>/stamps.json: the stamp book's names and texts. A secret stamp's
//   description before it is earned is the book's neutral text (Game Center shows it to nobody
//   while the achievement is hidden, but wants one).
// - leaderboards.json: the leaderboards' names per site language.
//
//   node store/achievements/catalogue.mjs   checks it, writes target/store/achievements/
//                                           catalogue.json and, once the icons are there
//                                           (`pnpm run store:icons`), Play's import ZIP
//
// The stores' limits: Play's names ≤ 100 and descriptions ≤ 500 characters, names unique per
// locale, points multiples of 5 from 5 to 200 and ≤ 2000 together, steps ≤ 10000. Game Center's
// ids ≤ 100 single-byte characters, points ≤ 100 each and ≤ 1000 together, ≤ 100 achievements.
// Apple documents no length for the texts: they are held to 100 (names) and 255 characters
// (descriptions, App Store Connect's limit for Game Center's other text fields).
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { crc32 } from 'node:zlib';

import { APPLE, PLAY, SITE_LANGUAGES } from '../locales.mjs';

export const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
export const OUT = path.join(ROOT, 'target/store/achievements');
/** The icons (`pnpm run store:icons`): `<platform>/<id>.png`, achievements and leaderboards. */
export const ICONS = path.join(OUT, 'icons');
/** The IDs Play gave the achievements and leaderboards, which the Android app reports with. */
export const PLAY_IDS = path.join(ROOT, 'app/achievements-play.json');

export const ICON_SIZE = { apple: 1024, play: 512 };
export const LIMITS = {
  play: { name: 100, description: 500, points: [5, 200], total: 2000, steps: 10_000, count: 200, leaderboards: 70 },
  apple: { name: 100, description: 255, points: [5, 100], total: 1000, id: 100, count: 100, leaderboards: 100 },
};

const chars = (text) => [...text].length;
const readJson = (file) => JSON.parse(fs.readFileSync(file, 'utf8'));

/** The sources: the Rust model, the stamp book's texts of every site language, the names. */
export function loadSources(root = ROOT) {
  const model = readJson(path.join(root, 'store/achievements/model.json'));
  const stamps = Object.fromEntries(
    SITE_LANGUAGES.map((site) => [site, readJson(path.join(root, 'locales', site, 'stamps.json'))]),
  );
  const leaderboards = readJson(path.join(root, 'store/achievements/leaderboards.json'));
  return { model, stamps, leaderboards };
}

/** A stamp's name and text in a site language, and the book's text for a secret one. */
function texts(sources, site, stamp) {
  const book = sources.stamps[site];
  return {
    name: book.names?.[stamp]?.trim(),
    text: book.texts?.[stamp]?.trim(),
    secret: book.secret?.text?.trim(),
  };
}

/**
 * What goes to each platform, in the order of the stamp book:
 * `{ apple: { locales, achievements, leaderboards }, play: {...} }`.
 */
export function buildCatalogue(sources = loadSources()) {
  const { model } = sources;
  const apple = {
    locales: APPLE.map((l) => l.locale),
    achievements: model.achievements.map((a) => ({
      id: a.id,
      // App Store Connect's own name for it, in its lists
      referenceName: `${texts(sources, 'en', a.stamp).name} (${a.stamp})`,
      points: a.points,
      showBeforeEarned: !a.secret,
      repeatable: false,
      steps: a.steps,
      image: `apple/${a.id}.png`,
      localizations: Object.fromEntries(
        APPLE.map(({ locale, site }) => {
          const t = texts(sources, site, a.stamp);
          return [
            locale,
            { name: t.name, beforeEarnedDescription: a.secret ? t.secret : t.text, afterEarnedDescription: t.text },
          ];
        }),
      ),
    })),
    leaderboards: model.leaderboards.map((b) => ({
      id: b.id,
      referenceName: `${sources.leaderboards.en[b.id]} (${b.id})`,
      // The daily's time in hundredths of a second (GamesLeaderboard::apple_score), the counts
      // as they are
      defaultFormatter: b.lower_is_better ? 'ELAPSED_TIME_CENTISECOND' : 'INTEGER',
      scoreSortType: b.lower_is_better ? 'ASC' : 'DESC',
      submissionType: 'BEST_SCORE',
      // The daily's board starts over with the puzzle day, at midnight UTC
      recurrence: b.id === 'daily_time' ? { duration: 'P1D', rule: 'FREQ=DAILY;INTERVAL=1' } : null,
      image: `apple/${b.id}.png`,
      localizations: Object.fromEntries(APPLE.map(({ locale, site }) => [locale, { name: sources.leaderboards[site]?.[b.id] }])),
    })),
  };
  const play = {
    locales: PLAY.map((l) => l.locale),
    achievements: model.achievements.map((a) => ({
      id: a.id,
      achievementType: a.steps ? 'INCREMENTAL' : 'STANDARD',
      steps: a.steps,
      initialState: a.secret ? 'HIDDEN' : 'REVEALED',
      points: a.points,
      icon: `play/${a.id}.png`,
      // Play shows a hidden achievement's name and description only once it is unlocked
      name: Object.fromEntries(PLAY.map(({ locale, site }) => [locale, texts(sources, site, a.stamp).name])),
      description: Object.fromEntries(PLAY.map(({ locale, site }) => [locale, texts(sources, site, a.stamp).text])),
    })),
    leaderboards: model.leaderboards
      .filter((b) => b.play)
      .map((b) => ({
        id: b.id,
        scoreOrder: b.lower_is_better ? 'SMALLER_IS_BETTER' : 'LARGER_IS_BETTER',
        // Milliseconds are Play's TIME_DURATION
        scoreFormat: b.lower_is_better ? { numberFormatType: 'TIME_DURATION' } : { numberFormatType: 'NUMERIC', numDecimalPlaces: 0 },
        icon: `play/${b.id}.png`,
        name: Object.fromEntries(PLAY.map(({ locale, site }) => [locale, sources.leaderboards[site]?.[b.id]])),
      })),
  };
  return { apple, play };
}

/** Every problem of the catalogue, as messages; empty when everything is fine. */
export function validate(sources = loadSources()) {
  const errors = [];
  const fail = (where, message) => errors.push(`${where}: ${message}`);
  const { model } = sources;

  if (model.achievements.length === 0) fail('model.json', 'no achievements');
  const ids = new Set();
  for (const a of model.achievements) {
    if (ids.has(a.id)) fail('model.json', `${a.id} twice`);
    ids.add(a.id);
    if (a.id !== `stamp_${a.stamp}`) fail('model.json', `${a.id} is not stamp_${a.stamp}`);
    if (!/^[A-Za-z0-9_.]+$/.test(a.id) || a.id.length > LIMITS.apple.id) fail('model.json', `${a.id}: Game Center ids are ≤ 100 letters, digits, _ and .`);
    if (a.steps !== null && !(Number.isInteger(a.steps) && a.steps >= 2 && a.steps <= LIMITS.play.steps)) {
      fail('model.json', `${a.id}: ${a.steps} steps, 2 to ${LIMITS.play.steps}`);
    }
    for (const store of ['apple', 'play']) {
      const [min, max] = LIMITS[store].points;
      if (!(a.points % 5 === 0 && a.points >= min && a.points <= max)) fail(`${store} ${a.id}`, `${a.points} points, a multiple of 5 from ${min} to ${max}`);
    }
  }
  const total = model.achievements.reduce((sum, a) => sum + a.points, 0);
  for (const store of ['apple', 'play']) {
    if (total > LIMITS[store].total) fail(store, `${total} points, at most ${LIMITS[store].total}`);
    if (model.achievements.length > LIMITS[store].count) fail(store, `${model.achievements.length} achievements, at most ${LIMITS[store].count}`);
  }
  for (const b of model.leaderboards) {
    if (ids.has(b.id)) fail('model.json', `leaderboard ${b.id} has an achievement's id`);
    ids.add(b.id);
  }

  for (const site of SITE_LANGUAGES) {
    const book = sources.stamps[site];
    if (!book?.secret?.text) fail(`locales/${site}/stamps.json`, 'no secret.text');
    for (const a of model.achievements) {
      const t = texts(sources, site, a.stamp);
      if (!t.name) fail(`locales/${site}/stamps.json`, `no name for ${a.stamp}`);
      if (!t.text) fail(`locales/${site}/stamps.json`, `no text for ${a.stamp}`);
      if (/\{\{/.test(`${t.name}${t.text}`)) fail(`locales/${site}/stamps.json`, `${a.stamp}: a placeholder the stores would show as it is`);
    }
    for (const b of model.leaderboards) {
      if (!sources.leaderboards[site]?.[b.id]) fail('store/achievements/leaderboards.json', `no name for ${b.id} in ${site}`);
    }
  }
  if (errors.length > 0) return errors;

  const catalogue = buildCatalogue(sources);
  const { apple, play } = catalogue;
  for (const a of apple.achievements) {
    for (const [locale, l] of Object.entries(a.localizations)) {
      const where = `Game Center ${locale} ${a.id}`;
      if (chars(l.name) > LIMITS.apple.name) fail(where, `name over ${LIMITS.apple.name} characters`);
      for (const field of ['beforeEarnedDescription', 'afterEarnedDescription']) {
        if (chars(l[field]) > LIMITS.apple.description) fail(where, `${field} over ${LIMITS.apple.description} characters`);
      }
    }
  }
  for (const a of play.achievements) {
    for (const locale of play.locales) {
      const where = `Play Games ${locale} ${a.id}`;
      if (chars(a.name[locale]) > LIMITS.play.name) fail(where, `name over ${LIMITS.play.name} characters`);
      if (chars(a.description[locale]) > LIMITS.play.description) fail(where, `description over ${LIMITS.play.description} characters`);
    }
    // Play's import ZIP is CSV without quoting
    if (a.name['en-US'].includes(',')) fail(`Play Games en-US ${a.id}`, 'no comma in the name (the import file is CSV)');
  }
  // Play wants every name once per locale (Game Center doesn't, the same rule keeps them apart)
  for (const store of [apple, play]) {
    const label = store === apple ? 'Game Center' : 'Play Games';
    for (const locale of store.locales) {
      const seen = new Map();
      for (const a of store.achievements) {
        const name = store === apple ? a.localizations[locale].name : a.name[locale];
        if (seen.has(name)) fail(`${label} ${locale}`, `${a.id} and ${seen.get(name)} are both "${name}"`);
        seen.set(name, a.id);
      }
      const boards = new Map();
      for (const b of store.leaderboards) {
        const name = store === apple ? b.localizations[locale].name : b.name[locale];
        if (chars(name) > LIMITS[store === apple ? 'apple' : 'play'].name) fail(`${label} ${locale} ${b.id}`, 'name too long');
        if (boards.has(name)) fail(`${label} ${locale}`, `leaderboards ${b.id} and ${boards.get(name)} are both "${name}"`);
        boards.set(name, b.id);
      }
    }
  }
  if (play.leaderboards.length > LIMITS.play.leaderboards) fail('Play Games', 'too many leaderboards');
  return errors;
}

/** The IDs Play gave, `app/achievements-play.json`. */
export function loadPlayIds(file = PLAY_IDS) {
  const ids = fs.existsSync(file) ? readJson(file) : {};
  return { project_id: ids.project_id ?? null, achievements: ids.achievements ?? {}, leaderboards: ids.leaderboards ?? {} };
}

/** `app/achievements-play.json` as it is committed: keys in the order of the catalogue. */
export function formatPlayIds(ids, catalogue = buildCatalogue()) {
  const ordered = (items, map) =>
    Object.fromEntries(items.filter((x) => map[x.id]).map((x) => [x.id, map[x.id]]));
  return `${JSON.stringify(
    {
      project_id: ids.project_id ?? null,
      achievements: ordered(catalogue.play.achievements, ids.achievements),
      leaderboards: ordered(catalogue.play.leaderboards, ids.leaderboards),
    },
    null,
    2,
  )}\n`;
}

/**
 * Play Console's bulk import (Play Games Services, Achievements, Import achievements): the
 * English names, type, steps, state, points and order, and the icons, which Play's API can't
 * set. No descriptions: the file is CSV without quoting and they have commas; play.mjs adds
 * them and the translations afterwards.
 */
export function playImportFiles(catalogue, icons = ICONS) {
  const metadata = catalogue.play.achievements
    .map((a, i) =>
      [a.name['en-US'], '', a.steps ? 'True' : 'False', a.steps ?? '', a.initialState === 'HIDDEN' ? 'Hidden' : 'Revealed', a.points, i + 1].join(','),
    )
    .join('\n');
  const mappings = catalogue.play.achievements.map((a) => `${a.name['en-US']},${a.id}.png`).join('\n');
  return [
    ['AchievementsMetadata.csv', Buffer.from(`${metadata}\n`)],
    ['AchievementsIconsMappings.csv', Buffer.from(`${mappings}\n`)],
    ...catalogue.play.achievements.map((a) => [`${a.id}.png`, fs.readFileSync(path.join(icons, a.icon))]),
  ];
}

/** A ZIP of `files` ([name, bytes]), stored without compression (PNGs are compressed already). */
export function zip(files) {
  const local = [];
  const central = [];
  let offset = 0;
  for (const [name, data] of files) {
    const nameBytes = Buffer.from(name);
    const crc = crc32(data);
    const header = Buffer.alloc(30);
    header.writeUInt32LE(0x04034b50, 0);
    header.writeUInt16LE(20, 4); // version needed
    header.writeUInt16LE(0x0800, 6); // UTF-8 names
    header.writeUInt16LE(0, 8); // stored
    header.writeUInt32LE(0, 10); // time and date: 1980, the same bytes every run
    header.writeUInt32LE(crc, 14);
    header.writeUInt32LE(data.length, 18);
    header.writeUInt32LE(data.length, 22);
    header.writeUInt16LE(nameBytes.length, 26);
    header.writeUInt16LE(0, 28);
    local.push(header, nameBytes, data);
    const entry = Buffer.alloc(46);
    entry.writeUInt32LE(0x02014b50, 0);
    entry.writeUInt16LE(20, 4);
    entry.writeUInt16LE(20, 6);
    entry.writeUInt16LE(0x0800, 8);
    entry.writeUInt16LE(0, 10);
    entry.writeUInt32LE(0, 12);
    entry.writeUInt32LE(crc, 16);
    entry.writeUInt32LE(data.length, 20);
    entry.writeUInt32LE(data.length, 24);
    entry.writeUInt16LE(nameBytes.length, 28);
    entry.writeUInt32LE(offset, 42);
    central.push(entry, nameBytes);
    offset += header.length + nameBytes.length + data.length;
  }
  const centralSize = central.reduce((n, b) => n + b.length, 0);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(files.length, 8);
  end.writeUInt16LE(files.length, 10);
  end.writeUInt32LE(centralSize, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...local, ...central, end]);
}

/** Problems of the icons: every one there, at its platform's size, without alpha. */
export async function checkIcons(catalogue, icons = ICONS) {
  const { checkImage } = await import('../assemble.mjs');
  const problems = [];
  for (const item of [...catalogue.apple.achievements, ...catalogue.apple.leaderboards]) {
    problems.push(...checkImage(path.join(icons, item.image), { width: ICON_SIZE.apple, height: ICON_SIZE.apple }));
  }
  for (const item of [...catalogue.play.achievements, ...catalogue.play.leaderboards]) {
    problems.push(...checkImage(path.join(icons, item.icon), { width: ICON_SIZE.play, height: ICON_SIZE.play }));
  }
  return problems;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const sources = loadSources();
  const errors = validate(sources);
  if (errors.length > 0) {
    for (const e of errors) console.error(`✗ ${e}`);
    console.error(`\n${errors.length} problem(s) in the achievements`);
    process.exit(1);
  }
  const catalogue = buildCatalogue(sources);
  fs.mkdirSync(OUT, { recursive: true });
  fs.writeFileSync(path.join(OUT, 'catalogue.json'), `${JSON.stringify(catalogue, null, 2)}\n`);
  const points = sources.model.achievements.reduce((sum, a) => sum + a.points, 0);
  console.log(
    `Achievements OK: ${catalogue.apple.achievements.length} achievements (${points} points), ` +
      `Game Center ${catalogue.apple.leaderboards.length} and Play ${catalogue.play.leaderboards.length} leaderboards, ` +
      `${catalogue.apple.locales.length} and ${catalogue.play.locales.length} locales`,
  );
  if (fs.existsSync(ICONS)) {
    const problems = await checkIcons(catalogue);
    if (problems.length > 0) {
      console.error(`The icons have problems:\n${problems.join('\n')}`);
      process.exit(1);
    }
    fs.writeFileSync(path.join(OUT, 'play-import.zip'), zip(playImportFiles(catalogue)));
    console.log(`Icons OK, ${path.relative(ROOT, path.join(OUT, 'play-import.zip'))} written`);
  }
}
