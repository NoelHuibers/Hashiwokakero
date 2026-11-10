// Uploads the app to Huawei AppGallery (docs/stores.md, Huawei AppGallery) through the Publishing
// API of AppGallery Connect (https://developer.huawei.com/consumer/en/doc/AppGallery-connect-Guides/agcapi-publish_api_overview-0000001111685332):
//
//   node scripts/huawei-appgallery.ts release --version <x.y.z> --code <n> [--whatsnew <file>]
//                                             [--submit] [--dry-run] <apk>...
//   node scripts/huawei-appgallery.ts listing [--images <dir>] [--dry-run]
//
// release: the foss APKs `hashi-<version>-foss-<abi>.apk` become the packages of the app's next
// version, one APK per ABI (AppGallery's multi-APK version: each goes to the devices of its CPU
// architecture, the higher versionCode where both run): each is uploaded, then all are bound to
// the version at once (app-file-info), the release notes go into every language's "new
// features", and with --submit the version is submitted for review (a full release: it goes live
// after the review). The app is marked as not needing Google services. It refuses an app in
// review and a newer version on AppGallery, and does nothing when this version is live already.
// A re-run uploads the APKs again (AppGallery parses each upload; binding replaces the draft's).
//
// listing: the app's name (its label in the APK: AppGallery wants the name the app shows) and the
// texts of Google Play's listing (fastlane/metadata/android: short and full description) in
// Huawei's languages (HUAWEI below), with --images the icon and each language's
// phone screenshots from a folder like target/store/images (the "Store listing" workflow's
// artifact). Into the version being prepared; never submitted (the next release submits it).
//
// --dry-run: the local checks and the plan, no request to Huawei (the service account's key is
// still checked, when there is one).
//
// The Service Account (AppGallery Connect, Users and permissions, API key, Connect API) in the
// environment: HUAWEI_SERVICE_ACCOUNT_JSON, the credential file AppGallery Connect downloads when
// it is created (`<...>private.json`: key_id, sub_account, private_key). The app is found by its
// package name. HUAWEI_APPGALLERY_API points it at another server (the tests' mock).
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { pngInfo } from '../store/assemble.ts';
import { PLAY, PLAY_METADATA } from '../store/locales.ts';
import { type Apk, apks, otherStores } from './galaxy-store.ts';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
export const API = 'https://connect-api.cloud.huawei.com/api';
/** The audience of a Service Account's JWT (Getting Started, Service Account mode). */
export const AUDIENCE = 'https://oauth-login.cloud.huawei.com/oauth2/v3/token';
export const PACKAGE = 'rs.hashi';
const ICON = path.join(ROOT, 'public/icon-512.png');
/** The Android app's strings, its label (app_name) among them. */
const STRINGS = path.join(ROOT, 'app/src-tauri/gen/android/app/src/main/res/values/strings.xml');

/**
 * Every language AppGallery's listings take (Publishing API reference, Languages). Each Google
 * Play locale of ours that is one of them gets its texts and screenshots: all of them but fr-CA
 * and es-US, which AppGallery doesn't have (French and the US Spanish read fr-FR's and es-419's).
 */
export const HUAWEI_KNOWN =
  `ar az-AZ eu-ES be bn-BD bs bg my-MM ca zh-CN zh-TW zh-HK hr cs-CZ da-DK nl-NL en-US en-GB et fil
  fi-FI fr-FR gl-ES ka-GE de-DE el-GR he_IL hi-IN hu-HU id it-IT ja-JP jv kk km-KH ko-KR lo-LA lv lt
  mk-MK ms ne-NP no-NO fa pl-PL pt-BR pt-PT ro ru-RU sr si-LK sk sl es-419 es-ES sv-SE th bo tr-TR uk
  ur uz vi`.split(/\s+/);

/**
 * The listing's languages: AppGallery's code (Play's, for all of ours) and the Play locale whose
 * texts it gets. en-US is the default language, which the others fall back to.
 */
export const HUAWEI: readonly { code: string; play: string; site: string }[] = PLAY.filter((p) =>
  HUAWEI_KNOWN.includes(p.locale),
).map((p) => ({ code: p.locale, play: p.same ?? p.locale, site: p.site }));
export const DEFAULT_LANGUAGE = 'en-US';

/**
 * The limits of a language's texts (Updating App Localization Information), in characters; the
 * name's is the review guidelines' (1.1: 30 outside Chinese), the API takes 64.
 */
export const LIMITS = { appName: 30, briefInfo: 80, appDesc: 8000, newFeatures: 500 } as const;
/**
 * The phone screenshots (Specifications for App Visual Assets, APKs): 3 to 8, 450×800 or larger
 * in the same proportions, each side 320 to 3840 px, a PNG of at most 2 MB.
 */
export const SCREENSHOTS = { min: 3, max: 8, minSide: 320, maxSide: 3840, bytes: 2 << 20 } as const;
/** The frame of the store images whose screenshots go to AppGallery: Play's phone, 1080×1920. */
export const SCREENSHOT_FRAME = 'play-phone';
/** The icon: 216×216 or 512×512, a PNG of at most 2 MB. */
const ICON_SIZES = [216, 512];

/** fileType of app-file-info (Updating App File Information). */
export const FILE_TYPE = { icon: 0, screenshots: 2, package: 5 } as const;
/** releaseType of a full release, the whole network (not by phase, 3). */
export const FULL_RELEASE = 1;

/**
 * releaseState values (Querying App Information) in which a version can't be changed: releasing,
 * in review, update in review, removal requested, in pre-review.
 */
export const BUSY: ReadonlyMap<number, string> = new Map([
  [3, 'being released'],
  [4, 'in review'],
  [5, 'in review (an update)'],
  [6, 'waiting for its removal'],
  [12, 'in pre-review'],
]);
/** releaseState of a released app. */
export const RELEASED = 0;

/** The result codes the script tells apart (Publishing API, Result Codes). */
export const RESULT = {
  /** "Failed to query the app information" on app-submit, also while a package is parsed. */
  submitQuery: 204144660,
  /** "The package is being compiled. Please try again 3 to 5 minutes later." */
  compiling: 204144727,
  /** "Failed to perform pre-check before submitting the app for review." */
  precheck: 204144664,
  /** Too many APKs (300 for an app ID). */
  uploadApk: 204144662,
  /** The account isn't allowed: suspended, or its identity not verified. */
  suspended: 204144666,
  /** Inconsistent signatures of the APKs, or of the versions. */
  signatures: 204144722,
  versionSignatures: 204144723,
} as const;
/**
 * The sub-codes of submitQuery that are for good (an App Bundle's problems), and the words of the
 * ones that only mean "still parsing, try again" (as Mozilla's client has seen them).
 */
const PERMANENT_SUB_CODES = /\b802101(?:0\d|16)\b/;
const PARSING = ['is being processed', 'it may take', 'parsing', '解析中'];
/** app-submit takes a call every two minutes at most (Publishing API, Version Change History). */
export const SUBMIT_RETRY = { delayMs: 120_000, timeoutMs: 600_000 } as const;

const chars = (text: string) => [...text].length;

// --- Authentication ------------------------------------------------------------------------

export interface ServiceAccount {
  keyId: string;
  subAccount: string;
  key: crypto.KeyObject;
}

/** The Service Account from its credential file (the JSON AppGallery Connect downloads). */
export function serviceAccount(text: string): ServiceAccount {
  let json: unknown;
  try {
    json = JSON.parse(text);
  } catch (cause) {
    throw new Error(
      "HUAWEI_SERVICE_ACCOUNT_JSON isn't JSON (the Service Account's credential file, <...>private.json)",
      { cause },
    );
  }
  const field = (name: string) =>
    isObject(json) && typeof json[name] === 'string' ? json[name] : '';
  const keyId = field('key_id');
  const subAccount = field('sub_account');
  const pem = field('private_key').replaceAll('\\n', '\n');
  if (keyId === '' || subAccount === '' || pem === '')
    throw new Error(
      'HUAWEI_SERVICE_ACCOUNT_JSON has no key_id, sub_account or private_key: the credential file of a Service Account, not of an API client',
    );
  let key: crypto.KeyObject;
  try {
    key = crypto.createPrivateKey(pem);
  } catch (cause) {
    throw new Error("HUAWEI_SERVICE_ACCOUNT_JSON's private_key isn't a private key", { cause });
  }
  if (key.asymmetricKeyType !== 'rsa')
    throw new Error(
      `HUAWEI_SERVICE_ACCOUNT_JSON's private_key isn't an RSA key (${key.asymmetricKeyType})`,
    );
  return { keyId, subAccount, key };
}

/**
 * The bearer token of a request: a JWT the Service Account signs itself (PS256), valid for the
 * hour Huawei allows, issued a minute back for a server clock that is behind.
 */
export function jwt(account: ServiceAccount, now = Date.now()): string {
  const iat = Math.floor(now / 1000) - 60;
  const part = (value: object) => Buffer.from(JSON.stringify(value)).toString('base64url');
  const unsigned = `${part({ kid: account.keyId, typ: 'JWT', alg: 'PS256' })}.${part({
    aud: AUDIENCE,
    iss: account.subAccount,
    iat,
    exp: iat + 3600,
  })}`;
  const signature = crypto
    .sign('sha256', Buffer.from(unsigned), {
      key: account.key,
      padding: crypto.constants.RSA_PKCS1_PSS_PADDING,
      saltLength: crypto.constants.RSA_PSS_SALTLEN_DIGEST,
    })
    .toString('base64url');
  return `${unsigned}.${signature}`;
}

// --- Requests and responses ----------------------------------------------------------------

export class HuaweiError extends Error {
  readonly status: number;
  /** ret.code of the answer, null when there is none. */
  readonly code: number | null;
  readonly msg: string;
  constructor(message: string, status: number, code: number | null, msg = '') {
    super(message);
    this.status = status;
    this.code = code;
    this.msg = msg;
  }
}

const isObject = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

/** The ret of an answer, `{code, msg}`: null when it has none. */
export function retOf(body: unknown): { code: number; msg: string } | null {
  if (!(isObject(body) && isObject(body.ret))) return null;
  return { code: Number(body.ret.code), msg: String(body.ret.msg ?? '') };
}

/** What to do about an error, for the ones that need the owner. */
function hint(status: number, code: number | null): string {
  if (status === 401 || status === 403)
    return ' (HUAWEI_SERVICE_ACCOUNT_JSON: a developer-level Service Account with the App administrator role, and a key it still has)';
  if (code === RESULT.suspended)
    return " (the developer account isn't verified or is suspended: AppGallery Connect, Account center)";
  if (code === RESULT.signatures || code === RESULT.versionSignatures)
    return ' (every APK has to be signed with the direct key, as the version on AppGallery is)';
  if (code === RESULT.precheck)
    return ' (something the review needs is missing: complete the version in AppGallery Connect, then submit it there or run this again)';
  if (code === RESULT.uploadApk)
    return ' (AppGallery keeps at most 300 APKs of an app: delete old packages of draft versions in AppGallery Connect)';
  return '';
}

export interface Connection {
  fetch: typeof fetch;
  api: string;
  account: ServiceAccount;
  /** Waits between attempts of app-submit (the tests' don't). */
  sleep: (ms: number) => Promise<void>;
  /** The clock of the retries. */
  now: () => number;
}

export interface Request {
  query?: Record<string, string | number>;
  json?: unknown;
  /** The answer with a ret.code other than 0, instead of an error (app-submit's retries). */
  anyRet?: boolean;
}

/** A call of the Publishing API (a path below /api) with a fresh token. */
export async function call(
  c: Connection,
  what: string,
  method: 'GET' | 'POST' | 'PUT',
  sub: string,
  r: Request = {},
): Promise<Record<string, unknown>> {
  const url = new URL(`${c.api}${sub}`);
  for (const [k, v] of Object.entries(r.query ?? {})) url.searchParams.set(k, String(v));
  const headers: Record<string, string> = {
    Authorization: `Bearer ${jwt(c.account)}`,
    Accept: 'application/json',
  };
  if (r.json !== undefined) headers['Content-Type'] = 'application/json';
  const response = await c.fetch(url, {
    method,
    headers,
    ...(r.json === undefined ? {} : { body: JSON.stringify(r.json) }),
  });
  const raw = await response.text();
  let body: unknown = raw;
  try {
    body = raw === '' ? {} : JSON.parse(raw);
  } catch {
    // Not JSON: the error says what it is
  }
  const ret = retOf(body);
  if (!(response.ok && isObject(body))) {
    const msg = ret?.msg ?? (typeof body === 'string' ? body.slice(0, 500) : '');
    throw new HuaweiError(
      `${what}: HTTP ${response.status}${msg === '' ? '' : `: ${msg}`}${hint(response.status, ret?.code ?? null)}`,
      response.status,
      ret?.code ?? null,
      msg,
    );
  }
  if (ret !== null && ret.code !== 0 && r.anyRet !== true)
    throw new HuaweiError(
      `${what}: ${ret.code} ${ret.msg}${hint(response.status, ret.code)}`,
      response.status,
      ret.code,
      ret.msg,
    );
  return body;
}

// --- The app -------------------------------------------------------------------------------

/** The app's ID, by its package name: the app has to be created in AppGallery Connect first. */
export async function appId(c: Connection): Promise<string> {
  const body = await call(c, 'appid-list', 'GET', '/publish/v2/appid-list', {
    query: { packageName: PACKAGE },
  });
  const ids = (Array.isArray(body.appids) ? body.appids : [])
    .filter(isObject)
    .map((p) => String(p.value ?? ''))
    .filter((id) => id !== '');
  if (ids.length === 0)
    throw new Error(
      `AppGallery has no app ${PACKAGE} for this account: create it in AppGallery Connect (docs/stores.md, Huawei AppGallery)`,
    );
  if (ids.length > 1) throw new Error(`AppGallery has ${ids.length} apps ${PACKAGE}: ${ids}`);
  return ids[0] ?? '';
}

export interface AppState {
  id: string;
  releaseState: number | null;
  /** The version being prepared (or the released one, when there is no draft). */
  versionNumber: string | null;
  versionCode: number | null;
  /** The version on AppGallery. */
  onShelfVersionNumber: string | null;
  onShelfVersionCode: number | null;
  /** The listing's languages. */
  languages: { lang: string; newFeatures: string | null }[];
}

const numberOr = (value: unknown): number | null =>
  value === null || value === undefined || value === '' || Number.isNaN(Number(value))
    ? null
    : Number(value);
const stringOr = (value: unknown): string | null =>
  value === null || value === undefined || value === '' ? null : String(value);

/** The app's state in app-info's answer. */
export function appState(id: string, body: Record<string, unknown>): AppState {
  const info = isObject(body.appInfo) ? body.appInfo : {};
  return {
    id,
    releaseState: numberOr(info.releaseState),
    versionNumber: stringOr(info.versionNumber),
    versionCode: numberOr(info.versionCode),
    onShelfVersionNumber: stringOr(info.onShelfVersionNumber),
    onShelfVersionCode: numberOr(info.onShelfVersionCode),
    languages: (Array.isArray(body.languages) ? body.languages : [])
      .filter(isObject)
      .map((l) => ({ lang: String(l.lang ?? ''), newFeatures: stringOr(l.newFeatures) }))
      .filter((l) => l.lang !== ''),
  };
}

export async function getApp(c: Connection, id: string): Promise<AppState> {
  return appState(
    id,
    await call(c, 'app-info', 'GET', '/publish/v2/app-info', { query: { appId: id } }),
  );
}

/** Why the app's version can't be changed now, null when it can. */
export function busy(a: AppState): string | null {
  const what = a.releaseState === null ? undefined : BUSY.get(a.releaseState);
  return what === undefined
    ? null
    : `The app is ${what} (releaseState ${a.releaseState}): AppGallery Connect shows where it is. Wait for it, or cancel the review there, then run this again`;
}

// --- Uploads -------------------------------------------------------------------------------

/**
 * A file onto Huawei's file server (Obtaining the File Upload URL, Uploading a File): its object
 * ID, which app-file-info binds to the app. The upload URL lasts about five minutes and comes with
 * the headers its signature covers; the file isn't for the Chinese mainland.
 */
export async function upload(c: Connection, id: string, file: string): Promise<string> {
  const name = path.basename(file);
  const size = fs.statSync(file).size;
  const sha256 = crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
  const body = await call(c, `upload-url ${name}`, 'GET', '/publish/v2/upload-url/for-obs', {
    query: {
      appId: id,
      fileName: name,
      contentLength: size,
      sha256,
      releaseType: FULL_RELEASE,
      chineseMainlandFlag: 0,
    },
  });
  const info = isObject(body.urlInfo) ? body.urlInfo : {};
  const url = stringOr(info.url);
  const objectId = stringOr(info.objectId);
  if (url === null || objectId === null)
    throw new HuaweiError(`upload-url ${name}: no url or objectId in the answer`, 200, null);
  const headers: Record<string, string> = {};
  if (isObject(info.headers))
    for (const [k, v] of Object.entries(info.headers)) headers[k] = String(v);
  if (!Object.keys(headers).some((k) => k.toLowerCase() === 'content-type'))
    headers['Content-Type'] = 'application/octet-stream';
  const response = await c.fetch(url, {
    method: stringOr(info.method) ?? 'PUT',
    headers,
    body: await fs.openAsBlob(file),
  });
  if (!response.ok)
    throw new HuaweiError(
      `upload ${name}: HTTP ${response.status}: ${(await response.text()).slice(0, 500)}`,
      response.status,
      null,
    );
  return objectId;
}

/** Binds uploaded files to the app's version being prepared (Updating App File Information). */
export async function bindFiles(
  c: Connection,
  id: string,
  what: string,
  json: Record<string, unknown>,
): Promise<Record<string, unknown>> {
  return await call(c, `app-file-info ${what}`, 'PUT', '/publish/v2/app-file-info', {
    query: { appId: id },
    json,
  });
}

// --- Release -------------------------------------------------------------------------------

/**
 * The release notes as every language's "new features": "Hashi <version>" when they name another
 * store (AppGallery turns those away, and its app has no Play Games), refused over Huawei's limit.
 */
export function releaseNotes(notes: string, version: string): { text: string; note?: string } {
  const stores = otherStores(notes);
  if (stores.length > 0)
    return {
      text: `Hashi ${version}`,
      note: `The release notes name ${stores.join(', ')}: AppGallery's new features are "Hashi ${version}"`,
    };
  if (chars(notes) > LIMITS.newFeatures)
    throw new Error(
      `the release notes have ${chars(notes)} characters, AppGallery takes ${LIMITS.newFeatures}`,
    );
  return { text: notes === '' ? `Hashi ${version}` : notes };
}

/** The APKs AppGallery takes: a 64-bit one has to be there (Releasing an App, APK). */
export function checkApks(list: Apk[]): void {
  if (!list.some((a) => a.abi === 'arm64-v8a'))
    throw new Error(
      'no arm64-v8a APK: AppGallery takes a phone app only with a 64-bit package (HUAWEI_APPGALLERY_ABIS)',
    );
}

/** What AppGallery has, against this version: null to go on, else why there's nothing to do. */
export function versionCheck(a: AppState, list: Apk[], code: number): string | null {
  const live = a.onShelfVersionCode;
  if (live === null) return null;
  if (Math.floor(live / 10) > code)
    throw new Error(
      `AppGallery has a newer version, ${a.onShelfVersionNumber ?? '?'} (versionCode ${live})`,
    );
  if (list.some((apk) => apk.versionCode === live))
    return `AppGallery has ${a.onShelfVersionNumber ?? 'this version'} (versionCode ${live}) on sale already: nothing to do`;
  return null;
}

export type Log = (line: string) => void;

export interface ReleaseOptions {
  version: string;
  code: number;
  apks: Apk[];
  newFeatures: string;
  submit: boolean;
}

/** Whether app-submit's answer only means "the packages are still being parsed". */
export function stillParsing(ret: { code: number; msg: string }): boolean {
  if (ret.code === RESULT.compiling) return true;
  if (ret.code !== RESULT.submitQuery || PERMANENT_SUB_CODES.test(ret.msg)) return false;
  const msg = ret.msg.toLowerCase();
  return PARSING.some((p) => msg.includes(p));
}

/**
 * Submits the version for review as a full release, once AppGallery has parsed its packages:
 * every two minutes (its least interval) for up to ten while it says it is still parsing them.
 */
export async function submit(c: Connection, id: string, log: Log): Promise<void> {
  const deadline = c.now() + SUBMIT_RETRY.timeoutMs;
  for (;;) {
    const body = await call(c, 'app-submit', 'POST', '/publish/v2/app-submit', {
      query: { appId: id, releaseType: FULL_RELEASE },
      anyRet: true,
    });
    const ret = retOf(body);
    if (ret === null || ret.code === 0) return;
    if (!stillParsing(ret) || c.now() + SUBMIT_RETRY.delayMs > deadline)
      throw new HuaweiError(
        `app-submit: ${ret.code} ${ret.msg}${hint(200, ret.code)}${stillParsing(ret) ? ' (still parsing after ten minutes: the packages are bound, submit the version in AppGallery Connect)' : ''}`,
        200,
        ret.code,
        ret.msg,
      );
    log(`AppGallery is still parsing the packages (${ret.msg}), submitting again in two minutes`);
    await c.sleep(SUBMIT_RETRY.delayMs);
  }
}

/** The release: the APKs as the next version's packages, submitted with `submit`. Its summary. */
export async function release(c: Connection, o: ReleaseOptions, log: Log): Promise<string[]> {
  checkApks(o.apks);
  const id = await appId(c);
  const a = await getApp(c, id);
  log(
    `The app ${id}: releaseState ${a.releaseState ?? '?'}, on sale ${a.onShelfVersionNumber ?? 'nothing'} (${a.onShelfVersionCode ?? '-'}), being prepared ${a.versionNumber ?? '-'} (${a.versionCode ?? '-'})`,
  );
  const problem = busy(a);
  if (problem !== null) throw new Error(problem);
  const done = versionCheck(a, o.apks, o.code);
  if (done !== null) return [`AppGallery: ${done}`];

  // The foss build has no Google services: for Huawei's devices without them
  await call(c, 'properties/gms', 'PUT', '/publish/v2/properties/gms', {
    query: { appId: id },
    json: { needGms: 0 },
  });
  const files: { fileName: string; fileDestUrl: string }[] = [];
  for (const apk of o.apks) {
    const objectId = await upload(c, id, apk.file);
    files.push({ fileName: path.basename(apk.file), fileDestUrl: objectId });
    log(`Uploaded ${path.basename(apk.file)} (versionCode ${apk.versionCode})`);
  }
  const bound = await bindFiles(c, id, 'APKs', { fileType: FILE_TYPE.package, files });
  const packages = Array.isArray(bound.pkgVersion) ? bound.pkgVersion.map(String) : [];
  log(`Bound ${files.length} APKs to the version${packages.length > 0 ? ` (${packages})` : ''}`);

  const languages =
    a.languages.length > 0 ? a.languages : [{ lang: DEFAULT_LANGUAGE, newFeatures: null }];
  let changed = 0;
  for (const l of languages) {
    if (l.newFeatures === o.newFeatures) continue;
    await call(c, `app-language-info ${l.lang}`, 'PUT', '/publish/v2/app-language-info', {
      query: { appId: id },
      json: { lang: l.lang, newFeatures: o.newFeatures },
    });
    changed++;
  }
  log(`New features in ${changed} of ${languages.length} languages`);

  const summary = [
    `AppGallery: ${o.version} as the packages ${o.apks.map((apk) => `${apk.abi} (${apk.versionCode})`).join(', ')} of the app ${id}, without Google services`,
  ];
  if (o.submit) {
    await submit(c, id, log);
    summary.push('Submitted for review (a full release). It goes live after it.');
  } else {
    summary.push(
      'Not submitted (HUAWEI_APPGALLERY_SUBMIT is false): submit the version in AppGallery Connect.',
    );
  }
  return summary;
}

// --- Listing -------------------------------------------------------------------------------

export interface ListingText {
  code: string;
  play: string;
  appName: string;
  briefInfo: string;
  appDesc: string;
}

/**
 * The app's name on AppGallery: its label in the APK (app_name). AppGallery's review wants the name
 * the app shows (Review Guidelines 1.20), no keywords or descriptions in it (1.2), so not Play's
 * title ("Hashi: Daily Bridges Puzzle"). The app has one label for every language.
 */
export function apkLabel(strings = STRINGS): string {
  const label = /<string name="app_name">"?([^"<]*)"?<\/string>/.exec(
    fs.readFileSync(strings, 'utf8'),
  )?.[1];
  if (label === undefined || label.trim() === '')
    throw new Error(`${path.relative(ROOT, strings)} has no app_name`);
  return label.trim();
}

/** The texts of each AppGallery language: the app's name, and Google Play's descriptions. */
export function listingTexts(metadata = path.join(ROOT, PLAY_METADATA)): ListingText[] {
  const read = (locale: string, file: string) =>
    fs.readFileSync(path.join(metadata, locale, file), 'utf8').trim();
  const name = apkLabel();
  return HUAWEI.map(({ code, play }) => ({
    code,
    play,
    appName: name,
    briefInfo: read(play, 'short_description.txt'),
    appDesc: read(play, 'full_description.txt'),
  }));
}

/** A text's problems with Huawei's limits and rules (no other stores). */
export function textProblems(t: ListingText): string[] {
  const at = `${t.code} (${t.play})`;
  const problems: string[] = [];
  const stores = otherStores([t.appName, t.briefInfo, t.appDesc].join('\n'));
  if (stores.length > 0) problems.push(`${at}: names ${stores.join(', ')}`);
  for (const [field, label] of [
    ['appName', 'the name'],
    ['briefInfo', 'the short description'],
    ['appDesc', 'the description'],
  ] as const) {
    const n = chars(t[field]);
    if (n === 0) problems.push(`${at}: no ${label.replace(/^the /, '')}`);
    if (n > LIMITS[field])
      problems.push(`${at}: ${label} has ${n} characters, AppGallery takes ${LIMITS[field]}`);
  }
  return problems;
}

export interface ListingPlan {
  texts: ListingText[];
  /** Languages left as AppGallery has them, and why. */
  skipped: string[];
}

/** Which languages get their texts: those within Huawei's limits; the default one has to. */
export function planListing(texts: ListingText[]): ListingPlan {
  const plan: ListingPlan = { texts: [], skipped: [] };
  for (const t of texts) {
    const problems = textProblems(t);
    if (t.code === DEFAULT_LANGUAGE && problems.length > 0)
      throw new Error(`the default language's texts: ${problems.join('; ')}`);
    if (problems.length > 0) plan.skipped.push(`${problems.join('; ')}: kept as it is`);
    else plan.texts.push(t);
  }
  return plan;
}

/** An image's problems with AppGallery's rules for phone screenshots. */
export function screenshotProblems(
  file: string,
  width: number,
  height: number,
  size: number,
): string[] {
  const s = SCREENSHOTS;
  const name = path.relative(ROOT, file) || file;
  const problems: string[] = [];
  if (Math.min(width, height) < s.minSide || Math.max(width, height) > s.maxSide)
    problems.push(
      `${name} is ${width}×${height}, each side has to be ${s.minSide} to ${s.maxSide} px`,
    );
  else if (width * 16 !== height * 9)
    problems.push(`${name} is ${width}×${height}, not upright 9:16 (as 450×800)`);
  if (size > s.bytes)
    problems.push(`${name} has ${(size / 1048576).toFixed(1)} MB, AppGallery takes 2 MB`);
  return problems;
}

export interface ListingImages {
  icon: string;
  /** By AppGallery's language code. */
  screenshots: Map<string, string[]>;
}

/** The listing's images from an images folder, checked. */
export function listingImages(images: string, icon = ICON): ListingImages {
  const problems: string[] = [];
  const i = pngInfo(icon);
  if (i.width !== i.height || !ICON_SIZES.includes(i.width))
    problems.push(`${path.relative(ROOT, icon)} is ${i.width}×${i.height}, not 216×216 or 512×512`);
  if (fs.statSync(icon).size > SCREENSHOTS.bytes)
    problems.push(`${path.relative(ROOT, icon)} is over 2 MB`);
  const screenshots = new Map<string, string[]>();
  for (const { code, site } of HUAWEI) {
    const dir = path.join(images, site, SCREENSHOT_FRAME);
    const files = fs.existsSync(dir)
      ? fs
          .readdirSync(dir)
          .filter((f) => f.endsWith('.png'))
          .sort((a, b) => Number.parseInt(a, 10) - Number.parseInt(b, 10))
          .map((f) => path.join(dir, f))
      : [];
    if (files.length < SCREENSHOTS.min || files.length > SCREENSHOTS.max)
      problems.push(
        `${code}: ${files.length} screenshots in ${path.relative(ROOT, dir) || dir}, AppGallery takes ${SCREENSHOTS.min} to ${SCREENSHOTS.max}`,
      );
    for (const f of files) {
      const { width, height } = pngInfo(f);
      problems.push(...screenshotProblems(f, width, height, fs.statSync(f).size));
    }
    screenshots.set(code, files);
  }
  if (problems.length > 0) throw new Error(`the images have problems:\n${problems.join('\n')}`);
  return { icon, screenshots };
}

/** The listing into the version being prepared: texts, and the images when there are. */
export async function listing(
  c: Connection,
  texts: ListingText[],
  images: ListingImages | null,
  log: Log,
): Promise<string[]> {
  const id = await appId(c);
  const a = await getApp(c, id);
  const problem = busy(a);
  if (problem !== null) throw new Error(problem);
  const plan = planListing(texts);
  const done: string[] = [];
  for (const t of plan.texts) {
    try {
      await call(c, `app-language-info ${t.code}`, 'PUT', '/publish/v2/app-language-info', {
        query: { appId: id },
        json: { lang: t.code, appName: t.appName, appDesc: t.appDesc, briefInfo: t.briefInfo },
      });
    } catch (e) {
      // A language the app doesn't have yet, which Huawei didn't add: added in AppGallery Connect
      if (t.code === DEFAULT_LANGUAGE || a.languages.some((l) => l.lang === t.code)) throw e;
      if (!(e instanceof HuaweiError)) throw e;
      plan.skipped.push(
        `${t.code} (${t.play}): AppGallery didn't add the language (${e.message}): add it in AppGallery Connect (App information, Manage languages), then run this again`,
      );
      continue;
    }
    if (images !== null) {
      if (t.code === DEFAULT_LANGUAGE) {
        const icon = await upload(c, id, images.icon);
        await bindFiles(c, id, `icon ${t.code}`, {
          lang: t.code,
          fileType: FILE_TYPE.icon,
          files: [{ fileName: path.basename(images.icon), fileDestUrl: icon }],
        });
      }
      const shots = images.screenshots.get(t.code) ?? [];
      const files: { fileName: string; fileDestUrl: string }[] = [];
      for (const f of shots)
        files.push({ fileName: path.basename(f), fileDestUrl: await upload(c, id, f) });
      await bindFiles(c, id, `screenshots ${t.code}`, {
        lang: t.code,
        fileType: FILE_TYPE.screenshots,
        imgShowType: 0,
        files,
      });
      log(`${t.code}: the texts and ${shots.length} screenshots`);
    } else {
      log(`${t.code}: the texts`);
    }
    done.push(t.code);
  }
  return [
    `AppGallery: the listing of the app ${id} in ${done.length} languages (${done.join(', ')})${images === null ? ', texts only' : ', with the icon and the phone screenshots'} (not submitted: the next release submits it, or AppGallery Connect)`,
    ...plan.skipped.map((s) => `Not changed: ${s}`),
  ];
}

// --- Command line --------------------------------------------------------------------------

const USAGE = `usage: node scripts/huawei-appgallery.ts release --version <x.y.z> --code <n> [--whatsnew <file>] [--submit] [--dry-run] <apk>...
       node scripts/huawei-appgallery.ts listing [--images <dir>] [--dry-run]`;

function usage(): never {
  console.error(USAGE);
  process.exit(2);
}

/** The Service Account from the environment: checked unless a dry run without one. */
function environment(env: NodeJS.ProcessEnv, dryRun: boolean): ServiceAccount | null {
  const text = env.HUAWEI_SERVICE_ACCOUNT_JSON ?? '';
  if (text === '') {
    if (dryRun) return null;
    throw new Error('HUAWEI_SERVICE_ACCOUNT_JSON not set');
  }
  const account = serviceAccount(text);
  jwt(account);
  return account;
}

function connect(account: ServiceAccount, env: NodeJS.ProcessEnv): Connection {
  return {
    fetch,
    api: env.HUAWEI_APPGALLERY_API ?? API,
    account,
    sleep: (ms) =>
      new Promise((resolve) => {
        setTimeout(resolve, ms);
      }),
    now: () => Date.now(),
  };
}

async function main(args: string[], env: NodeJS.ProcessEnv): Promise<string[]> {
  const command = args.shift();
  const flag = (name: string) => {
    const i = args.indexOf(name);
    if (i !== -1) args.splice(i, 1);
    return i !== -1;
  };
  const option = (name: string) => {
    const i = args.indexOf(name);
    if (i === -1) return undefined;
    const value = args[i + 1];
    if (value === undefined) usage();
    args.splice(i, 2);
    return value;
  };
  const dryRun = flag('--dry-run');
  const log: Log = (line) => console.log(line);

  if (command === 'release') {
    const submitting = flag('--submit');
    const version = option('--version');
    const code = Number(option('--code'));
    const whatsnew = option('--whatsnew');
    if (version === undefined || !Number.isInteger(code) || code <= 0 || args.length === 0) usage();
    const list = apks(args, version, code);
    for (const apk of list) fs.accessSync(apk.file);
    checkApks(list);
    const notes = releaseNotes(
      whatsnew === undefined ? '' : fs.readFileSync(whatsnew, 'utf8').trim(),
      version,
    );
    const lines = notes.note === undefined ? [] : [notes.note];
    const account = environment(env, dryRun);
    if (dryRun || account === null) {
      return [
        ...lines,
        `AppGallery, dry run: ${version} would become the packages ${list.map((a) => `${path.basename(a.file)} (versionCode ${a.versionCode}, ${(fs.statSync(a.file).size / 1048576).toFixed(1)} MB)`).join(', ')} of ${PACKAGE}, without Google services, the new features "${notes.text.slice(0, 80)}${notes.text.length > 80 ? '...' : ''}"${submitting ? ', then submitted for review' : ', not submitted'}`,
      ];
    }
    return lines.concat(
      await release(
        connect(account, env),
        { version, code, apks: list, newFeatures: notes.text, submit: submitting },
        log,
      ),
    );
  }

  if (command === 'listing') {
    const imagesDir = option('--images');
    if (args.length > 0) usage();
    const texts = listingTexts();
    const problems = texts.flatMap(textProblems);
    // The default language's problems stop it here already
    planListing(texts);
    const images = imagesDir === undefined ? null : listingImages(imagesDir);
    const account = environment(env, dryRun);
    if (dryRun || account === null) {
      return [
        `AppGallery, dry run: the listing in ${texts.length} languages${images === null ? ', texts only' : `, with the icon and ${[...images.screenshots.values()].reduce((n, s) => n + s.length, 0)} screenshots`}`,
        ...problems.map((p) => `Would keep AppGallery's text: ${p}`),
      ];
    }
    return await listing(connect(account, env), texts, images, log);
  }
  usage();
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    const summary = await main(process.argv.slice(2), process.env);
    for (const line of summary) console.log(line);
    if (process.env.GITHUB_STEP_SUMMARY)
      fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${summary.join('\n\n')}\n`);
  } catch (e) {
    console.log(`::error::${e instanceof Error ? e.message : e}`);
    process.exit(1);
  }
}
