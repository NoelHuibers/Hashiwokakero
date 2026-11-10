// node --test scripts/tests/: the Amazon Appstore upload (scripts/amazon-appstore.ts), against a
// fake of Amazon's App Submission API that keeps the app's edit, its ETags among it
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { after, test } from 'node:test';
import { PLAY } from '../../store/locales.ts';
import {
  AMAZON,
  AmazonError,
  accessToken,
  type Binary,
  type Connection,
  editOf,
  errorMessage,
  featureBullets,
  firstParagraph,
  imageCount,
  LIMITS,
  type ListingText,
  listing,
  listingBody,
  listingTexts,
  planBinaries,
  planListing,
  release,
  releaseNotes,
  SCOPE,
  screenshotProblems,
  textProblems,
} from '../amazon-appstore.ts';
import { apks } from '../galaxy-store.ts';

const API = 'https://developer.amazon.com/api/appstore/v1';
const TOKEN_URL = 'https://api.amazon.com/auth/o2/token';
const APP = 'amzn1.devportal.mobileapp.0123456789abcdef0123456789abcdef';
// 1.2.0
const CODE = 1_002_000;

const binary = (id: string, versionCode: number, name = `APK${id}`): Binary => ({
  id,
  versionCode,
  name,
});
const list = () =>
  apks(
    [
      'hashi-1.2.0-foss-armeabi-v7a.apk',
      'hashi-1.2.0-foss-arm64-v8a.apk',
      'hashi-1.2.0-foss-x86_64.apk',
    ],
    '1.2.0',
    CODE,
  );

test("the edit's binaries: each replaced by its ABI's APK, the rest stays out", () => {
  const plan = planBinaries([binary('a', 10_010_002), binary('b', 10_010_001)], list(), CODE);
  assert.deepEqual(
    plan.replace.map(({ binary: b, apk }) => [b.id, apk.abi, apk.versionCode]),
    [
      ['a', 'arm64-v8a', 10_020_002],
      ['b', 'armeabi-v7a', 10_020_001],
    ],
  );
  assert.deepEqual(plan.current, []);
  assert.deepEqual(
    plan.unused.map((a) => a.abi),
    ['x86_64'],
  );
  // One binary for every Fire tablet: only the 32-bit APK goes
  const one = planBinaries([binary('a', 10_010_001)], list(), CODE);
  assert.deepEqual(
    one.replace.map(({ apk }) => apk.abi),
    ['armeabi-v7a'],
  );
  assert.deepEqual(
    one.unused.map((a) => a.abi),
    ['x86_64', 'arm64-v8a'],
  );
  // A re-run: arm64 is this version already
  const again = planBinaries([binary('a', 10_020_002), binary('b', 10_010_001)], list(), CODE);
  assert.deepEqual(
    again.current.map(({ binary: b }) => b.id),
    ['a'],
  );
  assert.deepEqual(
    again.replace.map(({ binary: b }) => b.id),
    ['b'],
  );
});

test('refuses binaries it would get wrong: newer, not foss, twice, an ABI without its APK, none', () => {
  assert.throws(
    () => planBinaries([binary('a', 10_030_002)], list(), CODE),
    /newer version: the binary APKa \(versionCode 10030002\)/,
  );
  // Play's universal APK, uploaded by hand
  assert.throws(
    () => planBinaries([binary('a', 1_001_000)], list(), CODE),
    /isn't a foss APK.*Developer Console/,
  );
  assert.throws(
    () => planBinaries([binary('a', 10_010_002), binary('b', 10_010_002)], list(), CODE),
    /two binaries of arm64-v8a/,
  );
  assert.throws(
    () =>
      planBinaries(
        [binary('a', 10_010_001)],
        apks(['hashi-1.2.0-foss-arm64-v8a.apk'], '1.2.0', CODE),
        CODE,
      ),
    /a binary of armeabi-v7a .*no armeabi-v7a APK/,
  );
  assert.throws(
    () => planBinaries([], list(), CODE),
    /first version is submitted in the Developer/,
  );
});

test('release notes as the recent changes, unless they name another store', () => {
  assert.deepEqual(releaseNotes('Hashi 1.2: rivals', '1.2.0'), { text: 'Hashi 1.2: rivals' });
  assert.deepEqual(releaseNotes('', '1.2.0'), { text: 'Hashi 1.2.0' });
  const play = releaseNotes('Hashi 1.1: achievements in Google Play Games', '1.1.0');
  assert.equal(play.text, 'Hashi 1.1.0');
  assert.match(play.note ?? '', /name Google Play Games/);
  assert.throws(() => releaseNotes('x'.repeat(4001), '1.2.0'), /4001 characters/);
});

test("Amazon's error bodies", () => {
  assert.equal(
    errorMessage(403, { message: 'Request is not authorized.' }),
    'Request is not authorized.',
  );
  assert.equal(
    errorMessage(400, {
      error: 'invalid_client',
      error_description: 'Client authentication failed',
    }),
    'Client authentication failed',
  );
  assert.equal(errorMessage(400, { error: 'invalid_scope' }), 'invalid_scope');
  assert.equal(
    errorMessage(403, { errors: [{ errorMessage: 'No screenshots' }, { message: 'No icon' }] }),
    'No screenshots; No icon',
  );
  assert.equal(errorMessage(500, '<html>oops</html>'), '<html>oops</html>');
  assert.equal(errorMessage(500, null), 'HTTP 500');
  assert.equal(errorMessage(400, { code: 7 }), '{"code":7}');
  assert.deepEqual(editOf({ id: 'e1', status: 'IN_PROGRESS' }), {
    id: 'e1',
    status: 'IN_PROGRESS',
  });
  assert.equal(editOf({}), null);
  assert.equal(editOf(null), null);
  assert.equal(imageCount([{ id: 'a' }]), 1);
  assert.equal(imageCount({ images: [] }), 0);
  assert.equal(imageCount({ id: 'a' }), null);
});

// --- The fake API ---

interface Call {
  method: string;
  url: URL;
  headers: Record<string, string>;
  body?: string;
  json?: Record<string, unknown>;
  file?: Blob;
}

interface State {
  edit: { id: string; status: string } | null;
  /** The live version's binaries, which a new edit starts from. */
  live: Binary[];
  binaries: Binary[];
  listings: Map<string, Record<string, unknown>>;
  /** `<language>/<type>`: the uploaded files' contents */
  images: Map<string, string[]>;
  /** A resource's version, its ETag */
  versions: Map<string, number>;
  /** Languages Amazon won't add */
  refuse?: string[];
  /** Answers by "METHOD path" instead of the fake's. */
  fail?: Record<string, [number, unknown]>;
  committed?: boolean;
  nextId: number;
}

const newState = (extra: Partial<State> = {}): State => ({
  edit: null,
  live: [],
  binaries: [],
  listings: new Map(),
  images: new Map(),
  versions: new Map(),
  nextId: 1,
  ...extra,
});

/** A fetch that answers as Amazon's API would, and the calls it got. */
function fake(state: State): { fetch: typeof fetch; calls: Call[] } {
  const calls: Call[] = [];
  const reply = (status: number, body?: unknown, etag?: string) =>
    new Response(body === undefined ? null : JSON.stringify(body), {
      status,
      headers: {
        ...(body === undefined ? {} : { 'content-type': 'application/json' }),
        ...(etag === undefined ? {} : { etag }),
      },
    });
  const etag = (key: string) => `"${key}#${state.versions.get(key) ?? 0}"`;
  const bump = (key: string) => state.versions.set(key, (state.versions.get(key) ?? 0) + 1);
  const f = async (input: string | URL | Request, init?: RequestInit): Promise<Response> => {
    const url = new URL(String(input));
    const method = init?.method ?? 'GET';
    const call: Call = { method, url, headers: { ...(init?.headers as Record<string, string>) } };
    if (typeof init?.body === 'string') call.body = init.body;
    if (typeof init?.body === 'string' && call.headers['Content-Type'] === 'application/json')
      call.json = JSON.parse(init.body);
    else if (init?.body instanceof Blob) call.file = init.body;
    calls.push(call);
    const route = `${method} ${url.pathname}`;
    const failure = state.fail?.[route];
    if (failure !== undefined) return reply(...failure);
    if (url.href === TOKEN_URL) {
      const form = new URLSearchParams(String(init?.body));
      if (form.get('client_secret') !== 'secret')
        return reply(401, {
          error: 'invalid_client',
          error_description: 'Client authentication failed',
        });
      return reply(200, { access_token: 'Atc|token', token_type: 'bearer', expires_in: 3600 });
    }
    if (call.headers.Authorization !== 'Bearer Atc|token')
      return reply(403, { message: 'Request is not authorized.' });
    const base = `/api/appstore/v1/applications/${APP}/edits`;
    if (!url.pathname.startsWith(base)) return reply(404, { message: 'no such app' });
    const rest = url.pathname.slice(base.length);
    // Changes need the ETag of the resource's last GET
    const guarded = (key: string): Response | null =>
      call.headers['If-Match'] === etag(key)
        ? null
        : reply(412, { message: `stale ETag of ${key}` });
    const open = state.edit !== null && state.edit.status === 'IN_PROGRESS';
    const e = state.edit?.id ?? 'none';
    if (rest === '' && method === 'GET') return reply(200, state.edit ?? {});
    if (rest === '' && method === 'POST') {
      if (state.edit !== null) return reply(412, { message: 'An edit already exists' });
      state.edit = { id: `edit-${state.nextId++}`, status: 'IN_PROGRESS' };
      state.binaries = state.live.map((b) => ({ ...b }));
      return reply(200, state.edit);
    }
    if (!(open && rest.startsWith(`/${e}`))) return reply(404, { message: `no edit ${rest}` });
    const sub = rest.slice(`/${e}`.length);
    const apk = /^\/apks\/([^/]+)$/.exec(sub);
    const replace = /^\/apks\/([^/]+)\/replace$/.exec(sub);
    const lang = /^\/listings\/([^/]+)$/.exec(sub);
    const img = /^\/listings\/([^/]+)\/([a-z-]+)(\/upload)?$/.exec(sub);
    if (sub === '' && method === 'GET') return reply(200, state.edit, etag('edit'));
    if (sub === '/validate' && method === 'POST') return reply(200, state.edit);
    if (sub === '/commit' && method === 'POST') {
      const stale = guarded('edit');
      if (stale !== null) return stale;
      state.committed = true;
      if (state.edit) state.edit.status = 'SUBMITTED';
      return reply(200, state.edit);
    }
    if (sub === '/apks' && method === 'GET') return reply(200, state.binaries);
    if (apk !== null && method === 'GET') {
      const b = state.binaries.find((x) => x.id === apk[1]);
      return b === undefined
        ? reply(404, { message: 'no apk' })
        : reply(200, b, etag(`apk/${b.id}`));
    }
    if (replace !== null && method === 'PUT') {
      const b = state.binaries.find((x) => x.id === replace[1]);
      if (b === undefined) return reply(404, { message: 'no apk' });
      const stale = guarded(`apk/${b.id}`);
      if (stale !== null) return stale;
      assert.equal(call.headers['Content-Type'], 'application/vnd.android.package-archive');
      const content = await call.file?.text();
      const abi = /^APK (.+)$/.exec(content ?? '')?.[1] ?? '';
      const digit = { 'armeabi-v7a': 1, 'arm64-v8a': 2, x86_64: 4 }[abi] ?? 0;
      b.versionCode = CODE * 10 + digit;
      bump(`apk/${b.id}`);
      return reply(200, b);
    }
    if (sub === '/listings' && method === 'GET')
      return reply(200, { listings: Object.fromEntries(state.listings) });
    if (lang !== null) {
      const language = lang[1] ?? '';
      const current = state.listings.get(language);
      if (method === 'GET')
        return current === undefined
          ? reply(404, { message: 'no listing' })
          : reply(200, current, etag(`listing/${language}`));
      if (method === 'PUT') {
        if (current === undefined) {
          if (state.refuse?.includes(language))
            return reply(400, { message: 'Language not supported' });
        } else {
          const stale = guarded(`listing/${language}`);
          if (stale !== null) return stale;
        }
        state.listings.set(language, call.json ?? {});
        bump(`listing/${language}`);
        return reply(200, call.json);
      }
    }
    if (img !== null) {
      const [, language, type, upload] = img;
      const key = `${language}/${type}`;
      const images = state.images.get(key) ?? [];
      if (method === 'GET' && upload === undefined)
        return reply(
          200,
          images.map((_, i) => ({ id: `${key}/${i}` })),
          etag(`images/${key}`),
        );
      const stale = guarded(`images/${key}`);
      if (stale !== null) return stale;
      bump(`images/${key}`);
      if (method === 'DELETE') {
        state.images.set(key, []);
        return reply(204);
      }
      if (method === 'POST' && upload !== undefined) {
        const content = (await call.file?.arrayBuffer()) ?? new ArrayBuffer(0);
        const name = `${content.byteLength} bytes`;
        state.images.set(
          key,
          type?.endsWith('-icons') || type === 'promo-images' ? [name] : [...images, name],
        );
        return reply(200, { id: `${key}/${images.length}` });
      }
    }
    return reply(404, { message: `no route ${route}` });
  };
  return { fetch: f as typeof fetch, calls };
}

const dirs: string[] = [];
after(() => {
  for (const dir of dirs) fs.rmSync(dir, { recursive: true, force: true });
});
const tmp = () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'amazon-appstore-test-'));
  dirs.push(dir);
  return dir;
};

function apkFiles(): string[] {
  const dir = tmp();
  return ['armeabi-v7a', 'arm64-v8a', 'x86_64'].map((abi) => {
    const file = path.join(dir, `hashi-1.2.0-foss-${abi}.apk`);
    fs.writeFileSync(file, `APK ${abi}`);
    return file;
  });
}

const connection = (f: typeof fetch): Connection => ({
  fetch: f,
  api: API,
  appId: APP,
  token: 'Atc|token',
});
const lines: string[] = [];
const log = (line: string) => lines.push(line);

test('the access token: client credentials for the App Submission API', async () => {
  const { fetch: f, calls } = fake(newState());
  assert.equal(
    await accessToken(f, TOKEN_URL, 'amzn1.application-oa2-client.x', 'secret'),
    'Atc|token',
  );
  const form = new URLSearchParams(calls[0]?.body);
  assert.deepEqual(Object.fromEntries(form), {
    grant_type: 'client_credentials',
    client_id: 'amzn1.application-oa2-client.x',
    client_secret: 'secret',
    scope: 'appstore::apps:readwrite',
  });
  assert.equal(calls[0]?.method, 'POST');
  assert.equal(calls[0]?.headers['Content-Type'], 'application/x-www-form-urlencoded');
  await assert.rejects(
    accessToken(f, TOKEN_URL, 'amzn1.application-oa2-client.x', 'wrong'),
    /accessToken: HTTP 401: Client authentication failed \(AMAZON_CLIENT_ID and AMAZON_CLIENT_SECRET/,
  );
  assert.equal(SCOPE, 'appstore::apps:readwrite');
});

test('a release: a new edit, each binary replaced with its ETag, recent changes, committed', async () => {
  const state = newState({
    live: [binary('a', 10_010_002), binary('b', 10_010_001)],
    listings: new Map([
      ['en-US', { language: 'en-US', title: 'Hashi', recentChanges: 'Hashi 1.1' }],
      ['de-DE', { language: 'de-DE', title: 'Hashi', recentChanges: 'Hashi 1.1' }],
    ]),
  });
  const { fetch: f, calls } = fake(state);
  const summary = await release(
    connection(f),
    {
      version: '1.2.0',
      code: CODE,
      apks: apks(apkFiles(), '1.2.0', CODE),
      recentChanges: 'New: rivals',
      submit: true,
    },
    log,
  );
  const edit = `/api/appstore/v1/applications/${APP}/edits`;
  assert.deepEqual(
    calls.map((c) => `${c.method} ${c.url.pathname.replace(edit, '')}`),
    [
      'GET ',
      'POST ',
      'GET /edit-1/apks',
      'GET /edit-1/apks/a',
      'PUT /edit-1/apks/a/replace',
      'GET /edit-1/apks/b',
      'PUT /edit-1/apks/b/replace',
      'GET /edit-1/apks',
      'GET /edit-1/listings',
      'GET /edit-1/listings/en-US',
      'PUT /edit-1/listings/en-US',
      'GET /edit-1/listings/de-DE',
      'PUT /edit-1/listings/de-DE',
      'POST /edit-1/validate',
      'GET /edit-1',
      'POST /edit-1/commit',
    ],
  );
  for (const c of calls) assert.equal(c.headers.Authorization, 'Bearer Atc|token');
  // The APK as the body, with its name
  assert.equal(calls[4]?.headers.fileName, 'hashi-1.2.0-foss-arm64-v8a.apk');
  assert.equal(await calls[4]?.file?.text(), 'APK arm64-v8a');
  assert.deepEqual(
    state.binaries.map((b) => [b.id, b.versionCode]),
    [
      ['a', 10_020_002],
      ['b', 10_020_001],
    ],
  );
  // The listing as it was, with the release notes
  assert.deepEqual(state.listings.get('de-DE'), {
    language: 'de-DE',
    title: 'Hashi',
    recentChanges: 'New: rivals',
  });
  assert.ok(state.committed);
  assert.match(summary.join('\n'), /arm64-v8a \(10020002\), armeabi-v7a \(10020001\)/);
  assert.match(summary.join('\n'), /Not uploaded, the app has no binary of their ABI: x86_64/);
  assert.match(summary.join('\n'), /Submitted for review/);
});

test('a re-run continues the open edit: only what is missing, not committed without --submit', async () => {
  const state = newState({
    edit: { id: 'edit-7', status: 'IN_PROGRESS' },
    binaries: [binary('a', 10_020_002), binary('b', 10_010_001)],
    listings: new Map([['en-US', { language: 'en-US', recentChanges: 'New: rivals' }]]),
  });
  const { fetch: f, calls } = fake(state);
  const summary = await release(
    connection(f),
    {
      version: '1.2.0',
      code: CODE,
      apks: apks(apkFiles(), '1.2.0', CODE),
      recentChanges: 'New: rivals',
      submit: false,
    },
    log,
  );
  assert.deepEqual(
    calls
      .filter((c) => c.method !== 'GET')
      .map((c) => `${c.method} ${path.basename(c.url.pathname)}`),
    ['PUT replace'],
  );
  assert.equal(state.committed, undefined);
  assert.match(summary.join('\n'), /arm64-v8a already there/);
  assert.match(summary.join('\n'), /Not submitted/);
});

test('nothing changes while an edit is in review, nor with a newer version', async () => {
  const review = fake(newState({ edit: { id: 'edit-3', status: 'SUBMITTED' } }));
  const files = apkFiles();
  await assert.rejects(
    release(
      connection(review.fetch),
      {
        version: '1.2.0',
        code: CODE,
        apks: apks(files, '1.2.0', CODE),
        recentChanges: 'x',
        submit: true,
      },
      log,
    ),
    /edit edit-3 is SUBMITTED, not open/,
  );
  assert.equal(review.calls.length, 1);
  const newer = fake(newState({ live: [binary('a', 10_030_002)] }));
  await assert.rejects(
    release(
      connection(newer.fetch),
      {
        version: '1.2.0',
        code: CODE,
        apks: apks(files, '1.2.0', CODE),
        recentChanges: 'x',
        submit: true,
      },
      log,
    ),
    /newer version/,
  );
  assert.equal(newer.calls.filter((c) => c.method === 'PUT').length, 0);
});

test("Amazon's refusals stop the release with what to do", async () => {
  const files = apkFiles();
  const options = {
    version: '1.2.0',
    code: CODE,
    apks: apks(files, '1.2.0', CODE),
    recentChanges: 'x',
    submit: true,
  };
  const edits = `/api/appstore/v1/applications/${APP}/edits`;
  // The app in review: no edit can be made
  const review = fake(
    newState({ fail: { [`POST ${edits}`]: [412, { message: 'Precondition Failed' }] } }),
  );
  await assert.rejects(
    release(connection(review.fetch), options, log),
    /createEdit: HTTP 412: Precondition Failed \(the app is in review/,
  );
  // The validation's list
  const invalid = fake(
    newState({
      live: [binary('a', 10_010_002)],
      fail: {
        [`POST ${edits}/edit-1/validate`]: [
          403,
          { errors: [{ errorMessage: 'Content rating missing' }] },
        ],
      },
    }),
  );
  const error = await release(connection(invalid.fetch), options, log).catch((e: unknown) => e);
  assert.ok(error instanceof AmazonError);
  assert.equal(error.status, 403);
  assert.match(
    error.message,
    /validateEdit: HTTP 403: Content rating missing \(Amazon refuses the edit: fix it in the Developer Console/,
  );
  assert.equal(invalid.calls.filter((c) => c.url.pathname.endsWith('/commit')).length, 0);
  // A security profile without the API
  const denied = fake(newState());
  await assert.rejects(
    release({ ...connection(denied.fetch), token: 'other' }, options, log),
    /getActiveEdit: HTTP 403: Request is not authorized\. \(the security profile has to be attached/,
  );
});

// --- Listing ---

test('each listing language is a Play locale with its texts and a site language for the screenshots', () => {
  const codes = AMAZON.map((a) => a.code);
  assert.equal(new Set(codes).size, codes.length);
  assert.equal(codes[0], 'en-US');
  for (const { play } of AMAZON)
    assert.ok(
      PLAY.some((p) => p.locale === play && p.same === undefined),
      play,
    );
  const texts = listingTexts();
  assert.equal(texts.length, AMAZON.length);
  // All within Amazon's limits, without other stores
  for (const t of texts) assert.deepEqual(textProblems(t), [], t.code);
  const en = texts[0];
  assert.ok(en);
  assert.match(en.shortDescription, /^Hashi is Hashiwokakero, .*by Sunday\.$/);
  assert.equal(en.featureBullets.length, 4);
  assert.ok(en.featureBullets.every((b) => !(b.startsWith('•') || b.includes('\n'))));
});

test('the short description and the feature bullets from the description', () => {
  const description =
    'First line\nwraps here.\n\nHEAD\n• one\n• two\n\nPlain text.\n\nMORE\n  • three\n• four';
  assert.equal(firstParagraph(description), 'First line wraps here.');
  assert.deepEqual(featureBullets(description), ['one', 'three']);
});

const t = (code: string, extra: Partial<ListingText> = {}): ListingText => ({
  code,
  play: code,
  title: `Hashi ${code}`,
  shortDescription: 'Short.',
  fullDescription: 'Long.',
  featureBullets: ['a', 'b', 'c'],
  ...extra,
});

test("the listing's checks: over a limit or naming another store is left as it is", () => {
  const plan = planListing([
    t('en-US'),
    t('de-DE', { fullDescription: 'x'.repeat(4001) }),
    t('fr-FR', { featureBullets: ['a', 'b'] }),
    t('ja-JP', { shortDescription: '橋'.repeat(700) }),
    t('it-IT', { fullDescription: 'Ora su Google Play' }),
    t('nl-NL'),
  ]);
  assert.deepEqual(
    plan.texts.map((x) => x.code),
    ['en-US', 'nl-NL'],
  );
  assert.match(plan.skipped[0] ?? '', /de-DE.*4001 characters, Amazon takes 4000/);
  assert.match(plan.skipped[1] ?? '', /fr-FR.*2 feature bullets/);
  assert.match(plan.skipped[2] ?? '', /ja-JP.*2100 bytes/);
  assert.match(plan.skipped[3] ?? '', /it-IT.*names Google Play/);
  assert.throws(
    () => planListing([t('en-US', { title: '' })]),
    /default language's texts: en-US \(en-US\): no title/,
  );
  assert.equal(LIMITS.fullDescription, 4000);
  // Amazon's other fields stay
  assert.deepEqual(
    listingBody(t('en-US'), { keywords: ['puzzle'], recentChanges: 'r', title: 'old' }),
    {
      keywords: ['puzzle'],
      recentChanges: 'r',
      language: 'en-US',
      title: 'Hashi en-US',
      shortDescription: 'Short.',
      fullDescription: 'Long.',
      featureBullets: ['a', 'b', 'c'],
    },
  );
});

test("Amazon's screenshot sizes, either way up", () => {
  // Play's phone and 7" tablet screenshots
  assert.deepEqual(screenshotProblems('a.png', 1080, 1920), []);
  assert.deepEqual(screenshotProblems('a.png', 1200, 1920), []);
  assert.deepEqual(screenshotProblems('a.png', 2560, 1600), []);
  // Play's 10" tablet, the iPad's
  assert.match(screenshotProblems('a.png', 1440, 2560)[0] ?? '', /1440×2560, Amazon takes 800×480/);
  assert.match(screenshotProblems('a.png', 2064, 2752)[0] ?? '', /either way up/);
});

test('the listing into the edit: texts and images per language; a language Amazon refuses is reported', async () => {
  const state = newState({
    edit: { id: 'edit-2', status: 'IN_PROGRESS' },
    listings: new Map([
      ['en-US', { language: 'en-US', keywords: ['bridges'], recentChanges: 'r' }],
    ]),
    images: new Map([['en-US/screenshots', ['old 1', 'old 2']]]),
    refuse: ['ja-JP'],
  });
  const { fetch: f, calls } = fake(state);
  const dir = tmp();
  const file = (name: string, size: number) => {
    const p = path.join(dir, name);
    fs.writeFileSync(p, Buffer.alloc(size));
    return p;
  };
  const images = {
    small: file('small.png', 114),
    large: file('large.png', 512),
    promo: new Map([['en-US', file('feature.png', 1024)]]),
    screenshots: new Map([
      ['en-US', [file('1.png', 1), file('2.png', 2), file('3.png', 3)]],
      ['de-DE', [file('4.png', 4), file('5.png', 5), file('6.png', 6)]],
    ]),
  };
  const summary = await listing(connection(f), [t('en-US'), t('de-DE'), t('ja-JP')], images, log);
  assert.deepEqual(state.listings.get('en-US'), {
    language: 'en-US',
    keywords: ['bridges'],
    recentChanges: 'r',
    title: 'Hashi en-US',
    shortDescription: 'Short.',
    fullDescription: 'Long.',
    featureBullets: ['a', 'b', 'c'],
  });
  // A new language: its listing without If-Match
  const put = calls.find((c) => c.method === 'PUT' && c.url.pathname.endsWith('/listings/de-DE'));
  assert.equal(put?.headers['If-Match'], undefined);
  assert.equal(state.listings.get('de-DE')?.title, 'Hashi de-DE');
  // The screenshots replaced, the icons and the promotional image once each
  assert.deepEqual(state.images.get('en-US/screenshots'), ['1 bytes', '2 bytes', '3 bytes']);
  assert.deepEqual(state.images.get('de-DE/screenshots'), ['4 bytes', '5 bytes', '6 bytes']);
  assert.deepEqual(state.images.get('en-US/small-icons'), ['114 bytes']);
  assert.deepEqual(state.images.get('en-US/large-icons'), ['512 bytes']);
  assert.deepEqual(state.images.get('en-US/promo-images'), ['1024 bytes']);
  assert.equal(state.images.get('de-DE/promo-images'), undefined);
  for (const c of calls.filter((x) => x.url.pathname.endsWith('/upload')))
    assert.equal(c.headers['Content-Type'], 'image/png');
  assert.equal(state.listings.has('ja-JP'), false);
  assert.match(summary[0] ?? '', /in 2 languages \(en-US, de-DE\), with the icons/);
  assert.match(summary[1] ?? '', /ja-JP.*Language not supported.*add it in the Developer Console/);
  // Not committed
  assert.equal(state.committed, undefined);
  assert.equal(state.edit?.status, 'IN_PROGRESS');
});
