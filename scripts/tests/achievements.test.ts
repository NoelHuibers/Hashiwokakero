// The game achievements (store/achievements): the catalogue in the repository passes the stores'
// limits and each check catches what it is there for, and the uploads against App Store Connect
// and Play Games Services in memory (store/achievements/fake.ts) create everything once, change
// only what changed and nothing on a second run.
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';

import {
  asArray,
  counting,
  dig,
  dryRun,
  httpClient,
  jwt,
  type Recording,
  tally,
} from '../../store/achievements/api.ts';
import { appleAnswer, appleEmpty, syncApple } from '../../store/achievements/apple.ts';
import {
  buildCatalogue,
  formatPlayIds,
  loadPlayIds,
  loadSources,
  PLAY_IDS,
  playImportFiles,
  type Sources,
  validate,
  zip,
} from '../../store/achievements/catalogue.ts';
import { fakeApple, fakePlay } from '../../store/achievements/fake.ts';
import { syncPlay } from '../../store/achievements/play.ts';
import { APPLE, PLAY } from '../../store/locales.ts';

const sources = loadSources();
const catalogue = buildCatalogue(sources);
const quiet = () => {
  // No log in the tests
};
const writesOf = (api: Recording) => api.writes.map((w) => `${w.method} ${w.path}`);
/** No writes (a short message otherwise: assert's diff of thousands of them takes minutes). */
const nothing = (api: Recording) =>
  assert.equal(
    api.writes.length,
    0,
    `${api.writes.length} writes:\n${writesOf(api).slice(0, 10).join('\n')}`,
  );
const temporary: string[] = [];

/** What must be there for the test to go on. */
function must<T>(value: T | undefined | null, what: string): T {
  assert.ok(value !== undefined && value !== null, `no ${what}`);
  return value;
}
/** The stamp book's names and texts in a site language. */
const names = (s: Sources, site: string) => must(s.stamps[site]?.names, `names in ${site}`);
const texts = (s: Sources, site: string) => must(s.stamps[site]?.texts, `texts in ${site}`);
/** The model's `i`th stamp. */
const stampAt = (s: Sources, i: number) => must(s.model.achievements[i], `stamp ${i}`);
test.after(() => {
  for (const dir of temporary) fs.rmSync(dir, { recursive: true, force: true });
});

/** Icons as the uploads read them, in a temporary directory: a few bytes per file. */
function icons(change: Record<string, string> = {}) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'hashi-icons-'));
  temporary.push(dir);
  for (const item of [...catalogue.apple.achievements, ...catalogue.apple.leaderboards]) {
    fs.mkdirSync(path.join(dir, 'apple'), { recursive: true });
    fs.writeFileSync(path.join(dir, item.image), change[item.id] ?? `icon of ${item.id}`);
  }
  for (const item of [...catalogue.play.achievements, ...catalogue.play.leaderboards]) {
    fs.mkdirSync(path.join(dir, 'play'), { recursive: true });
    fs.writeFileSync(path.join(dir, item.icon), `icon of ${item.id}`);
  }
  return dir;
}

/** The sources with one change, checked. */
function problems(change: (sources: Sources) => void) {
  const changed = structuredClone(sources);
  change(changed);
  return validate(changed);
}
const has = (list: string[], pattern: RegExp) =>
  assert.ok(
    list.some((e) => pattern.test(e)),
    `no problem like ${pattern} in:\n${list.join('\n')}`,
  );

test('the catalogue in the repository has no problems', () => {
  assert.deepEqual(validate(sources), []);
});

test('every stamp is an achievement in every store locale, the secrets hidden', () => {
  const stamps = sources.model.achievements;
  assert.equal(stamps.length, 48);
  assert.equal(catalogue.apple.achievements.length, 48);
  assert.equal(catalogue.play.achievements.length, 48);
  assert.deepEqual(
    catalogue.apple.locales,
    APPLE.map((l) => l.locale),
  );
  assert.deepEqual(
    catalogue.play.locales,
    PLAY.map((l) => l.locale),
  );
  const en = { names: names(sources, 'en'), texts: texts(sources, 'en') };
  const secret = sources.stamps.en?.secret?.text;
  for (const [i, a] of catalogue.apple.achievements.entries()) {
    const stamp = stampAt(sources, i);
    assert.equal(a.id, `stamp_${stamp.stamp}`);
    assert.equal(a.showBeforeEarned, !stamp.secret);
    assert.equal(a.repeatable, false);
    assert.deepEqual(Object.keys(a.localizations), catalogue.apple.locales);
    const l = must(a.localizations['en-US'], 'en-US');
    assert.equal(l.name, en.names[stamp.stamp]);
    assert.equal(l.afterEarnedDescription, en.texts[stamp.stamp]);
    // A secret stays one until it is earned
    assert.equal(l.beforeEarnedDescription, stamp.secret ? secret : en.texts[stamp.stamp]);
    // Copies of a language get its texts (fr-CA is fr-FR's French)
    assert.deepEqual(a.localizations['fr-CA'], a.localizations['fr-FR']);
    const p = must(catalogue.play.achievements[i], `Play's achievement ${i}`);
    assert.equal(p.initialState, stamp.secret ? 'HIDDEN' : 'REVEALED');
    assert.equal(p.achievementType, stamp.steps ? 'INCREMENTAL' : 'STANDARD');
    assert.equal(p.points, a.points);
    assert.equal(p.name['ja-JP'], names(sources, 'ja')[stamp.stamp]);
    assert.equal(p.description['zh-TW'], texts(sources, 'zh-Hant')[stamp.stamp]);
  }
  assert.ok(catalogue.apple.achievements.some((a) => !a.showBeforeEarned));
  // Counting stamps are incremental with the stamp book's target
  const play = (id: string) =>
    must(
      catalogue.play.achievements.find((a) => a.id === id),
      id,
    );
  assert.equal(play('stamp_solved_100').steps, 100);
  assert.equal(play('stamp_first').achievementType, 'STANDARD');
});

test('the daily time is a daily Game Center leaderboard in hundredths, and not on Play', () => {
  const daily = must(
    catalogue.apple.leaderboards.find((b) => b.id === 'daily_time'),
    'daily_time',
  );
  assert.equal(daily.defaultFormatter, 'ELAPSED_TIME_CENTISECOND');
  assert.equal(daily.scoreSortType, 'ASC');
  assert.deepEqual(daily.recurrence, { duration: 'PT24H', rule: 'FREQ=DAILY;INTERVAL=1' });
  assert.deepEqual(
    catalogue.play.leaderboards.map((b) => b.id),
    ['puzzles_solved', 'best_streak'],
  );
  for (const b of catalogue.play.leaderboards) assert.equal(b.scoreOrder, 'LARGER_IS_BETTER');
  assert.equal(catalogue.apple.leaderboards.find((b) => b.id === 'best_streak')?.recurrence, null);
});

test('each check catches what it is there for', () => {
  has(
    problems((s) => delete names(s, 'de').oni),
    /locales\/de\/stamps.json: no name for oni/,
  );
  has(
    problems((s) => (texts(s, 'fr').gold = 'x'.repeat(501))),
    /Play Games fr-FR stamp_gold: description over 500/,
  );
  has(
    problems((s) => (texts(s, 'fr').gold = 'x'.repeat(121))),
    /Game Center fr-FR stamp_gold: afterEarnedDescription over 120/,
  );
  has(
    problems((s) => (names(s, 'ko').gold = 'x'.repeat(101))),
    /Play Games ko-KR stamp_gold: name over 100/,
  );
  has(
    problems((s) => (names(s, 'en').gold = 'Gold, first')),
    /no comma in the name/,
  );
  has(
    problems((s) => (names(s, 'it').gold = must(names(s, 'it').podium, 'podium'))),
    /Play Games it-IT: stamp_gold and stamp_podium|stamp_podium and stamp_gold/,
  );
  has(
    problems((s) => (texts(s, 'en').gold = 'Be {{ place }}.')),
    /a placeholder/,
  );
  has(
    problems((s) => (stampAt(s, 0).points = 7)),
    /stamp_first: 7 points, a multiple of 5/,
  );
  has(
    problems((s) => (stampAt(s, 0).points = 150)),
    /apple stamp_first: 150 points/,
  );
  has(
    problems((s) => {
      for (const a of s.model.achievements) a.points = 25;
    }),
    /apple: 1200 points, at most 1000/,
  );
  has(
    problems((s) => (stampAt(s, 1).steps = 20_000)),
    /20000 steps/,
  );
  has(
    problems((s) => (stampAt(s, 1).id = 'stamp_first')),
    /stamp_first twice/,
  );
  has(
    problems((s) => (stampAt(s, 2).id = 'stamp-50')),
    /is not stamp_solved_50/,
  );
  has(
    problems((s) => delete must(s.leaderboards.th, 'th').best_streak),
    /no name for best_streak in th/,
  );
});

/** The files of a ZIP, read back from its central directory. */
function unzip(buffer: Buffer) {
  const end = buffer.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
  const count = buffer.readUInt16LE(end + 10);
  let at = buffer.readUInt32LE(end + 16);
  const files: Record<string, Buffer> = {};
  for (let i = 0; i < count; i++) {
    assert.equal(buffer.readUInt32LE(at), 0x02014b50);
    const size = buffer.readUInt32LE(at + 20);
    const nameLength = buffer.readUInt16LE(at + 28);
    const offset = buffer.readUInt32LE(at + 42);
    const name = buffer.toString('utf8', at + 46, at + 46 + nameLength);
    const local = offset + 30 + buffer.readUInt16LE(offset + 26);
    files[name] = buffer.subarray(local, local + size);
    at += 46 + nameLength;
  }
  return files;
}

test("Play's import ZIP: CSV without header, seven values a row, an icon per achievement", () => {
  const dir = icons();
  const files = unzip(zip(playImportFiles(catalogue, dir)));
  assert.equal(Object.keys(files).length, 2 + 48);
  const file = (name: string) => must(files[name], name).toString();
  const rows = file('AchievementsMetadata.csv').trim().split('\n');
  assert.equal(rows.length, 48);
  for (const row of rows) assert.equal(row.split(',').length, 7, row);
  assert.equal(rows[0], 'First bridge,,False,,Revealed,5,1');
  assert.equal(rows[1], '10 dailies,,True,10,Revealed,10,2');
  const leapDay = must(names(sources, 'en').leap_day, 'leap_day');
  const hidden = must(
    rows.find((r) => r.startsWith(leapDay)),
    'the leap day',
  );
  assert.match(hidden, /,Hidden,/);
  const mappings = file('AchievementsIconsMappings.csv').trim().split('\n');
  assert.equal(mappings[0], 'First bridge,stamp_first.png');
  assert.equal(file('stamp_first.png'), 'icon of stamp_first');
});

test('Game Center: everything once, then nothing', async () => {
  const fake = fakeApple();
  const dir = icons();
  const first = counting(fake);
  await syncApple({ api: first, catalogue, icons: dir, log: quiet });
  const counts = tally(first.writes);
  assert.equal(counts['POST /v2/gameCenterAchievements'], 48);
  assert.equal(counts['POST /v2/gameCenterAchievementLocalizations'], 48 * 26);
  assert.equal(counts['PATCH /v2/gameCenterAchievementImages/{id}'], 48 * 26);
  assert.equal(counts['POST /v2/gameCenterLeaderboards'], 3);
  assert.equal(counts['POST /v1/gameCenterDetails'], 1);
  assert.equal(counts['PATCH /v1/gameCenterAppVersions/{id}'], 1);
  // Every image arrived whole
  for (const image of fake.state.images.values())
    assert.equal(image.attributes.assetDeliveryState.state, 'COMPLETE');
  const component = (vendorId: string) =>
    must(
      [...fake.state.components.values()].find((c) => c.attributes.vendorIdentifier === vendorId),
      vendorId,
    ).attributes;
  assert.equal(component('stamp_leap_day').showBeforeEarned, false);
  const daily = component('daily_time');
  assert.equal(daily.recurrenceDuration, 'PT24H');
  assert.match(String(daily.recurrenceStartDate), /T00:00:00Z$/);

  const second = counting(fake);
  const result = await syncApple({ api: second, catalogue, icons: dir, log: quiet });
  nothing(second);
  assert.deepEqual(result, { notes: [], failures: [] });
});

test('Game Center: a changed text, points or icon changes only that', async () => {
  const fake = fakeApple();
  const dir = icons();
  await syncApple({ api: fake, catalogue, icons: dir, log: quiet });
  const changed = structuredClone(catalogue);
  must(changed.apple.achievements[0]?.localizations['de-DE'], 'de-DE').name = 'Erste Brücke!';
  must(changed.apple.achievements[1], 'the second').points = 15;
  const run = counting(fake);
  await syncApple({
    api: run,
    catalogue: changed,
    icons: icons({ stamp_gold: 'a new icon' }),
    log: quiet,
  });
  assert.deepEqual(tally(run.writes), {
    'PATCH /v2/gameCenterAchievementLocalizations/{id}': 1,
    'PATCH /v2/gameCenterAchievements/{id}': 1,
    // The gold stamp's new icon in all 26 locales: deleted, reserved, sent in two parts, committed
    'DELETE /v2/gameCenterAchievementImages/{id}': 26,
    'POST /v2/gameCenterAchievementImages': 26,
    'PUT (image upload)': 52,
    'PATCH /v2/gameCenterAchievementImages/{id}': 26,
  });
  assert.equal(dig(run.writes[0]?.body, 'data', 'attributes', 'name'), 'Erste Brücke!');
});

test('Game Center: a change of a live achievement goes into a new version', async () => {
  const fake = fakeApple();
  const dir = icons();
  await syncApple({ api: fake, catalogue, icons: dir, log: quiet });
  fake.release();
  // Live and unchanged: nothing
  const same = counting(fake);
  await syncApple({ api: same, catalogue, icons: dir, log: quiet });
  nothing(same);

  const changed = structuredClone(catalogue);
  must(changed.apple.achievements[3]?.localizations.ja, 'ja').afterEarnedDescription =
    'デイリーを100回クリアした。';
  const run = counting(fake);
  await syncApple({ api: run, catalogue: changed, icons: dir, log: quiet });
  const counts = tally(run.writes);
  assert.equal(counts['POST /v2/gameCenterAchievementVersions'], 1);
  // The new version is filled in completely
  assert.equal(counts['POST /v2/gameCenterAchievementLocalizations'], 26);
  assert.equal(counts['POST /v2/gameCenterAchievementImages'], 26);
  const again = counting(fake);
  await syncApple({ api: again, catalogue: changed, icons: dir, log: quiet });
  nothing(again);

  // Points can't change once live: App Store Connect says no, and the run fails
  must(changed.apple.achievements[3], 'the fourth').points = 25;
  const { failures } = await syncApple({ api: fake, catalogue: changed, icons: dir, log: quiet });
  assert.equal(failures.length, 1);
  assert.match(failures[0] ?? '', /^achievement stamp_solved_100: .*409/);
});

test('Game Center: without the icons nothing is changed', async () => {
  const fake = counting(fakeApple());
  const dir = icons();
  fs.rmSync(path.join(dir, 'apple', 'stamp_oni.png'));
  await assert.rejects(
    syncApple({ api: fake, catalogue, icons: dir, log: quiet }),
    /no icons in .* for stamp_oni: `pnpm run store:icons` first/,
  );
  nothing(fake);
});

test('Game Center: no version being prepared is a note, not a failure', async () => {
  const fake = fakeApple({
    versions: [{ id: 'asv-0', versionString: '1.0', state: 'READY_FOR_DISTRIBUTION' }],
  });
  const { notes } = await syncApple({ api: fake, catalogue, icons: icons(), log: quiet });
  assert.match(notes.join('\n'), /No iOS version is being prepared/);
  assert.equal(fake.state.gcAppVersions.size, 0);
  assert.ok(fake.state.detail);
});

test('Game Center: one that is refused fails the run, after the others', async () => {
  const fake = fakeApple();
  // App Store Connect turns down the daily leaderboard's recurrence
  const refusing = {
    ...fake,
    post: async (path: string, body: unknown) => {
      if (dig(body, 'data', 'attributes', 'vendorIdentifier') === 'daily_time')
        throw new Error('422 recurrenceRule is invalid');
      return fake.post(path, body);
    },
  };
  const { failures } = await syncApple({ api: refusing, catalogue, icons: icons(), log: quiet });
  assert.deepEqual(failures, ['leaderboard daily_time: 422 recurrenceRule is invalid']);
  const made = [...fake.state.components.values()].map((c) => c.attributes.vendorIdentifier);
  assert.equal(made.length, 48 + 2);
  assert.ok(made.includes('best_streak') && !made.includes('daily_time'));
});

test('Game Center dry run: reads only, writes nothing', async () => {
  const fake = fakeApple();
  const dir = icons();
  await syncApple({ api: fake, catalogue, icons: dir, log: quiet });
  const changed = structuredClone(catalogue);
  must(changed.apple.achievements[0]?.localizations['en-US'], 'en-US').name = 'First bridge!';
  const before = JSON.stringify([...fake.state.localizations.values()]);
  const dry = dryRun(fake, { answer: appleAnswer, empty: appleEmpty });
  await syncApple({ api: dry, catalogue: changed, icons: dir, log: quiet });
  assert.deepEqual(tally(dry.writes), { 'PATCH /v2/gameCenterAchievementLocalizations/{id}': 1 });
  assert.equal(JSON.stringify([...fake.state.localizations.values()]), before);
  // Against nothing at all it plans everything, without reading below what it would create
  const empty = dryRun(fakeApple(), { answer: appleAnswer, empty: appleEmpty });
  await syncApple({ api: empty, catalogue, icons: dir, log: quiet });
  assert.equal(tally(empty.writes)['POST /v2/gameCenterAchievements'], 48);
  assert.equal(tally(empty.writes)['POST /v2/gameCenterAchievementVersions'], undefined);
});

test('Play Games: everything once with its IDs, then nothing', async () => {
  const fake = fakePlay();
  const first = counting(fake);
  const { ids } = await syncPlay({
    api: first,
    catalogue,
    ids: loadPlayIds(),
    projectId: '1234',
    log: quiet,
  });
  assert.deepEqual(tally(first.writes), {
    'POST /applications/{id}/achievements': 48,
    'POST /applications/{id}/leaderboards': 2,
  });
  assert.equal(Object.keys(ids.achievements).length, 48);
  assert.match(ids.achievements.stamp_first ?? '', /^CgkI/);
  assert.deepEqual(Object.keys(ids.leaderboards), ['puzzles_solved', 'best_streak']);
  const achievement = (stamp: string) =>
    must(fake.state.achievements.get(must(ids.achievements[stamp], stamp)), `Play's ${stamp}`);
  const made = achievement('stamp_solved_365');
  assert.equal(made.achievementType, 'INCREMENTAL');
  assert.equal(made.stepsToUnlock, 365);
  assert.equal(dig(made, 'draft', 'pointValue'), 40);
  assert.equal(asArray(dig(made, 'draft', 'name', 'translations')).length, 25);
  assert.equal(achievement('stamp_palindrome').initialState, 'HIDDEN');
  assert.equal(achievement('stamp_first').stepsToUnlock, undefined);

  // With the IDs, and without them (found by the English name, as after the console's import)
  for (const known of [ids, loadPlayIds('/nonexistent')]) {
    const again = counting(fake);
    const result = await syncPlay({
      api: again,
      catalogue,
      ids: known,
      projectId: '1234',
      log: quiet,
    });
    nothing(again);
    assert.deepEqual(result.ids, ids);
  }
});

test('Play Games: a change is one PUT; what publishing fixes is a note and an error', async () => {
  const fake = fakePlay();
  const { ids } = await syncPlay({
    api: fake,
    catalogue,
    ids: loadPlayIds(),
    projectId: '1234',
    log: quiet,
  });
  const changed = structuredClone(catalogue);
  const first = must(changed.play.achievements[0], 'the first');
  first.description['de-DE'] = 'Löse dein erstes Tagesrätsel!';
  const run = counting(fake);
  await syncPlay({ api: run, catalogue: changed, ids, projectId: '1234', log: quiet });
  assert.deepEqual(writesOf(run), [`PUT /achievements/${ids.achievements.stamp_first}`]);
  assert.equal(
    dig(
      asArray(dig(run.writes[0]?.body, 'draft', 'description', 'translations')).find(
        (t) => dig(t, 'locale') === 'de-DE',
      ),
      'value',
    ),
    'Löse dein erstes Tagesrätsel!',
  );

  fake.publish();
  first.initialState = 'HIDDEN';
  const result = await syncPlay({
    api: fake,
    catalogue: changed,
    ids,
    projectId: '1234',
    log: quiet,
  });
  assert.match(result.notes.join('\n'), /stamp_first is published: Play keeps its hidden/);
  assert.equal(result.failures.length, 1);
  assert.match(result.failures[0] ?? '', /can't change once published/);
  // The IDs still come out whole
  assert.deepEqual(result.ids, ids);
});

test('app/achievements-play.json is in its committed form', () => {
  const committed = fs.readFileSync(PLAY_IDS, 'utf8');
  assert.equal(formatPlayIds(loadPlayIds(), catalogue), committed);
  const ids = loadPlayIds();
  for (const id of Object.keys(ids.achievements))
    assert.ok(
      catalogue.play.achievements.some((a) => a.id === id),
      id,
    );
  for (const id of Object.keys(ids.leaderboards))
    assert.ok(
      catalogue.play.leaderboards.some((b) => b.id === id),
      id,
    );
});

test('the client retries when told to slow down, and signs its tokens', async () => {
  const answers = [
    new Response('slow down', { status: 429, headers: { 'retry-after': '3' } }),
    new Response('{"data":[]}', { status: 200 }),
  ];
  const waits: number[] = [];
  const seen: [string, string | undefined][] = [];
  const api = httpClient({
    base: 'https://api.example',
    token: async () => 'tok',
    fetchImpl: async (url, init) => {
      seen.push([url, init.headers.authorization]);
      return must(answers.shift(), 'an answer');
    },
    wait: async (ms) => waits.push(ms),
    log: quiet,
  });
  assert.deepEqual(await api.get('/v1/x'), { data: [] });
  assert.deepEqual(waits, [3000]);
  assert.deepEqual(seen, [
    ['https://api.example/v1/x', 'Bearer tok'],
    ['https://api.example/v1/x', 'Bearer tok'],
  ]);

  const failing = httpClient({
    base: 'https://api.example',
    token: async () => 't',
    fetchImpl: async () => new Response('{"errors":[1]}', { status: 409 }),
    log: quiet,
  });
  await assert.rejects(failing.post('/v1/y', {}), /POST https:\/\/api.example\/v1\/y: 409/);

  // App Store Connect's ES256, Google's RS256
  const ec = crypto.generateKeyPairSync('ec', { namedCurve: 'P-256' });
  const token = jwt({
    alg: 'ES256',
    key: ec.privateKey,
    kid: 'KEY',
    payload: { iss: 'me', aud: 'appstoreconnect-v1' },
  });
  const [h = '', p = '', s = ''] = token.split('.');
  assert.deepEqual(JSON.parse(Buffer.from(h, 'base64url').toString()), {
    alg: 'ES256',
    typ: 'JWT',
    kid: 'KEY',
  });
  assert.equal(JSON.parse(Buffer.from(p, 'base64url').toString()).aud, 'appstoreconnect-v1');
  assert.ok(
    crypto.verify(
      'sha256',
      Buffer.from(`${h}.${p}`),
      { key: ec.publicKey, dsaEncoding: 'ieee-p1363' },
      Buffer.from(s, 'base64url'),
    ),
  );
  const rsa = crypto.generateKeyPairSync('rsa', { modulusLength: 2048 });
  const [gh = '', gp = '', gs = ''] = jwt({
    alg: 'RS256',
    key: rsa.privateKey,
    payload: { iss: 'sa' },
  }).split('.');
  assert.ok(
    crypto.verify(
      'sha256',
      Buffer.from(`${gh}.${gp}`),
      rsa.publicKey,
      Buffer.from(gs, 'base64url'),
    ),
  );
});
