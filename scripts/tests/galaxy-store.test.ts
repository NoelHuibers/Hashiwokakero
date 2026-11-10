// node --test scripts/tests/: the Galaxy Store upload (scripts/galaxy-store.ts), against a fake of
// Samsung's Content Publish API that keeps the app's state
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { after, test } from 'node:test';
import { PLAY } from '../../store/locales.ts';
import {
  type App,
  accessToken,
  apks,
  app,
  type Binary,
  type Connection,
  GALAXY,
  GalaxyError,
  jwt,
  LIMITS,
  type ListingText,
  listingRequest,
  listingTexts,
  otherStores,
  planBinaries,
  planListing,
  privateKey,
  release,
  releaseNotes,
  responseError,
  screenshotProblems,
  statusProblem,
  UPLOAD_URL,
} from '../galaxy-store.ts';

const API = 'https://devapi.samsungapps.com';
const CONTENT = '000012345678';
// 1.2.0
const CODE = 1_002_000;

const { privateKey: rsa, publicKey } = crypto.generateKeyPairSync('rsa', { modulusLength: 2048 });
const pkcs8 = rsa.export({ type: 'pkcs8', format: 'pem' }).toString();

test('the JWT: RS256, signed by the service account for the publishing scope, at most 20 minutes', () => {
  const now = Date.UTC(2026, 10, 10, 12);
  const token = jwt('sa-123', rsa, now);
  const [header, claims, signature] = token.split('.');
  assert.ok(header && claims && signature);
  const decode = (part: string) => JSON.parse(Buffer.from(part, 'base64url').toString());
  assert.deepEqual(decode(header), { alg: 'RS256', typ: 'JWT' });
  const c = decode(claims);
  assert.equal(c.iss, 'sa-123');
  assert.deepEqual(c.scopes, ['publishing']);
  assert.ok(c.iat <= now / 1000 && c.exp > now / 1000);
  assert.ok(c.exp - c.iat <= 20 * 60);
  assert.ok(
    crypto.verify(
      'sha256',
      Buffer.from(`${header}.${claims}`),
      publicKey,
      Buffer.from(signature, 'base64url'),
    ),
  );
});

test('reads the private key as Seller Portal gives it, with escaped line breaks, or bare', () => {
  const expected = rsa.export({ type: 'pkcs8', format: 'der' });
  const der = (key: crypto.KeyObject) => key.export({ type: 'pkcs8', format: 'der' });
  assert.deepEqual(der(privateKey(pkcs8)), expected);
  assert.deepEqual(der(privateKey(pkcs8.replaceAll('\n', '\\n'))), expected);
  assert.deepEqual(der(privateKey(expected.toString('base64'))), expected);
  const pkcs1 = rsa.export({ type: 'pkcs1', format: 'pem' }).toString();
  assert.deepEqual(der(privateKey(pkcs1)), expected);
  assert.throws(() => privateKey(''), /isn't a private key/);
  assert.throws(() => privateKey('not a key'), /isn't a private key/);
  const ec = crypto.generateKeyPairSync('ec', { namedCurve: 'P-256' }).privateKey;
  assert.throws(
    () => privateKey(ec.export({ type: 'pkcs8', format: 'pem' }).toString()),
    /ec key, not RSA/,
  );
});

test("tells the API's answers from its errors, in each of its shapes", () => {
  assert.equal(responseError(200, { resultCode: '0000', resultMessage: 'Ok' }), null);
  assert.equal(responseError(200, { ctntId: CONTENT, httpStatus: 'OK', errorCode: null }), null);
  assert.equal(responseError(200, { fileKey: 'k', errorCode: null, errorMsg: null }), null);
  assert.equal(responseError(200, { ok: true, createdItem: { accessToken: 'a' } }), null);
  assert.equal(responseError(204, null), null);
  assert.equal(responseError(200, [{ contentId: CONTENT }]), null);
  assert.deepEqual(responseError(200, { resultCode: '3201', resultMessage: 'not REGISTERING' }), {
    code: '3201',
    message: 'not REGISTERING',
  });
  assert.deepEqual(responseError(400, { resultCode: '1000', resultMessage: 'bad request' }), {
    code: '1000',
    message: 'bad request',
  });
  assert.deepEqual(
    responseError(200, { httpStatus: 'BAD_REQUEST', errorCode: '4105', errorMsg: 'no' }),
    {
      code: '4105',
      message: 'no',
    },
  );
  assert.deepEqual(
    responseError(401, { ok: false, error: { code: 'AUTH_REQUIRE', message: 'm' } }),
    {
      code: 'AUTH_REQUIRE',
      message: 'm',
    },
  );
  assert.deepEqual(responseError(502, '<html>Bad gateway</html>'), {
    code: null,
    message: '<html>Bad gateway</html>',
  });
  assert.deepEqual(responseError(500, null), { code: null, message: 'HTTP 500' });
});

test('the APKs: by their names, 64-bit first; others refused', () => {
  const list = apks(
    ['release/hashi-1.2.0-foss-armeabi-v7a.apk', 'release/hashi-1.2.0-foss-arm64-v8a.apk'],
    '1.2.0',
    CODE,
  );
  assert.deepEqual(
    list.map((a) => [a.abi, a.versionCode]),
    [
      ['arm64-v8a', 10_020_002],
      ['armeabi-v7a', 10_020_001],
    ],
  );
  assert.throws(
    () => apks(['hashi-1.2.0.aab'], '1.2.0', CODE),
    /not hashi-1\.2\.0-foss-<abi>\.apk/,
  );
  assert.throws(() => apks(['hashi-1.1.0-foss-arm64-v8a.apk'], '1.2.0', CODE), /not hashi-1\.2\.0/);
  assert.throws(() => apks(['hashi-1.2.0-foss-mips.apk'], '1.2.0', CODE), /foss-<abi>/);
  assert.throws(
    () => apks(['a/hashi-1.2.0-foss-x86_64.apk', 'b/hashi-1.2.0-foss-x86_64.apk'], '1.2.0', CODE),
    /an ABI twice/,
  );
  assert.throws(() => apks([], '1.2.0', CODE), /no APK/);
});

const binary = (seq: number, versionCode: number, packageName = 'rs.hashi'): Binary => ({
  binarySeq: String(seq),
  versionCode,
  versionName: `${Math.floor(versionCode / 10_000_000)}.${Math.floor(versionCode / 10_000) % 1000}.0`,
  packageName,
});

test("the update's binaries: older versions out, these APKs in once; newer versions and other apps refused", () => {
  const list = apks(
    ['hashi-1.2.0-foss-arm64-v8a.apk', 'hashi-1.2.0-foss-armeabi-v7a.apk'],
    '1.2.0',
    CODE,
  );
  const old = [binary(1, 10_010_002), binary(2, 10_010_001)];
  const plan = planBinaries(old, list, CODE);
  assert.deepEqual(plan.remove, old);
  assert.deepEqual(plan.keep, []);
  assert.deepEqual(
    plan.add.map((a) => a.versionCode),
    [10_020_002, 10_020_001],
  );
  // A re-run: arm64 is there already, x86_64 of this version (by hand) goes
  const again = planBinaries(
    [binary(1, 10_010_001), binary(3, 10_020_002), binary(4, 10_020_004)],
    list,
    CODE,
  );
  assert.deepEqual(
    again.remove.map((b) => b.binarySeq),
    ['1', '4'],
  );
  assert.deepEqual(
    again.keep.map((b) => b.binarySeq),
    ['3'],
  );
  assert.deepEqual(
    again.add.map((a) => a.abi),
    ['armeabi-v7a'],
  );
  assert.throws(() => planBinaries([binary(5, 10_030_002)], list, CODE), /newer version, 1\.3\.0/);
  assert.throws(
    () => planBinaries([binary(1, 10_010_002, 'com.example')], list, CODE),
    /com\.example, not rs\.hashi: SAMSUNG_CONTENT_ID is another app/,
  );
  // The first upload of an app without binaries
  assert.equal(planBinaries([], list, CODE).add.length, 2);
});

test('which statuses allow an update', () => {
  for (const status of ['FOR_SALE', 'REGISTERING', 'UPDATING', 'RE_REGISTERING'])
    assert.equal(statusProblem(status), null, status);
  assert.match(statusProblem('UNDER_CONTENT_REVIEW') ?? '', /in review/);
  assert.match(statusProblem('READY_TO_PRE_REVIEWS') ?? '', /in review/);
  assert.match(statusProblem('READY_FOR_REVIEW') ?? '', /in review/);
  assert.match(statusProblem('DEVICE_TEST_REJECTED') ?? '', /rejected/);
  assert.match(statusProblem('READY_FOR_CHANGE') ?? '', /publish it/);
  assert.match(statusProblem('SUSPENDED') ?? '', /no update/);
});

// --- The fake API ---

interface Call {
  method: string;
  url: URL;
  headers: Record<string, string>;
  json?: Record<string, unknown>;
  form?: FormData;
}

interface State {
  status: string;
  binaries: Binary[];
  /** The binaries on sale, while an update is open. */
  sale?: Binary[];
  nextSeq: number;
  /** fileKey: file name */
  files: Map<string, string>;
  /** Answers by "METHOD path" instead of the fake's. */
  fail?: Record<string, [number, unknown]>;
  submitted?: boolean;
}

/** A fetch that answers as Samsung's API would, and the calls it got. */
function fake(state: State): { fetch: typeof fetch; calls: Call[] } {
  const calls: Call[] = [];
  const reply = (status: number, body?: unknown) =>
    new Response(body === undefined ? null : JSON.stringify(body), {
      status,
      headers: body === undefined ? {} : { 'content-type': 'application/json' },
    });
  const open = () => ['REGISTERING', 'UPDATING', 'RE_REGISTERING'].includes(state.status);
  const f = async (input: string | URL | Request, init?: RequestInit): Promise<Response> => {
    const url = new URL(String(input));
    const method = init?.method ?? 'GET';
    const call: Call = { method, url, headers: { ...(init?.headers as Record<string, string>) } };
    if (typeof init?.body === 'string') call.json = JSON.parse(init.body);
    else if (init?.body instanceof FormData) call.form = init.body;
    calls.push(call);
    const route = `${method} ${url.host}${url.pathname}`;
    const failure = state.fail?.[route];
    if (failure !== undefined) return reply(...failure);
    const entry = (status: string, binaries: Binary[]) => ({
      contentId: CONTENT,
      appTitle: 'Hashi',
      contentStatus: status,
      defaultLanguageCode: 'ENG',
      paid: 'N',
      publicationType: '01',
      chinaAgeLimit: null,
      // Samsung's numbers as strings
      binaryList: binaries.map((b) => ({ ...b, versionCode: String(b.versionCode), gms: 'N' })),
    });
    switch (route) {
      case 'POST devapi.samsungapps.com/auth/accessToken':
        return reply(200, { ok: true, createdItem: { accessToken: 'access-1' } });
      case 'DELETE devapi.samsungapps.com/auth/revokeAccessToken':
        return reply(200, { ok: true });
      case 'GET devapi.samsungapps.com/seller/contentInfo':
        assert.equal(url.searchParams.get('contentId'), CONTENT);
        return reply(200, [
          ...(state.sale === undefined ? [] : [entry('FOR_SALE', state.sale)]),
          entry(state.status, state.binaries),
        ]);
      case 'POST devapi.samsungapps.com/seller/contentUpdate':
        if (state.status === 'FOR_SALE') {
          state.sale = [...state.binaries];
          state.status = 'UPDATING';
        } else if (!open()) {
          return reply(200, { httpStatus: 'FORBIDDEN', errorCode: '4105', errorMsg: 'not ready' });
        }
        return reply(200, {
          ctntId: CONTENT,
          contentStatus: 'REGISTERING',
          httpStatus: 'OK',
          errorCode: null,
          errorMsg: null,
        });
      case 'POST devapi.samsungapps.com/seller/createUploadSessionId':
        return reply(200, { url: UPLOAD_URL, sessionId: 'session-1' });
      case 'POST seller.samsungapps.com/galaxyapi/fileUpload': {
        if (call.form?.get('sessionId') !== 'session-1')
          return reply(403, { resultCode: '7002', resultMessage: 'SessionId does not exist' });
        const file = call.form.get('file');
        if (!(file instanceof File))
          return reply(403, { resultCode: '7001', resultMessage: 'file' });
        const key = `key-${state.files.size + 1}`;
        state.files.set(key, file.name);
        return reply(200, {
          fileKey: key,
          fileName: file.name,
          fileSize: String(file.size),
          errorCode: null,
          errorMsg: null,
        });
      }
      case 'POST devapi.samsungapps.com/seller/v2/content/binary': {
        if (!open())
          return reply(400, {
            resultCode: '3201',
            resultMessage: 'Application status is not REGISTERING',
          });
        const name = state.files.get(String(call.json?.filekey));
        assert.ok(name, `an uploaded file: ${call.json?.filekey}`);
        const abi = /-foss-(.+)\.apk$/.exec(name)?.[1] ?? '';
        const digit = { 'armeabi-v7a': 1, 'arm64-v8a': 2, x86_64: 4 }[abi] ?? 0;
        const seq = state.nextSeq++;
        state.binaries.push(binary(seq, CODE * 10 + digit));
        return reply(200, {
          resultCode: '0000',
          resultMessage: 'Ok',
          data: { binarySeq: String(seq) },
        });
      }
      case 'DELETE devapi.samsungapps.com/seller/v2/content/binary': {
        if (!open())
          return reply(400, {
            resultCode: '3201',
            resultMessage: 'Application status is not REGISTERING',
          });
        const seq = url.searchParams.get('binarySeq');
        state.binaries = state.binaries.filter((b) => b.binarySeq !== seq);
        return reply(200, { resultCode: '0000', resultMessage: 'Ok' });
      }
      case 'POST devapi.samsungapps.com/seller/contentSubmit':
        if (!open())
          return reply(403, {
            resultCode: '4105',
            resultMessage: 'Content is not ready for distribution',
          });
        state.status = 'READY_FOR_REVIEW';
        state.submitted = true;
        return reply(204);
      default:
        return reply(404, { resultCode: '404', resultMessage: `no route ${route}` });
    }
  };
  return { fetch: f as typeof fetch, calls };
}

/** The file of an upload call. */
function fileOf(call: Call | undefined): File {
  const file = call?.form?.get('file');
  assert.ok(file instanceof File);
  return file;
}

const dirs: string[] = [];
after(() => {
  for (const dir of dirs) fs.rmSync(dir, { recursive: true, force: true });
});

function apkFiles(): string[] {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'galaxy-store-test-'));
  dirs.push(dir);
  return ['armeabi-v7a', 'arm64-v8a'].map((abi) => {
    const file = path.join(dir, `hashi-1.2.0-foss-${abi}.apk`);
    fs.writeFileSync(file, `APK ${abi}`);
    return file;
  });
}

const connection = (f: typeof fetch): Connection => ({
  fetch: f,
  api: API,
  uploadUrl: UPLOAD_URL,
  serviceAccountId: 'sa-123',
  token: 'access-1',
});

const lines: string[] = [];
const log = (line: string) => lines.push(line);

test('the access token: the JWT as bearer, the token from createdItem', async () => {
  const { fetch: f, calls } = fake({
    status: 'FOR_SALE',
    binaries: [],
    nextSeq: 1,
    files: new Map(),
  });
  assert.equal(await accessToken(f, API, 'the.jwt.here'), 'access-1');
  assert.equal(calls[0]?.method, 'POST');
  assert.equal(calls[0]?.headers.Authorization, 'Bearer the.jwt.here');
  assert.equal(calls[0]?.headers['service-account-id'], undefined);
  const refused = fake({
    status: 'FOR_SALE',
    binaries: [],
    nextSeq: 1,
    files: new Map(),
    fail: {
      'POST devapi.samsungapps.com/auth/accessToken': [
        401,
        { code: 'AUTH_REQUIRE', message: 'invalid' },
      ],
    },
  });
  await assert.rejects(
    accessToken(refused.fetch, API, 'x'),
    /accessToken: HTTP 401, AUTH_REQUIRE: invalid \(the service account or its key/,
  );
});

test('a release: opens the update, uploads, swaps the binaries 64-bit first, submits', async () => {
  const files = apkFiles();
  const state: State = {
    status: 'FOR_SALE',
    binaries: [binary(1, 10_010_002), binary(2, 10_010_001)],
    nextSeq: 3,
    files: new Map(),
  };
  const { fetch: f, calls } = fake(state);
  const summary = await release(
    connection(f),
    {
      contentId: CONTENT,
      version: '1.2.0',
      code: CODE,
      apks: apks(files, '1.2.0', CODE),
      newFeature: 'New: river',
      submit: true,
    },
    log,
  );
  assert.deepEqual(
    calls.map((c) => `${c.method} ${c.url.pathname}`),
    [
      'GET /seller/contentInfo',
      'POST /seller/contentUpdate',
      'GET /seller/contentInfo',
      'POST /seller/createUploadSessionId',
      'POST /galaxyapi/fileUpload',
      'POST /galaxyapi/fileUpload',
      'DELETE /seller/v2/content/binary',
      'DELETE /seller/v2/content/binary',
      'POST /seller/v2/content/binary',
      'POST /seller/v2/content/binary',
      'POST /seller/contentSubmit',
    ],
  );
  // Every call with the access token and the service account
  for (const c of calls) {
    assert.equal(c.headers.Authorization, 'Bearer access-1');
    assert.equal(c.headers['service-account-id'], 'sa-123');
    assert.equal(c.headers['content-type'], c.json === undefined ? undefined : 'application/json');
  }
  // The update: the required fields as the app has them, the release notes, nothing else changed
  assert.deepEqual(calls[1]?.json, {
    contentId: CONTENT,
    defaultLanguageCode: 'ENG',
    paid: 'N',
    publicationType: '01',
    newFeature: 'New: river',
    addLanguage: null,
    screenshots: null,
    sellCountryList: null,
  });
  // The files in one session, to the upload host, arm64 first
  assert.equal(calls[4]?.url.href, UPLOAD_URL);
  assert.equal(fileOf(calls[4]).name, 'hashi-1.2.0-foss-arm64-v8a.apk');
  assert.equal(await fileOf(calls[4]).text(), 'APK arm64-v8a');
  assert.equal(fileOf(calls[5]).name, 'hashi-1.2.0-foss-armeabi-v7a.apk');
  // The old binaries by their seq, both ways the documentation shows
  assert.equal(calls[6]?.url.searchParams.get('binarySeq'), '1');
  assert.deepEqual(calls[6]?.json, { contentId: CONTENT, binarySeq: '1' });
  assert.equal(calls[7]?.url.searchParams.get('contentId'), CONTENT);
  assert.deepEqual(calls[8]?.json, { contentId: CONTENT, gms: 'N', filekey: 'key-1' });
  assert.deepEqual(calls[9]?.json, { contentId: CONTENT, gms: 'N', filekey: 'key-2' });
  assert.deepEqual(calls[10]?.json, { contentId: CONTENT });

  assert.deepEqual(
    state.binaries.map((b) => b.versionCode),
    [10_020_002, 10_020_001],
  );
  // The version on sale untouched until the review
  assert.deepEqual(
    state.sale?.map((b) => b.versionCode),
    [10_010_002, 10_010_001],
  );
  assert.ok(state.submitted);
  assert.match(
    summary.join('\n'),
    /arm64-v8a \(10020002\), armeabi-v7a \(10020001\); deleted 10010002, 10010001/,
  );
  assert.match(summary.join('\n'), /Submitted for review/);
});

test('a re-run continues the open update: only what is missing, not submitted without --submit', async () => {
  const files = apkFiles();
  const state: State = {
    status: 'REGISTERING',
    sale: [binary(1, 10_010_002)],
    // A run before deleted the old binary and added arm64, then failed
    binaries: [binary(3, 10_020_002)],
    nextSeq: 4,
    files: new Map(),
  };
  const { fetch: f, calls } = fake(state);
  const summary = await release(
    connection(f),
    {
      contentId: CONTENT,
      version: '1.2.0',
      code: CODE,
      apks: apks(files, '1.2.0', CODE),
      submit: false,
    },
    log,
  );
  const uploads = calls.filter((c) => c.url.pathname === '/galaxyapi/fileUpload');
  assert.deepEqual(
    uploads.map((c) => fileOf(c).name),
    ['hashi-1.2.0-foss-armeabi-v7a.apk'],
  );
  assert.equal(calls.filter((c) => c.method === 'DELETE').length, 0);
  assert.equal(calls.filter((c) => c.url.pathname === '/seller/contentSubmit').length, 0);
  // No release notes: the update request without newFeature
  assert.equal(calls[1]?.json?.newFeature, undefined);
  assert.deepEqual(
    state.binaries.map((b) => b.versionCode),
    [10_020_002, 10_020_001],
  );
  assert.equal(state.submitted, undefined);
  assert.match(summary.join('\n'), /Not submitted/);
});

test('nothing changes while the app is in review, nor when the store has a newer version', async () => {
  const files = apkFiles();
  const review = fake({
    status: 'UNDER_CONTENT_REVIEW',
    binaries: [binary(1, 10_010_002)],
    nextSeq: 2,
    files: new Map(),
  });
  await assert.rejects(
    release(
      connection(review.fetch),
      {
        contentId: CONTENT,
        version: '1.2.0',
        code: CODE,
        apks: apks(files, '1.2.0', CODE),
        submit: true,
      },
      log,
    ),
    /The app is UNDER_CONTENT_REVIEW: it is in review/,
  );
  assert.equal(review.calls.length, 1);
  const newer = fake({
    status: 'FOR_SALE',
    binaries: [binary(1, 10_030_002)],
    nextSeq: 2,
    files: new Map(),
  });
  await assert.rejects(
    release(
      connection(newer.fetch),
      {
        contentId: CONTENT,
        version: '1.2.0',
        code: CODE,
        apks: apks(files, '1.2.0', CODE),
        submit: true,
      },
      log,
    ),
    /newer version, 1\.3\.0/,
  );
  assert.equal(newer.calls.length, 1);
});

test("an error of the API stops the release, with Samsung's code and what to do", async () => {
  const files = apkFiles();
  const state: State = {
    status: 'FOR_SALE',
    binaries: [binary(1, 10_010_002)],
    nextSeq: 2,
    files: new Map(),
    fail: {
      'POST devapi.samsungapps.com/seller/v2/content/binary': [
        400,
        { resultCode: '5030', resultMessage: 'No device configured for binary' },
      ],
    },
  };
  const { fetch: f, calls } = fake(state);
  const error = await release(
    connection(f),
    {
      contentId: CONTENT,
      version: '1.2.0',
      code: CODE,
      apks: apks(files, '1.2.0', CODE),
      submit: true,
    },
    log,
  ).catch((e: unknown) => e);
  assert.ok(error instanceof GalaxyError);
  assert.equal(error.status, 400);
  assert.equal(error.code, '5030');
  assert.match(
    error.message,
    /add binary hashi-1\.2\.0-foss-arm64-v8a\.apk: HTTP 400, 5030: No device configured for binary \(.*GALAXY_STORE_ABIS=arm64-v8a/,
  );
  assert.equal(calls.filter((c) => c.url.pathname === '/seller/contentSubmit').length, 0);
  // An error in a 200 answer counts too
  const update = fake({
    status: 'FOR_SALE',
    binaries: [],
    nextSeq: 1,
    files: new Map(),
    fail: {
      'POST devapi.samsungapps.com/seller/contentUpdate': [
        200,
        { httpStatus: 'BAD_REQUEST', errorCode: '1000', errorMsg: 'bad request' },
      ],
    },
  });
  await assert.rejects(
    release(
      connection(update.fetch),
      {
        contentId: CONTENT,
        version: '1.2.0',
        code: CODE,
        apks: apks(files, '1.2.0', CODE),
        submit: true,
      },
      log,
    ),
    /contentUpdate: HTTP 200, 1000: bad request/,
  );
});

test('the open update in contentInfo, when it lists the version on sale too', () => {
  const body = [
    {
      contentId: CONTENT,
      contentStatus: 'FOR_SALE',
      binaryList: [{ binarySeq: 1, versionCode: '10010002' }],
    },
    {
      contentId: CONTENT,
      contentStatus: 'UPDATING',
      defaultLanguageCode: 'ENG',
      paid: 'N',
      publicationType: '03',
      binaryList: [{ binarySeq: 2, versionCode: '10020002', packageName: 'rs.hashi' }],
    },
  ];
  const a = app(body, CONTENT);
  assert.equal(a.contentStatus, 'UPDATING');
  assert.equal(a.publicationType, '03');
  assert.deepEqual(a.binaries, [
    { binarySeq: '2', versionCode: 10_020_002, versionName: null, packageName: 'rs.hashi' },
  ]);
  assert.equal(app([body[0]], CONTENT).contentStatus, 'FOR_SALE');
  assert.throws(() => app([], CONTENT), /no app/);
});

// --- Listing ---

test('each listing language is a Play locale with its texts and a site language for the screenshots', () => {
  const codes = GALAXY.map((g) => g.code);
  assert.equal(new Set(codes).size, codes.length);
  assert.equal(codes[0], 'ENG');
  for (const { play } of GALAXY)
    assert.ok(
      PLAY.some((p) => p.locale === play && p.same === undefined),
      play,
    );
  const texts = listingTexts();
  assert.equal(texts.length, GALAXY.length);
  for (const t of texts) {
    assert.ok(t.appTitle.length > 0 && !t.appTitle.includes('\n'), t.play);
    assert.ok(t.description.length > 0, t.play);
  }
  // English within Samsung's limits: it is the default language, which can't be left out
  const en = texts[0];
  assert.ok(en && Buffer.byteLength(en.description) <= LIMITS.description);
});

const appWith = (extra: Partial<App> = {}): App => ({
  contentId: CONTENT,
  contentStatus: 'FOR_SALE',
  defaultLanguageCode: 'ENG',
  paid: 'N',
  publicationType: '01',
  chinaAgeLimit: null,
  binaries: [],
  languages: [],
  screenshots: 4,
  ...extra,
});
const t = (code: string, description = `${code} description`): ListingText => ({
  code,
  play: code.toLowerCase(),
  appTitle: `Hashi ${code}`,
  description,
});

test('the listing: languages over a limit and Chinese without its age rating are left as they are', () => {
  const long = 'ก'.repeat(1500); // 4500 bytes
  const plan = planListing([t('ENG'), t('THA', long), t('ZHO'), t('DEU')], appWith());
  assert.deepEqual(
    plan.texts.map((x) => x.code),
    ['ENG', 'DEU'],
  );
  assert.match(plan.skipped[0] ?? '', /THA.*4500 bytes, Samsung takes 4000/);
  assert.match(plan.skipped[1] ?? '', /ZHO.*China's age rating/);
  assert.deepEqual(
    planListing([t('ZHO')], appWith({ chinaAgeLimit: '12' })).texts.map((x) => x.code),
    ['ZHO'],
  );
  assert.throws(() => planListing([t('ENG', long)], appWith()), /default language/);
});

test("the listing's request: texts, Samsung's other languages kept, screenshots new or reused", () => {
  const a = appWith({
    chinaAgeLimit: '12',
    languages: [
      {
        languagecode: 'THA',
        appTitle: 'old',
        description: 'old th',
        newFeature: 'nf',
        screenshots: [{}, {}, {}, {}],
      },
      {
        languagecode: 'DEU',
        appTitle: 'alt',
        description: 'alt',
        newFeature: 'neu',
        screenshots: [{}, {}, {}, {}, {}],
      },
    ],
  });
  const plan = planListing([t('ENG'), t('DEU'), t('ZHO')], a);
  const texts = listingRequest(a, plan, null);
  assert.equal(texts.appTitle, 'Hashi ENG');
  assert.equal(texts.longDescription, 'ENG description');
  assert.equal(texts.screenshots, null);
  assert.equal(texts.iconKey, undefined);
  assert.equal(texts.chinaAgeLimit, '12');
  assert.equal(texts.defaultLanguageCode, 'ENG');
  assert.deepEqual(texts.addLanguage, [
    {
      languagecode: 'DEU',
      appTitle: 'Hashi DEU',
      description: 'DEU description',
      newFeature: 'neu',
      screenshots: Array(5).fill({ reuseYn: true }),
    },
    {
      languagecode: 'ZHO',
      appTitle: 'Hashi ZHO',
      description: 'ZHO description',
      newFeature: null,
    },
    // Not in this listing: as Samsung has it
    {
      languagecode: 'THA',
      appTitle: 'old',
      description: 'old th',
      newFeature: 'nf',
      screenshots: Array(4).fill({ reuseYn: true }),
    },
  ]);
  const shots = new Map([
    ['ENG', ['e1', 'e2', 'e3', 'e4']],
    ['DEU', ['d1', 'd2', 'd3', 'd4']],
  ]);
  const images = listingRequest(a, plan, { iconKey: 'icon', shots });
  assert.equal(images.iconKey, 'icon');
  assert.deepEqual(images.screenshots, [
    { screenshotKey: 'e1', reuseYn: false },
    { screenshotKey: 'e2', reuseYn: false },
    { screenshotKey: 'e3', reuseYn: false },
    { screenshotKey: 'e4', reuseYn: false },
  ]);
  const deu = (images.addLanguage as { languagecode: string; screenshots?: unknown }[]).find(
    (l) => l.languagecode === 'DEU',
  );
  assert.deepEqual(deu?.screenshots, [
    { screenshotKey: 'd1', reuseYn: false },
    { screenshotKey: 'd2', reuseYn: false },
    { screenshotKey: 'd3', reuseYn: false },
    { screenshotKey: 'd4', reuseYn: false },
  ]);
});

test("Samsung's rules for screenshots: 320 to 3840 px a side, at most 2:1", () => {
  // Play's phone screenshots
  assert.deepEqual(screenshotProblems('a.png', 1080, 1920), []);
  assert.deepEqual(screenshotProblems('a.png', 1920, 1080), []);
  assert.deepEqual(screenshotProblems('a.png', 1000, 2000), []);
  assert.match(screenshotProblems('a.png', 1000, 2001)[0] ?? '', /longer than 2:1/);
  assert.match(screenshotProblems('a.png', 300, 500)[0] ?? '', /320 to 3840/);
  assert.match(screenshotProblems('a.png', 2000, 3900)[0] ?? '', /320 to 3840/);
});

test('release notes and texts that name other stores stay out of the Galaxy Store', () => {
  assert.deepEqual(otherStores('Stamps are achievements in Google Play Games and in Game Center'), [
    'Google Play Games',
    'Game Center',
  ]);
  assert.deepEqual(otherStores('Get it on the play store, the App Store or the APP STORE'), [
    'play store',
    'App Store',
  ]);
  // Not a store: words that only contain one
  assert.deepEqual(otherStores('Applesauce, display stores, the daily puzzle'), []);
  assert.deepEqual(releaseNotes('Hashi 1.2: rivals'), { newFeature: 'Hashi 1.2: rivals' });
  assert.deepEqual(releaseNotes(''), {});
  assert.match(
    releaseNotes('Hashi 1.1: achievements in Google Play Games').note ?? '',
    /name Google Play Games: not sent/,
  );
  assert.equal(releaseNotes('Hashi 1.1: achievements in Google Play Games').newFeature, undefined);
  assert.throws(() => releaseNotes('x'.repeat(4001)), /4001 bytes, Samsung takes 4000/);
  const plan = planListing([t('ENG'), t('DEU', 'Jetzt auch im App Store')], appWith());
  assert.match(plan.skipped[0] ?? '', /DEU.*names App Store/);
});
