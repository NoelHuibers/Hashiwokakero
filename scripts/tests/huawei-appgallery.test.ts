// node --test scripts/tests/: the Huawei AppGallery upload (scripts/huawei-appgallery.ts), against
// a fake of AppGallery Connect's Publishing API and file server that keeps the app's draft
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { after, test } from 'node:test';
import { PLAY } from '../../store/locales.ts';
import { apks } from '../galaxy-store.ts';
import {
  AUDIENCE,
  apkLabel,
  appState,
  busy,
  type Connection,
  checkApks,
  DEFAULT_LANGUAGE,
  HUAWEI,
  HUAWEI_KNOWN,
  HuaweiError,
  jwt,
  LIMITS,
  type ListingText,
  listing,
  listingImages,
  listingTexts,
  planListing,
  release,
  releaseNotes,
  SUBMIT_RETRY,
  screenshotProblems,
  serviceAccount,
  stillParsing,
  textProblems,
  versionCheck,
} from '../huawei-appgallery.ts';

const API = 'https://connect-api.cloud.huawei.com/api';
const OBS = 'https://obs.example.com';
const APP = '112233445';
// 1.2.0
const CODE = 1_002_000;

const { privateKey, publicKey } = crypto.generateKeyPairSync('rsa', { modulusLength: 2048 });
const credentials = JSON.stringify({
  project_id: '',
  key_id: 'kid-1',
  private_key: privateKey.export({ type: 'pkcs8', format: 'pem' }),
  sub_account: '1000001',
  token_uri: AUDIENCE,
});
const account = serviceAccount(credentials);

const dirs: string[] = [];
after(() => {
  for (const dir of dirs) fs.rmSync(dir, { recursive: true, force: true });
});
const tmp = () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'huawei-appgallery-test-'));
  dirs.push(dir);
  return dir;
};

function apkFiles(abis = ['armeabi-v7a', 'arm64-v8a']): string[] {
  const dir = tmp();
  return abis.map((abi) => {
    const file = path.join(dir, `hashi-1.2.0-foss-${abi}.apk`);
    fs.writeFileSync(file, `APK ${abi}`);
    return file;
  });
}

/** A PNG's header (pngInfo reads no more), padded to `size` bytes. */
function png(file: string, width: number, height: number, size = 100) {
  const head = Buffer.alloc(Math.max(size, 33));
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]).copy(head);
  head.writeUInt32BE(13, 8);
  head.write('IHDR', 12, 'latin1');
  head.writeUInt32BE(width, 16);
  head.writeUInt32BE(height, 20);
  head.writeUInt8(8, 24);
  head.writeUInt8(2, 25);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, head);
}

// --- The Service Account ---

test("the Service Account's JWT: PS256, its key ID, an hour, Huawei's audience", () => {
  const now = Date.UTC(2026, 10, 10, 12);
  const [header, payload, signature] = jwtOf(now).split('.');
  assert.deepEqual(JSON.parse(Buffer.from(header ?? '', 'base64url').toString()), {
    kid: 'kid-1',
    typ: 'JWT',
    alg: 'PS256',
  });
  const claims = JSON.parse(Buffer.from(payload ?? '', 'base64url').toString());
  assert.deepEqual(claims, {
    aud: AUDIENCE,
    iss: '1000001',
    iat: now / 1000 - 60,
    exp: now / 1000 - 60 + 3600,
  });
  assert.ok(verifies(`${header}.${payload}.${signature}`));
});

test("the credential file: a Service Account's, also with escaped line breaks; else a clear error", () => {
  const escaped = credentials.replaceAll('\\n', '\\\\n');
  assert.equal(serviceAccount(escaped).keyId, 'kid-1');
  assert.throws(() => serviceAccount('not json'), /isn't JSON/);
  // An API client's: client_id and client_secret
  assert.throws(
    () => serviceAccount(JSON.stringify({ client_id: '1', client_secret: 's' })),
    /not of an API client/,
  );
  assert.throws(
    () => serviceAccount(JSON.stringify({ key_id: 'k', sub_account: '1', private_key: 'garbage' })),
    /isn't a private key/,
  );
  const ec = crypto.generateKeyPairSync('ec', { namedCurve: 'P-256' }).privateKey;
  assert.throws(
    () =>
      serviceAccount(
        JSON.stringify({
          key_id: 'k',
          sub_account: '1',
          private_key: ec.export({ type: 'pkcs8', format: 'pem' }),
        }),
      ),
    /isn't an RSA key \(ec\)/,
  );
});

const jwtOf = (now: number) => jwt(account, now);
function verifies(token: string): boolean {
  const [header, payload, signature] = token.split('.');
  return crypto.verify(
    'sha256',
    Buffer.from(`${header}.${payload}`),
    {
      key: publicKey,
      padding: crypto.constants.RSA_PKCS1_PSS_PADDING,
      saltLength: crypto.constants.RSA_PSS_SALTLEN_DIGEST,
    },
    Buffer.from(signature ?? '', 'base64url'),
  );
}

// --- Local checks ---

test('the APKs: a 64-bit one has to be there', () => {
  checkApks(apks(apkFiles(), '1.2.0', CODE));
  assert.throws(
    () => checkApks(apks(apkFiles(['armeabi-v7a']), '1.2.0', CODE)),
    /no arm64-v8a APK/,
  );
});

test('release notes as the new features, unless they name another store', () => {
  assert.deepEqual(releaseNotes('Hashi 1.2: rivals', '1.2.0'), { text: 'Hashi 1.2: rivals' });
  assert.deepEqual(releaseNotes('', '1.2.0'), { text: 'Hashi 1.2.0' });
  const play = releaseNotes('Hashi 1.1: achievements in Google Play Games', '1.1.0');
  assert.equal(play.text, 'Hashi 1.1.0');
  assert.match(play.note ?? '', /name Google Play Games/);
  assert.throws(
    () => releaseNotes('x'.repeat(501), '1.2.0'),
    /501 characters, AppGallery takes 500/,
  );
  // The real ones fit, or name Play
  const real = fs.readFileSync('app/store/whatsnew/whatsnew-en-US', 'utf8').trim();
  assert.ok(releaseNotes(real, '1.1.0').text.length <= LIMITS.newFeatures);
});

const state = (info: Record<string, unknown>) =>
  appState(APP, { ret: { code: 0, msg: 'success' }, appInfo: info, languages: [] });

test("the app's state: in review it waits; a newer version refuses, this version is done", () => {
  for (const s of [3, 4, 5, 6, 12])
    assert.match(busy(state({ releaseState: s })) ?? '', /The app is .* \(releaseState/);
  for (const s of [0, 1, 7, 8, 11, 13]) assert.equal(busy(state({ releaseState: s })), null);
  const list = apks(apkFiles(), '1.2.0', CODE);
  assert.equal(versionCheck(state({ releaseState: 7 }), list, CODE), null);
  assert.equal(versionCheck(state({ onShelfVersionCode: 10_010_002 }), list, CODE), null);
  assert.match(
    versionCheck(
      state({ onShelfVersionCode: 10_020_002, onShelfVersionNumber: '1.2.0' }),
      list,
      CODE,
    ) ?? '',
    /1\.2\.0 \(versionCode 10020002\) on sale already/,
  );
  assert.throws(
    () => versionCheck(state({ onShelfVersionCode: 10_030_002 }), list, CODE),
    /newer version/,
  );
});

test('app-submit: still parsing is tried again, anything else is for good', () => {
  assert.ok(stillParsing({ code: 204144727, msg: 'The package is being compiled.' }));
  assert.ok(
    stillParsing({
      code: 204144660,
      msg: '[cds]submit failed, additional msg is [The pkg: [a.apk] is being processed. It may take 2-5 minutes]',
    }),
  );
  assert.ok(!stillParsing({ code: 204144660, msg: '80210105: Failed to parse the AAB package' }));
  assert.ok(!stillParsing({ code: 204144660, msg: 'registeredEntity can not be empty' }));
  assert.ok(!stillParsing({ code: 204144664, msg: 'pre-check failed' }));
});

// --- The fake API ---

interface Call {
  method: string;
  url: URL;
  headers: Record<string, string>;
  json?: Record<string, unknown>;
  file?: Blob;
}

interface Uploaded {
  name: string;
  content: string;
}

interface FakeState {
  /** The apps of the account with this package name. */
  apps: string[];
  info: Record<string, unknown>;
  languages: { lang: string; newFeatures?: string; appName?: string }[];
  /** Upload URLs handed out and not used yet, by object ID. */
  pending: Map<string, { name: string; length: number; sha256: string }>;
  objects: Map<string, Uploaded>;
  /** The draft: its packages, icon and screenshots by language. */
  packages: string[];
  icons: Map<string, string>;
  screenshots: Map<string, string[]>;
  needGms?: number;
  /** app-submit answers "still parsing" this many times first. */
  parsing: number;
  submitted: boolean;
  /** Languages Huawei won't add. */
  refuse?: string[];
  /** Answers by "METHOD path" instead of the fake's: [HTTP status, body]. */
  fail?: Record<string, [number, unknown]>;
  nextId: number;
}

const newState = (extra: Partial<FakeState> = {}): FakeState => ({
  apps: [APP],
  info: { releaseState: 0, onShelfVersionNumber: '1.1.0', onShelfVersionCode: 10_010_002 },
  languages: [{ lang: 'en-US', newFeatures: 'old' }, { lang: 'de-DE' }],
  pending: new Map(),
  objects: new Map(),
  packages: [],
  icons: new Map(),
  screenshots: new Map(),
  parsing: 0,
  submitted: false,
  nextId: 1,
  ...extra,
});

/** A fetch that answers as Huawei's API and file server would, and the calls it got. */
function fake(s: FakeState): { fetch: typeof fetch; calls: Call[] } {
  const calls: Call[] = [];
  const reply = (status: number, body: unknown) =>
    new Response(JSON.stringify(body), {
      status,
      headers: { 'content-type': 'application/json' },
    });
  const ok = (extra: Record<string, unknown> = {}) =>
    reply(200, { ret: { code: 0, msg: 'success' }, ...extra });
  const ret = (code: number, msg: string) => reply(200, { ret: { code, msg } });
  const f = async (input: string | URL | Request, init?: RequestInit): Promise<Response> => {
    const url = new URL(String(input));
    const method = init?.method ?? 'GET';
    const call: Call = { method, url, headers: { ...(init?.headers as Record<string, string>) } };
    if (typeof init?.body === 'string') call.json = JSON.parse(init.body);
    else if (init?.body instanceof Blob) call.file = init.body;
    calls.push(call);
    const failure = s.fail?.[`${method} ${url.pathname}`];
    if (failure !== undefined) return reply(...failure);

    // The file server: the signed headers of its upload URL, the announced length and hash
    if (url.origin === OBS) {
      const objectId = url.pathname.slice(1);
      const expected = s.pending.get(objectId);
      if (method !== 'PUT' || expected === undefined)
        return new Response('NoSuchUpload', { status: 403 });
      if (
        call.headers.Authorization !== `OBS ${objectId}` ||
        call.headers['x-amz-date'] === undefined
      )
        return new Response('SignatureDoesNotMatch', { status: 403 });
      const content = Buffer.from((await call.file?.arrayBuffer()) ?? new ArrayBuffer(0));
      if (content.length !== expected.length)
        return new Response('length differs', { status: 400 });
      if (crypto.createHash('sha256').update(content).digest('hex') !== expected.sha256)
        return new Response('sha256 differs', { status: 400 });
      s.pending.delete(objectId);
      s.objects.set(objectId, { name: expected.name, content: content.toString('latin1') });
      return new Response(null, { status: 200 });
    }

    const auth = /^Bearer (.+)$/.exec(call.headers.Authorization ?? '')?.[1];
    if (auth === undefined || !verifies(auth))
      return reply(401, { ret: { code: 401, msg: 'unauthorized' } });
    const appId = url.searchParams.get('appId');
    const route = `${method} ${url.pathname.replace(/^\/api/, '')}`;
    if (route === 'GET /publish/v2/appid-list') {
      assert.equal(url.searchParams.get('packageName'), 'rs.hashi');
      return ok({ appids: s.apps.map((value) => ({ key: 'Hashi', value })) });
    }
    if (appId !== APP) return ret(204144644, 'Failed to query app information.');
    if (route === 'GET /publish/v2/app-info')
      return ok({ appInfo: s.info, auditInfo: { auditOpinion: '' }, languages: s.languages });
    const busy = [3, 4, 5, 12].includes(Number(s.info.releaseState));
    if (route === 'PUT /publish/v2/properties/gms') {
      s.needGms = Number(call.json?.needGms);
      return ok();
    }
    if (route === 'GET /publish/v2/upload-url/for-obs') {
      assert.equal(url.searchParams.get('chineseMainlandFlag'), '0');
      const objectId = `obj-${s.nextId++}-${url.searchParams.get('fileName')}`;
      s.pending.set(objectId, {
        name: url.searchParams.get('fileName') ?? '',
        length: Number(url.searchParams.get('contentLength')),
        sha256: url.searchParams.get('sha256') ?? '',
      });
      return ok({
        urlInfo: {
          objectId,
          url: `${OBS}/${objectId}`,
          method: 'PUT',
          headers: {
            Authorization: `OBS ${objectId}`,
            'x-amz-date': '20261110T120000Z',
            'x-amz-content-sha256': 'UNSIGNED-PAYLOAD',
            'Content-Type': 'application/octet-stream',
          },
        },
      });
    }
    if (route === 'PUT /publish/v2/app-file-info') {
      if (busy) return ret(204144647, 'Failed to update the service.');
      const body = call.json ?? {};
      const files = (body.files as { fileName: string; fileDestUrl: string }[]).map((file) => {
        const o = s.objects.get(file.fileDestUrl);
        assert.ok(o, `${file.fileDestUrl} was uploaded`);
        assert.equal(o.name, file.fileName);
        return o.content;
      });
      const lang = String(body.lang ?? '');
      if (body.fileType === 5) {
        s.packages = files;
        return ok({ pkgVersion: ['pkg-1'] });
      }
      if (body.fileType === 0) s.icons.set(lang, files[0] ?? '');
      if (body.fileType === 2) {
        assert.equal(body.imgShowType, 0);
        s.screenshots.set(lang, files);
      }
      return ok();
    }
    if (route === 'PUT /publish/v2/app-language-info') {
      if (busy) return ret(204144647, 'Failed to update the service.');
      const body = call.json ?? {};
      const lang = String(body.lang);
      let l = s.languages.find((x) => x.lang === lang);
      if (l === undefined) {
        if (s.refuse?.includes(lang))
          return ret(204144650, 'the specified language does not exist');
        l = { lang };
        s.languages.push(l);
      }
      Object.assign(l, body);
      return ok();
    }
    if (route === 'POST /publish/v2/app-submit') {
      assert.equal(url.searchParams.get('releaseType'), '1');
      if (s.parsing > 0) {
        s.parsing--;
        return ret(
          204144660,
          '[cds]submit failed, additional msg is [The pkg: [hashi.apk] is being processed. It may take 2-5 minutes]',
        );
      }
      s.submitted = true;
      s.info.releaseState = 4;
      return ok();
    }
    return reply(404, { ret: { code: 404, msg: `no route ${route}` } });
  };
  return { fetch: f as typeof fetch, calls };
}

const connection = (f: typeof fetch, waited: number[] = []): Connection => {
  let now = 0;
  return {
    fetch: f,
    api: API,
    account,
    sleep: async (ms) => {
      waited.push(ms);
      now += ms;
    },
    now: () => now,
  };
};
const log = () => {
  // The tests read the summaries, not the log
};
const route = (c: Call) =>
  `${c.method} ${c.url.origin === OBS ? 'obs' : c.url.pathname.replace(/^\/api\/publish\/v2/, '')}`;

test('a release: both APKs uploaded and bound together, new features, no GMS, submitted', async () => {
  const s = newState();
  const { fetch: f, calls } = fake(s);
  const summary = await release(
    connection(f),
    {
      version: '1.2.0',
      code: CODE,
      apks: apks(apkFiles(), '1.2.0', CODE),
      newFeatures: 'New: rivals',
      submit: true,
    },
    log,
  );
  assert.deepEqual(calls.map(route), [
    'GET /appid-list',
    'GET /app-info',
    'PUT /properties/gms',
    'GET /upload-url/for-obs',
    'PUT obs',
    'GET /upload-url/for-obs',
    'PUT obs',
    'PUT /app-file-info',
    'PUT /app-language-info',
    'PUT /app-language-info',
    'POST /app-submit',
  ]);
  // 64-bit first; both in one binding: AppGallery's multi-APK version
  assert.deepEqual(s.packages, ['APK arm64-v8a', 'APK armeabi-v7a']);
  assert.equal(s.needGms, 0);
  assert.deepEqual(
    s.languages.map((l) => [l.lang, l.newFeatures]),
    [
      ['en-US', 'New: rivals'],
      ['de-DE', 'New: rivals'],
    ],
  );
  assert.ok(s.submitted);
  assert.match(summary.join('\n'), /arm64-v8a \(10020002\), armeabi-v7a \(10020001\).*\nSubmitted/);
  // The file names AppGallery shows, and the sha256 it checks the upload against
  const urlCall = calls.find((c) => c.url.pathname.endsWith('/upload-url/for-obs'));
  assert.equal(urlCall?.url.searchParams.get('fileName'), 'hashi-1.2.0-foss-arm64-v8a.apk');
  assert.equal(
    urlCall?.url.searchParams.get('sha256'),
    crypto.createHash('sha256').update('APK arm64-v8a').digest('hex'),
  );
});

test('app-submit waits two minutes at a time while AppGallery parses the packages', async () => {
  const s = newState({ parsing: 2 });
  const { fetch: f, calls } = fake(s);
  const waited: number[] = [];
  const options = {
    version: '1.2.0',
    code: CODE,
    apks: apks(apkFiles(), '1.2.0', CODE),
    newFeatures: 'New: rivals',
    submit: true,
  };
  await release(connection(f, waited), options, log);
  assert.deepEqual(waited, [SUBMIT_RETRY.delayMs, SUBMIT_RETRY.delayMs]);
  assert.equal(calls.filter((c) => route(c) === 'POST /app-submit').length, 3);
  assert.ok(s.submitted);
  // Parsing for longer than ten minutes: it gives up, saying the packages are bound
  const slow = fake(newState({ parsing: 100 }));
  await assert.rejects(
    release(connection(slow.fetch), options, log),
    /still parsing after ten minutes: the packages are bound/,
  );
  // At 0, 2, 4, 6, 8 and 10 minutes
  assert.equal(slow.calls.filter((c) => route(c) === 'POST /app-submit').length, 6);
});

test('without --submit the version stays a draft; the same new features are not sent again', async () => {
  const s = newState({ languages: [{ lang: 'en-US', newFeatures: 'New: rivals' }] });
  const { fetch: f, calls } = fake(s);
  const summary = await release(
    connection(f),
    {
      version: '1.2.0',
      code: CODE,
      apks: apks(apkFiles(), '1.2.0', CODE),
      newFeatures: 'New: rivals',
      submit: false,
    },
    log,
  );
  assert.ok(!s.submitted);
  assert.equal(calls.filter((c) => route(c) === 'PUT /app-language-info').length, 0);
  assert.equal(calls.filter((c) => route(c) === 'POST /app-submit').length, 0);
  assert.match(summary.join('\n'), /Not submitted \(HUAWEI_APPGALLERY_SUBMIT is false\)/);
  assert.equal(s.packages.length, 2);
});

test('nothing changes while the app is in review, with a newer version, or this one on sale', async () => {
  const options = {
    version: '1.2.0',
    code: CODE,
    apks: apks(apkFiles(), '1.2.0', CODE),
    newFeatures: 'x',
    submit: true,
  };
  const review = fake(newState({ info: { releaseState: 5 } }));
  await assert.rejects(release(connection(review.fetch), options, log), /in review \(an update\)/);
  const newer = fake(newState({ info: { releaseState: 0, onShelfVersionCode: 10_030_002 } }));
  await assert.rejects(release(connection(newer.fetch), options, log), /newer version/);
  const live = fake(newState({ info: { releaseState: 0, onShelfVersionCode: 10_020_001 } }));
  assert.match(
    (await release(connection(live.fetch), options, log)).join('\n'),
    /on sale already: nothing to do/,
  );
  for (const { calls } of [review, newer, live])
    assert.deepEqual(calls.map(route), ['GET /appid-list', 'GET /app-info']);
});

test("Huawei's refusals stop the release with what to do", async () => {
  const options = {
    version: '1.2.0',
    code: CODE,
    apks: apks(apkFiles(), '1.2.0', CODE),
    newFeatures: 'x',
    submit: true,
  };
  // No app with the package name yet
  const none = fake(newState({ apps: [] }));
  await assert.rejects(
    release(connection(none.fetch), options, log),
    /no app rs\.hashi for this account: create it in AppGallery Connect/,
  );
  // The pre-check: something the review needs is missing
  const precheck = fake(
    newState({
      fail: {
        'POST /api/publish/v2/app-submit': [
          200,
          { ret: { code: 204144664, msg: 'Failed to perform pre-check' } },
        ],
      },
    }),
  );
  const error = await release(connection(precheck.fetch), options, log).catch((e: unknown) => e);
  assert.ok(error instanceof HuaweiError);
  assert.equal(error.code, 204144664);
  assert.match(
    error.message,
    /app-submit: 204144664 Failed to perform pre-check \(something the review needs is missing/,
  );
  // Another signature
  const signed = fake(
    newState({
      fail: {
        'PUT /api/publish/v2/app-file-info': [
          200,
          {
            ret: { code: 204144723, msg: 'Inconsistent signatures of multiple package versions.' },
          },
        ],
      },
    }),
  );
  await assert.rejects(
    release(connection(signed.fetch), options, log),
    /204144723 .*signed with the direct key/,
  );
  // A Service Account without the role
  const denied = fake(
    newState({
      fail: {
        'GET /api/publish/v2/appid-list': [403, { ret: { code: 403, msg: 'no permission' } }],
      },
    }),
  );
  await assert.rejects(
    release(connection(denied.fetch), options, log),
    /appid-list: HTTP 403: no permission \(HUAWEI_SERVICE_ACCOUNT_JSON: a developer-level Service Account with the App administrator role/,
  );
  // The file server refusing an upload
  const obs = fake(
    newState({ fail: { 'PUT /obj-1-hashi-1.2.0-foss-arm64-v8a.apk': [403, 'denied'] } }),
  );
  await assert.rejects(
    release(connection(obs.fetch), options, log),
    /upload hashi-1\.2\.0-foss-arm64-v8a\.apk: HTTP 403/,
  );
});

// --- Listing ---

test("each listing language is one of AppGallery's and a Play locale with texts", () => {
  const codes = HUAWEI.map((l) => l.code);
  assert.equal(codes[0], DEFAULT_LANGUAGE);
  for (const l of HUAWEI) {
    assert.ok(HUAWEI_KNOWN.includes(l.code), l.code);
    assert.ok(fs.existsSync(`fastlane/metadata/android/${l.play}/title.txt`), l.play);
  }
  // Every Play locale AppGallery has; zh-HK reads zh-TW's texts as on Play
  assert.deepEqual(
    PLAY.filter((p) => !codes.includes(p.locale)).map((p) => p.locale),
    ['fr-CA', 'es-US'],
  );
  assert.equal(HUAWEI.find((l) => l.code === 'zh-HK')?.play, 'zh-TW');
});

test("the listing's texts are all within AppGallery's limits and name no other store", () => {
  const texts = listingTexts();
  assert.equal(texts.length, HUAWEI.length);
  for (const t of texts) assert.deepEqual(textProblems(t), [], t.code);
  assert.equal(planListing(texts).texts.length, texts.length);
  // The app's own name, not Play's title with its keywords
  assert.ok(texts.every((t) => t.appName === 'Hashi'));
  assert.equal(apkLabel(), 'Hashi');
});

test("the listing's checks: over a limit or naming another store is left as it is", () => {
  const t = (code: string, extra: Partial<ListingText> = {}): ListingText => ({
    code,
    play: code,
    appName: 'Hashi',
    briefInfo: 'Bridges',
    appDesc: 'A puzzle.',
    ...extra,
  });
  assert.match(
    textProblems(t('de-DE', { appName: 'x'.repeat(31) }))[0] ?? '',
    /name has 31 characters, AppGallery takes 30/,
  );
  assert.match(
    textProblems(t('de-DE', { briefInfo: 'x'.repeat(81) }))[0] ?? '',
    /short description has 81/,
  );
  assert.match(
    textProblems(t('de-DE', { appDesc: 'Get it on Google Play' }))[0] ?? '',
    /names Google Play/,
  );
  assert.match(textProblems(t('de-DE', { briefInfo: '' }))[0] ?? '', /no short description/);
  const plan = planListing([t('en-US'), t('de-DE', { appDesc: 'On the App Store too' })]);
  assert.deepEqual(
    plan.texts.map((x) => x.code),
    ['en-US'],
  );
  assert.match(plan.skipped[0] ?? '', /de-DE \(de-DE\): names App Store: kept as it is/);
  assert.throws(() => planListing([t('en-US', { appName: '' })]), /default language's texts/);
});

test("AppGallery's screenshot rules: upright 9:16, 320 to 3840 px, 2 MB", () => {
  assert.deepEqual(screenshotProblems('a.png', 1080, 1920, 1_000_000), []);
  assert.deepEqual(screenshotProblems('a.png', 450, 800, 1000), []);
  assert.match(screenshotProblems('a.png', 1920, 1080, 1000)[0] ?? '', /not upright 9:16/);
  assert.match(screenshotProblems('a.png', 1440, 2560 * 2, 1000)[0] ?? '', /320 to 3840/);
  assert.match(
    screenshotProblems('a.png', 1080, 1920, 3_000_000)[0] ?? '',
    /2\.9 MB, AppGallery takes 2 MB/,
  );
});

/** An images folder like the store listing's artifact: `n` phone screenshots per site language. */
function imagesDir(n = 7, size = 1000): string {
  const dir = tmp();
  for (const site of new Set(HUAWEI.map((l) => l.site)))
    for (let i = 1; i <= n; i++)
      png(path.join(dir, site, 'play-phone', `${i}-scene.png`), 1080, 1920, size);
  return dir;
}

test("the listing's images: the icon and each language's phone screenshots, checked", () => {
  const images = listingImages(imagesDir());
  assert.equal(images.screenshots.size, HUAWEI.length);
  assert.equal(images.screenshots.get('zh-HK')?.[0], images.screenshots.get('zh-TW')?.[0]);
  assert.throws(() => listingImages(imagesDir(9)), /9 screenshots .*AppGallery takes 3 to 8/);
  assert.throws(() => listingImages(imagesDir(3, 2_200_000)), /AppGallery takes 2 MB/);
  const icon = path.join(tmp(), 'icon.png');
  png(icon, 300, 300);
  assert.throws(() => listingImages(imagesDir(), icon), /not 216×216 or 512×512/);
});

test('the listing into the draft: texts per language, the icon and screenshots; a refused language is reported', async () => {
  const s = newState({ refuse: ['th'] });
  const { fetch: f, calls } = fake(s);
  const texts = listingTexts();
  const images = listingImages(imagesDir(3));
  const summary = await listing(connection(f), texts, images, log);
  const en = s.languages.find((l) => l.lang === 'en-US') as Record<string, unknown> | undefined;
  assert.equal(en?.appName, 'Hashi');
  assert.equal(en?.briefInfo, texts[0]?.briefInfo);
  assert.equal(s.languages.length, HUAWEI.length - 1);
  // The icon in the default language (the others fall back to it), screenshots in each
  assert.deepEqual([...s.icons.keys()], ['en-US']);
  assert.equal(s.screenshots.size, HUAWEI.length - 1);
  assert.equal(s.screenshots.get('de-DE')?.length, 3);
  assert.match(
    summary[0] ?? '',
    /in 22 languages .*with the icon and the phone screenshots \(not submitted/,
  );
  assert.match(summary[1] ?? '', /Not changed: th \(th\): AppGallery didn't add the language/);
  assert.equal(calls.filter((c) => route(c) === 'POST /app-submit').length, 0);
  // Texts only: no uploads
  const textsOnly = fake(newState());
  await listing(connection(textsOnly.fetch), texts, null, log);
  assert.equal(textsOnly.calls.filter((c) => route(c) === 'PUT obs').length, 0);
  // In review: nothing changes
  const review = fake(newState({ info: { releaseState: 4 } }));
  await assert.rejects(listing(connection(review.fetch), texts, null, log), /The app is in review/);
});
