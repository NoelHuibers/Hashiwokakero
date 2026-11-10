// Uploads the app to the Samsung Galaxy Store (docs/stores.md, Galaxy Store) through Samsung's
// Content Publish API (https://developer.samsung.com/galaxy-store/galaxy-store-developer-api):
//
//   node scripts/galaxy-store.ts release --version <x.y.z> --code <n> [--whatsnew <file>]
//                                        [--submit] [--dry-run] <apk>...
//   node scripts/galaxy-store.ts listing [--images <dir>] [--dry-run]
//
// release: the foss APKs `hashi-<version>-foss-<abi>.apk` become the binaries of an update of the
// app: it opens one (contentUpdate, with --whatsnew as its "what's new"), or takes the one a run
// before left open, uploads the APKs, deletes the update's binaries of older versions, adds the new
// ones (64-bit first) and with --submit submits the update for review. A re-run skips the binaries
// the update already has. It refuses an app in review, and an update with a newer version.
//
// listing: the texts of Google Play's listing (fastlane/metadata/android: title, full
// description) in Samsung's languages (GALAXY below), with --images the icon and each language's
// phone screenshots from a folder like target/store/images (the "Store listing" workflow's
// artifact). Into the open update, or a new one; never submitted (the next release submits it).
//
// --dry-run: the local checks and the plan, no request to Samsung (the key is still checked, when
// there is one).
//
// The service account (Seller Portal, Assistance, API Service) in the environment:
// SAMSUNG_SERVICE_ACCOUNT_ID, SAMSUNG_PRIVATE_KEY (its private key, PEM) and SAMSUNG_CONTENT_ID
// (the app's 12-digit content ID). GALAXY_STORE_API and GALAXY_STORE_UPLOAD_URL point it at
// another server (the tests' mock).
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { pngInfo } from '../store/assemble.ts';
import { PLAY, PLAY_METADATA } from '../store/locales.ts';
import { ABI_DIGITS, fossVersionCode } from './foss-apk.ts';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
export const API = 'https://devapi.samsungapps.com';
/** File uploads have a host of their own (File Upload). */
export const UPLOAD_URL = 'https://seller.samsungapps.com/galaxyapi/fileUpload';
export const PACKAGE = 'rs.hashi';
const ICON = path.join(ROOT, 'public/icon-512.png');

/**
 * The listing's languages: Samsung's code (Content Publish API reference, Language codes) and the
 * Google Play locale whose texts and screenshots it gets. ENG is the default language. Samsung has
 * one Spanish and one Portuguese: Latin America's and Brazil's, the larger markets. Hindi has no
 * listing language there.
 */
export const GALAXY: readonly { code: string; play: string }[] = [
  { code: 'ENG', play: 'en-US' },
  { code: 'DEU', play: 'de-DE' },
  { code: 'FRA', play: 'fr-FR' },
  { code: 'ITA', play: 'it-IT' },
  { code: 'SPA', play: 'es-419' },
  { code: 'POR', play: 'pt-BR' },
  { code: 'NLD', play: 'nl-NL' },
  { code: 'POL', play: 'pl-PL' },
  { code: 'TUR', play: 'tr-TR' },
  { code: 'RUS', play: 'ru-RU' },
  { code: 'UKR', play: 'uk' },
  { code: 'ARA', play: 'ar' },
  { code: 'IND', play: 'id' },
  { code: 'VIE', play: 'vi' },
  { code: 'THA', play: 'th' },
  { code: 'JPN', play: 'ja-JP' },
  { code: 'KOR', play: 'ko-KR' },
  { code: 'ZHO', play: 'zh-CN' },
  { code: '002', play: 'zh-TW' },
];
export const DEFAULT_LANGUAGE = 'ENG';
/** The Chinese listings, which need China's age rating (chinaAgeLimit). */
const CHINESE = new Set(['ZHO', '002']);

/** Samsung's limits of the texts, in UTF-8 bytes (Content Publish API reference). */
export const LIMITS = { appTitle: 100, description: 4000, newFeature: 4000 } as const;
/** Screenshots: 4 to 8, PNG or JPG, each side 320 to 3840 px, at most 2:1. The icon: 512×512 PNG. */
export const SCREENSHOTS = { min: 4, max: 8, minSide: 320, maxSide: 3840, ratio: 2 } as const;
const ICON_BYTES = 1024 * 1024;

/**
 * The statuses of an app with an update open (Seller Portal: Registering, Updating,
 * Re-Registering): binaries can be added and deleted, and it can be submitted.
 */
export const OPEN = new Set(['REGISTERING', 'UPDATING', 'RE_REGISTERING']);
/** On sale and nothing open: contentUpdate opens an update. */
export const ON_SALE = 'FOR_SALE';

const bytes = (text: string) => Buffer.byteLength(text, 'utf8');

// --- Authentication ------------------------------------------------------------------------

/**
 * The service account's private key, from the secret: the PEM file as Seller Portal gives it,
 * also with its line breaks as `\n`, or only its base64 body (PKCS #8).
 */
export function privateKey(text: string): crypto.KeyObject {
  let pem = text.trim().replaceAll('\\n', '\n');
  if (!pem.includes('-----BEGIN')) {
    const body = pem.replace(/\s+/g, '');
    pem = `-----BEGIN PRIVATE KEY-----\n${body.match(/.{1,64}/g)?.join('\n') ?? ''}\n-----END PRIVATE KEY-----\n`;
  }
  let key: crypto.KeyObject;
  try {
    key = crypto.createPrivateKey(pem);
  } catch (cause) {
    throw new Error(
      "SAMSUNG_PRIVATE_KEY isn't a private key (the PEM file of the service account)",
      { cause },
    );
  }
  if (key.asymmetricKeyType !== 'rsa')
    throw new Error(`SAMSUNG_PRIVATE_KEY is a ${key.asymmetricKeyType} key, not RSA`);
  return key;
}

/**
 * The JSON Web Token that buys an access token (Create an Access Token): RS256, issued by the
 * service account for the publishing scope, valid for 15 minutes (Samsung allows 20). Issued a
 * minute back, for a server clock that is behind.
 */
export function jwt(serviceAccountId: string, key: crypto.KeyObject, now = Date.now()): string {
  const iat = Math.floor(now / 1000) - 60;
  const part = (value: object) => Buffer.from(JSON.stringify(value)).toString('base64url');
  const unsigned = `${part({ alg: 'RS256', typ: 'JWT' })}.${part({
    iss: serviceAccountId,
    scopes: ['publishing'],
    iat,
    exp: iat + 15 * 60,
  })}`;
  const signature = crypto.sign('sha256', Buffer.from(unsigned), key).toString('base64url');
  return `${unsigned}.${signature}`;
}

// --- Requests and responses ----------------------------------------------------------------

export class GalaxyError extends Error {
  readonly status: number;
  readonly code: string | null;
  constructor(message: string, status: number, code: string | null) {
    super(message);
    this.status = status;
    this.code = code;
  }
}

const isObject = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown): string | null =>
  value === null || value === undefined || value === '' ? null : String(value);

/**
 * The error of a response, none when it succeeded. The API answers in several shapes: the binary
 * endpoints `{resultCode: "0000", resultMessage}`, contentUpdate and fileUpload `{errorCode: null,
 * errorMsg: null, ...}`, the authentication `{ok: true, ...}`; errors also as `{code, message}`
 * or `{error: {code, message}}`.
 */
export function responseError(
  status: number,
  body: unknown,
): { code: string | null; message: string } | null {
  const b = isObject(body) ? body : {};
  const inner = isObject(b.error) ? b.error : {};
  const code = text(b.resultCode) ?? text(b.errorCode) ?? text(inner.code) ?? text(b.code) ?? null;
  const message =
    text(b.resultMessage) ??
    text(b.errorMsg) ??
    text(inner.message) ??
    text(b.message) ??
    (typeof body === 'string' && body !== '' ? body.slice(0, 500) : null);
  const failed =
    status >= 400 ||
    (b.resultCode !== undefined && text(b.resultCode) !== '0000') ||
    text(b.errorCode) !== null ||
    b.ok === false;
  if (!failed) return null;
  return { code, message: message ?? `HTTP ${status}` };
}

/** What to do about an error, for the ones that need the owner. */
function hint(status: number, code: string | null): string {
  if (status === 401)
    return ' (the service account or its key: SAMSUNG_SERVICE_ACCOUNT_ID, SAMSUNG_PRIVATE_KEY)';
  if (status === 403 || code === '4008' || code === '4105')
    return ' (the service account needs the Publishing scope, and SAMSUNG_CONTENT_ID has to be an app of its seller)';
  if (code === '3100' || code === '4104') return ' (SAMSUNG_CONTENT_ID: no such app)';
  if (code === '3201')
    return ' (the app has no update open: it is in review, or not yet registered in Seller Portal)';
  if (code === '5030')
    return ' (no device left for this binary: another binary covers them all; GALAXY_STORE_ABIS=arm64-v8a uploads the 64-bit one alone)';
  return '';
}

export interface Connection {
  fetch: typeof fetch;
  api: string;
  uploadUrl: string;
  serviceAccountId: string;
  token: string;
}

/** A call of the API with the access token; its JSON answer (null without one). */
export async function call(
  c: Connection,
  what: string,
  method: 'GET' | 'POST' | 'DELETE',
  url: string,
  body?: { json: unknown } | { form: FormData },
): Promise<unknown> {
  const headers: Record<string, string> = {
    Authorization: `Bearer ${c.token}`,
    'service-account-id': c.serviceAccountId,
  };
  let payload: string | FormData | undefined;
  if (body !== undefined && 'json' in body) {
    headers['content-type'] = 'application/json';
    payload = JSON.stringify(body.json);
  } else if (body !== undefined) {
    // fetch sets multipart/form-data with its boundary
    payload = body.form;
  }
  const response = await c.fetch(url, {
    method,
    headers,
    ...(payload === undefined ? {} : { body: payload }),
  });
  return parse(what, response);
}

async function parse(what: string, response: Response): Promise<unknown> {
  const raw = await response.text();
  let body: unknown = null;
  if (raw !== '') {
    try {
      body = JSON.parse(raw);
    } catch {
      body = raw;
    }
  }
  const error = responseError(response.status, body);
  if (error !== null) {
    throw new GalaxyError(
      `${what}: HTTP ${response.status}${error.code === null ? '' : `, ${error.code}`}: ${error.message}${hint(response.status, error.code)}`,
      response.status,
      error.code,
    );
  }
  return body;
}

/** The access token for the JWT (Create an Access Token). */
export async function accessToken(f: typeof fetch, api: string, token: string): Promise<string> {
  const response = await f(`${api}/auth/accessToken`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', Authorization: `Bearer ${token}` },
  });
  const body = await parse('accessToken', response);
  const access = isObject(body) && isObject(body.createdItem) ? body.createdItem.accessToken : null;
  if (typeof access !== 'string' || access === '')
    throw new GalaxyError('accessToken: no createdItem.accessToken in the answer', 200, null);
  return access;
}

/** Access tokens never expire: each run's is revoked at its end. */
export async function revoke(c: Connection): Promise<void> {
  await call(c, 'revokeAccessToken', 'DELETE', `${c.api}/auth/revokeAccessToken`);
}

// --- The app -------------------------------------------------------------------------------

export interface Binary {
  binarySeq: string;
  versionCode: number;
  versionName: string | null;
  packageName: string | null;
}

export interface Screenshot {
  screenshotKey?: string | null;
  reuseYn: boolean;
}

export interface Language {
  languagecode: string;
  appTitle?: string | null;
  description?: string | null;
  newFeature?: string | null;
  screenshots?: unknown[] | null;
}

/** What the scripts need of contentInfo's answer, the rest stays as Samsung has it. */
export interface App {
  contentId: string;
  contentStatus: string;
  defaultLanguageCode: string;
  paid: string;
  publicationType: string;
  chinaAgeLimit: string | null;
  binaries: Binary[];
  languages: Language[];
  screenshots: number;
}

/**
 * The app in contentInfo's answer (an array). With an update open it may list the version on sale
 * too: the open one then.
 */
export function app(body: unknown, contentId: string): App {
  const entries = (Array.isArray(body) ? body : [body]).filter(isObject);
  const own = entries.filter((e) => e.contentId === undefined || String(e.contentId) === contentId);
  const entry = own.find((e) => OPEN.has(String(e.contentStatus))) ?? own[0];
  if (entry === undefined) throw new GalaxyError(`contentInfo: no app ${contentId}`, 200, null);
  const binaries = (Array.isArray(entry.binaryList) ? entry.binaryList : [])
    .filter(isObject)
    .map((b) => ({
      binarySeq: String(b.binarySeq),
      versionCode: Number(b.versionCode),
      versionName: text(b.versionName),
      packageName: text(b.packageName),
    }));
  return {
    contentId,
    contentStatus: String(entry.contentStatus),
    defaultLanguageCode: text(entry.defaultLanguageCode) ?? DEFAULT_LANGUAGE,
    paid: text(entry.paid) ?? 'N',
    publicationType: text(entry.publicationType) ?? '01',
    chinaAgeLimit: text(entry.chinaAgeLimit),
    binaries,
    languages: (Array.isArray(entry.addLanguage) ? entry.addLanguage : [])
      .filter(isObject)
      .map((l) => l as unknown as Language),
    screenshots: Array.isArray(entry.screenshots) ? entry.screenshots.length : 0,
  };
}

/** Whether an update can be made or continued in this status; else why not. */
export function statusProblem(status: string): string | null {
  if (status === ON_SALE || OPEN.has(status)) return null;
  if (status === 'READY_FOR_CHANGE')
    return 'it passed review and waits to be published (manual publication): publish it in Seller Portal first';
  if (status.endsWith('_REJECTED'))
    return 'its last submission was rejected: see the reason in Seller Portal, fix it there (the app goes back to Re-Registering)';
  if (/REVIEW|DEVICE_TEST|TEST_CONFIRMATION|READY_FOR_SALE/.test(status))
    return 'it is in review: wait for the result, then run this again';
  return 'Samsung allows no update in this status';
}

/** The request of contentUpdate that opens (or keeps) an update and changes nothing else. */
export function updateRequest(a: App, newFeature?: string): Record<string, unknown> {
  return {
    contentId: a.contentId,
    defaultLanguageCode: a.defaultLanguageCode,
    paid: a.paid,
    publicationType: a.publicationType,
    ...(newFeature === undefined ? {} : { newFeature }),
    // null: as they are
    addLanguage: null,
    screenshots: null,
    sellCountryList: null,
  };
}

// --- Release: the binaries -----------------------------------------------------------------

export interface Apk {
  file: string;
  abi: string;
  versionCode: number;
}

/** The foss APKs to upload, from their names, the 64-bit ones first. */
export function apks(files: string[], version: string, code: number): Apk[] {
  const name = new RegExp(`^hashi-${version.replaceAll('.', '\\.')}-foss-(.+)\\.apk$`);
  const list = files.map((file) => {
    const abi = name.exec(path.basename(file))?.[1];
    if (abi === undefined || ABI_DIGITS[abi] === undefined)
      throw new Error(`${path.basename(file)}: not hashi-${version}-foss-<abi>.apk`);
    return { file, abi, versionCode: fossVersionCode(code, abi) };
  });
  if (list.length === 0) throw new Error('no APK');
  if (new Set(list.map((a) => a.abi)).size !== list.length) throw new Error('an ABI twice');
  // Each later binary gets the devices the ones before don't cover: 64-bit (the higher digit) first
  return list.sort((a, b) => b.versionCode - a.versionCode);
}

export interface BinaryPlan {
  /** The update's binaries of other versions (or other ABIs), deleted. */
  remove: Binary[];
  /** The update's binaries that are these APKs already (a re-run), kept. */
  keep: Binary[];
  /** The APKs to upload. */
  add: Apk[];
}

/** What to do with the update's binaries. Refuses another app's and newer versions. */
export function planBinaries(existing: Binary[], list: Apk[], code: number): BinaryPlan {
  const other = existing.find((b) => b.packageName !== null && b.packageName !== PACKAGE);
  if (other !== undefined)
    throw new Error(
      `the app's binary ${other.binarySeq} is ${other.packageName}, not ${PACKAGE}: SAMSUNG_CONTENT_ID is another app`,
    );
  const newer = existing.find((b) => Math.floor(b.versionCode / 10) > code);
  if (newer !== undefined)
    throw new Error(
      `the Galaxy Store has a newer version, ${newer.versionName ?? '?'} (versionCode ${newer.versionCode})`,
    );
  const codes = new Set(list.map((a) => a.versionCode));
  return {
    remove: existing.filter((b) => !codes.has(b.versionCode)),
    keep: existing.filter((b) => codes.has(b.versionCode)),
    add: list.filter((a) => !existing.some((b) => b.versionCode === a.versionCode)),
  };
}

/** A new upload session (Create Session ID for File Upload), valid for a day. */
export async function uploadSession(c: Connection): Promise<string> {
  const body = await call(
    c,
    'createUploadSessionId',
    'POST',
    `${c.api}/seller/createUploadSessionId`,
  );
  const id = isObject(body) ? body.sessionId : null;
  if (typeof id !== 'string' || id === '')
    throw new GalaxyError('createUploadSessionId: no sessionId in the answer', 200, null);
  return id;
}

/** Uploads a file (File Upload); its fileKey. */
export async function upload(c: Connection, session: string, file: string): Promise<string> {
  const form = new FormData();
  form.append('sessionId', session);
  form.append('file', await fs.openAsBlob(file), path.basename(file));
  const body = await call(c, `fileUpload ${path.basename(file)}`, 'POST', c.uploadUrl, { form });
  const key = isObject(body) ? body.fileKey : null;
  if (typeof key !== 'string' || key === '')
    throw new GalaxyError(`fileUpload ${path.basename(file)}: no fileKey in the answer`, 200, null);
  return key;
}

/** The request of Add New Binary: the uploaded APK, without Google services (the foss build). */
export function addBinaryRequest(contentId: string, fileKey: string): Record<string, string> {
  return { contentId, gms: 'N', filekey: fileKey };
}

export type Log = (line: string) => void;

export interface ReleaseOptions {
  contentId: string;
  version: string;
  code: number;
  apks: Apk[];
  newFeature?: string;
  submit: boolean;
}

/** The release: the APKs as the binaries of the update, submitted with `submit`. Its summary. */
export async function release(c: Connection, o: ReleaseOptions, log: Log): Promise<string[]> {
  const info = `${c.api}/seller/contentInfo?contentId=${encodeURIComponent(o.contentId)}`;
  let a = app(await call(c, 'contentInfo', 'GET', info), o.contentId);
  log(
    `The app ${o.contentId}: ${a.contentStatus}, binaries ${a.binaries.map((b) => b.versionCode).join(', ') || 'none'}`,
  );
  const problem = statusProblem(a.contentStatus);
  if (problem !== null) throw new Error(`The app is ${a.contentStatus}: ${problem}`);
  // Checked before anything changes; again on the update below
  planBinaries(a.binaries, o.apks, o.code);

  const updated = await call(c, 'contentUpdate', 'POST', `${c.api}/seller/contentUpdate`, {
    json: updateRequest(a, o.newFeature),
  });
  // The update's binaries: a copy of those on sale at first
  a = app(await call(c, 'contentInfo', 'GET', info), o.contentId);
  const status = (isObject(updated) ? text(updated.contentStatus) : null) ?? a.contentStatus;
  if (!OPEN.has(status)) throw new Error(`contentUpdate left the app ${status}, no update is open`);
  const plan = planBinaries(a.binaries, o.apks, o.code);
  log(
    `The update (${status}): delete ${plan.remove.map((b) => b.versionCode).join(', ') || 'nothing'}, keep ${plan.keep.map((b) => b.versionCode).join(', ') || 'nothing'}, add ${plan.add.map((x) => x.versionCode).join(', ') || 'nothing'}`,
  );

  // Uploads first: nothing of the update changes until every APK is there
  const keys = new Map<Apk, string>();
  if (plan.add.length > 0) {
    const session = await uploadSession(c);
    for (const apk of plan.add) {
      keys.set(apk, await upload(c, session, apk.file));
      log(`Uploaded ${path.basename(apk.file)}`);
    }
  }
  // The old binaries out before the new ones come in, which then get their devices
  for (const b of plan.remove) {
    const query = `contentId=${encodeURIComponent(o.contentId)}&binarySeq=${encodeURIComponent(b.binarySeq)}`;
    await call(
      c,
      `delete binary ${b.binarySeq}`,
      'DELETE',
      `${c.api}/seller/v2/content/binary?${query}`,
      {
        json: { contentId: o.contentId, binarySeq: b.binarySeq },
      },
    );
    log(`Deleted the binary ${b.binarySeq} (versionCode ${b.versionCode})`);
  }
  for (const apk of plan.add) {
    const body = await call(
      c,
      `add binary ${path.basename(apk.file)}`,
      'POST',
      `${c.api}/seller/v2/content/binary`,
      {
        json: addBinaryRequest(o.contentId, keys.get(apk) ?? ''),
      },
    );
    const seq = isObject(body) && isObject(body.data) ? text(body.data.binarySeq) : null;
    log(
      `Added ${path.basename(apk.file)} (versionCode ${apk.versionCode}) as the binary ${seq ?? '?'}`,
    );
  }

  const summary = [
    `Galaxy Store: ${o.version} in the update of the app ${o.contentId}, the binaries ${o.apks.map((x) => `${x.abi} (${x.versionCode})`).join(', ')}${plan.remove.length > 0 ? `; deleted ${plan.remove.map((b) => b.versionCode).join(', ')}` : ''}`,
  ];
  if (o.submit) {
    await call(c, 'contentSubmit', 'POST', `${c.api}/seller/contentSubmit`, {
      json: { contentId: o.contentId },
    });
    log('Submitted for review');
    summary.push(
      'Submitted for review. It goes live after it, unless the app is set to manual publication in Seller Portal.',
    );
  } else {
    summary.push('Not submitted (GALAXY_STORE_SUBMIT is false): submit it in Seller Portal.');
  }
  return summary;
}

// --- Listing -------------------------------------------------------------------------------

export interface ListingText {
  code: string;
  play: string;
  appTitle: string;
  description: string;
}

/** The texts of each Galaxy language, from Google Play's listing. */
export function listingTexts(metadata = path.join(ROOT, PLAY_METADATA)): ListingText[] {
  const read = (locale: string, file: string) =>
    fs.readFileSync(path.join(metadata, locale, file), 'utf8').trim();
  return GALAXY.map(({ code, play }) => ({
    code,
    play,
    appTitle: read(play, 'title.txt'),
    description: read(play, 'full_description.txt'),
  }));
}

/**
 * The other stores a text names. Samsung turns away listings that point to other stores, and the
 * app from the Galaxy Store has no Google Play Games.
 */
export function otherStores(text: string): string[] {
  const found = new Map<string, string>();
  for (const [m] of text.matchAll(
    /\b(?:Google Play(?: Games)?|Play Store|App Store|Apple|Game Center|TestFlight)\b/gi,
  ))
    if (!found.has(m.toLowerCase())) found.set(m.toLowerCase(), m);
  return [...found.values()];
}

/**
 * The release notes as the update's "what's new": none when they name another store (Samsung's
 * stay then), refused over Samsung's limit.
 */
export function releaseNotes(text: string): { newFeature?: string; note?: string } {
  if (bytes(text) > LIMITS.newFeature)
    throw new Error(
      `the release notes have ${bytes(text)} bytes, Samsung takes ${LIMITS.newFeature}`,
    );
  const stores = otherStores(text);
  if (stores.length > 0)
    return {
      note: `The release notes name ${stores.join(', ')}: not sent to the Galaxy Store, its "what's new" stays as it was (Seller Portal)`,
    };
  return text === '' ? {} : { newFeature: text };
}

/** A text's problems with Samsung's limits and rules. */
export function textProblems(t: ListingText): string[] {
  const problems: string[] = [];
  const stores = otherStores(`${t.appTitle}\n${t.description}`);
  if (stores.length > 0) problems.push(`${t.code} (${t.play}): names ${stores.join(', ')}`);
  if (t.appTitle === '') problems.push(`${t.code} (${t.play}): no title`);
  if (bytes(t.appTitle) > LIMITS.appTitle)
    problems.push(
      `${t.code} (${t.play}): the title has ${bytes(t.appTitle)} bytes, Samsung takes ${LIMITS.appTitle}`,
    );
  if (t.description === '') problems.push(`${t.code} (${t.play}): no description`);
  if (bytes(t.description) > LIMITS.description)
    problems.push(
      `${t.code} (${t.play}): the description has ${bytes(t.description)} bytes, Samsung takes ${LIMITS.description}`,
    );
  return problems;
}

/** The phone screenshots of each Galaxy language in an images folder, by Samsung's code. */
export function listingScreenshots(images: string): Map<string, string[]> {
  const shots = new Map<string, string[]>();
  for (const { code, play } of GALAXY) {
    const site = PLAY.find((p) => p.locale === play)?.site;
    const dir = path.join(images, site ?? play, 'play-phone');
    const files = fs.existsSync(dir)
      ? fs
          .readdirSync(dir)
          .filter((f) => f.endsWith('.png'))
          .sort((a, b) => Number.parseInt(a, 10) - Number.parseInt(b, 10))
          .map((f) => path.join(dir, f))
      : [];
    shots.set(code, files);
  }
  return shots;
}

/** An image's problems with Samsung's rules for screenshots. */
export function screenshotProblems(file: string, width: number, height: number): string[] {
  const s = SCREENSHOTS;
  const name = path.relative(ROOT, file);
  if (Math.min(width, height) < s.minSide || Math.max(width, height) > s.maxSide)
    return [`${name} is ${width}×${height}, each side has to be ${s.minSide} to ${s.maxSide} px`];
  if (Math.max(width, height) > s.ratio * Math.min(width, height))
    return [`${name} is ${width}×${height}, longer than ${s.ratio}:1`];
  return [];
}

export interface ListingPlan {
  /** By Samsung's code: the texts to send. */
  texts: ListingText[];
  /** Languages left as Samsung has them, and why. */
  skipped: string[];
}

/**
 * Which languages get their texts: those within Samsung's limits. One over a limit keeps what
 * Samsung has, a Chinese one waits for China's age rating, which Samsung requires with them.
 */
export function planListing(texts: ListingText[], a: App): ListingPlan {
  const plan: ListingPlan = { texts: [], skipped: [] };
  for (const t of texts) {
    const problems = textProblems(t);
    if (t.code === DEFAULT_LANGUAGE && problems.length > 0)
      throw new Error(`the default language's texts: ${problems.join('; ')}`);
    if (problems.length > 0) plan.skipped.push(`${problems.join('; ')}: kept as it is`);
    else if (CHINESE.has(t.code) && a.chinaAgeLimit === null)
      plan.skipped.push(
        `${t.code} (${t.play}): needs China's age rating, set it in Seller Portal (App Information)`,
      );
    else plan.texts.push(t);
  }
  return plan;
}

/**
 * The request of contentUpdate for the listing: the default language's texts on top, the others
 * in addLanguage (every language Samsung has, else it drops it: those this doesn't change as they
 * are), with the uploaded screenshots (`shots`, by code) or the ones Samsung has.
 */
export function listingRequest(
  a: App,
  plan: ListingPlan,
  images: { iconKey: string; shots: Map<string, string[]> } | null,
): Record<string, unknown> {
  const screenshots = (code: string, existing: unknown[] | null | undefined) => {
    const keys = images?.shots.get(code);
    if (keys !== undefined && keys.length > 0)
      return keys.map((key): Screenshot => ({ screenshotKey: key, reuseYn: false }));
    return existing === null || existing === undefined
      ? null
      : existing.map((): Screenshot => ({ reuseYn: true }));
  };
  const own = (code: string) => plan.texts.find((t) => t.code === code);
  const top = own(DEFAULT_LANGUAGE);
  const languages: Language[] = [];
  const codes = [
    ...new Set([
      ...plan.texts.map((t) => t.code).filter((c) => c !== DEFAULT_LANGUAGE),
      ...a.languages.map((l) => l.languagecode).filter((c) => c !== DEFAULT_LANGUAGE),
    ]),
  ];
  for (const code of codes) {
    const existing = a.languages.find((l) => l.languagecode === code);
    const t = own(code);
    if (t === undefined && existing === undefined) continue;
    const shots = screenshots(code, existing?.screenshots);
    languages.push({
      languagecode: code,
      appTitle: t?.appTitle ?? existing?.appTitle ?? null,
      description: t?.description ?? existing?.description ?? null,
      newFeature: existing?.newFeature ?? null,
      ...(shots === null ? {} : { screenshots: shots }),
    });
  }
  return {
    contentId: a.contentId,
    defaultLanguageCode: DEFAULT_LANGUAGE,
    paid: a.paid,
    publicationType: a.publicationType,
    ...(top === undefined ? {} : { appTitle: top.appTitle, longDescription: top.description }),
    ...(images === null ? {} : { iconKey: images.iconKey }),
    screenshots: images?.shots.get(DEFAULT_LANGUAGE)?.length
      ? screenshots(DEFAULT_LANGUAGE, null)
      : null,
    addLanguage: languages,
    ...(a.chinaAgeLimit === null ? {} : { chinaAgeLimit: a.chinaAgeLimit }),
    sellCountryList: null,
  };
}

/** The listing's images: the icon and the screenshots of each language, checked. */
export function listingImages(images: string): { icon: string; shots: Map<string, string[]> } {
  const problems: string[] = [];
  const icon = pngInfo(ICON);
  if (icon.width !== 512 || icon.height !== 512)
    problems.push(`${path.relative(ROOT, ICON)} is ${icon.width}×${icon.height}, not 512×512`);
  if (fs.statSync(ICON).size > ICON_BYTES)
    problems.push(`${path.relative(ROOT, ICON)} is over 1 MB`);
  const shots = listingScreenshots(images);
  for (const [code, files] of shots) {
    if (files.length < SCREENSHOTS.min || files.length > SCREENSHOTS.max)
      problems.push(
        `${code}: ${files.length} screenshots, Samsung takes ${SCREENSHOTS.min} to ${SCREENSHOTS.max}`,
      );
    for (const f of files) {
      const { width, height } = pngInfo(f);
      problems.push(...screenshotProblems(f, width, height));
    }
  }
  if (problems.length > 0) throw new Error(`the images have problems:\n${problems.join('\n')}`);
  return { icon: ICON, shots };
}

/** The listing into the update: texts, and the images when there are. Its summary. */
export async function listing(
  c: Connection,
  contentId: string,
  texts: ListingText[],
  images: { icon: string; shots: Map<string, string[]> } | null,
  log: Log,
): Promise<string[]> {
  const info = `${c.api}/seller/contentInfo?contentId=${encodeURIComponent(contentId)}`;
  const a = app(await call(c, 'contentInfo', 'GET', info), contentId);
  log(`The app ${contentId}: ${a.contentStatus}`);
  const problem = statusProblem(a.contentStatus);
  if (problem !== null) throw new Error(`The app is ${a.contentStatus}: ${problem}`);
  const plan = planListing(texts, a);
  let uploaded: { iconKey: string; shots: Map<string, string[]> } | null = null;
  if (images !== null) {
    const session = await uploadSession(c);
    const iconKey = await upload(c, session, images.icon);
    const shots = new Map<string, string[]>();
    for (const t of plan.texts) {
      const keys: string[] = [];
      for (const f of images.shots.get(t.code) ?? []) keys.push(await upload(c, session, f));
      shots.set(t.code, keys);
      log(`Uploaded ${keys.length} screenshots of ${t.code}`);
    }
    uploaded = { iconKey, shots };
  }
  await call(c, 'contentUpdate', 'POST', `${c.api}/seller/contentUpdate`, {
    json: listingRequest(a, plan, uploaded),
  });
  return [
    `Galaxy Store: the listing of the app ${contentId} in ${plan.texts.length} languages (${plan.texts.map((t) => t.code).join(', ')})${images === null ? ', texts only' : ', with the icon and screenshots'}, in its open update (not submitted: the next release submits it, or Seller Portal)`,
    ...plan.skipped.map((s) => `Not changed: ${s}`),
  ];
}

// --- Command line --------------------------------------------------------------------------

const USAGE = `usage: node scripts/galaxy-store.ts release --version <x.y.z> --code <n> [--whatsnew <file>] [--submit] [--dry-run] <apk>...
       node scripts/galaxy-store.ts listing [--images <dir>] [--dry-run]`;

function usage(): never {
  console.error(USAGE);
  process.exit(2);
}

/** The connection with a fresh access token, from the environment. */
async function connect(env: NodeJS.ProcessEnv): Promise<Connection> {
  const serviceAccountId = env.SAMSUNG_SERVICE_ACCOUNT_ID ?? '';
  const key = privateKey(env.SAMSUNG_PRIVATE_KEY ?? '');
  const api = env.GALAXY_STORE_API ?? API;
  const token = await accessToken(fetch, api, jwt(serviceAccountId, key));
  return {
    fetch,
    api,
    uploadUrl: env.GALAXY_STORE_UPLOAD_URL ?? UPLOAD_URL,
    serviceAccountId,
    token,
  };
}

function environment(env: NodeJS.ProcessEnv, dryRun: boolean): string {
  const missing = [
    'SAMSUNG_SERVICE_ACCOUNT_ID',
    'SAMSUNG_PRIVATE_KEY',
    'SAMSUNG_CONTENT_ID',
  ].filter((name) => (env[name] ?? '') === '');
  if (missing.length > 0 && !dryRun) throw new Error(`${missing.join(', ')} not set`);
  // A dry run checks the key when there is one
  if (!dryRun || (env.SAMSUNG_PRIVATE_KEY ?? '') !== '')
    jwt(env.SAMSUNG_SERVICE_ACCOUNT_ID ?? '', privateKey(env.SAMSUNG_PRIVATE_KEY ?? ''));
  const contentId = env.SAMSUNG_CONTENT_ID ?? '';
  if (contentId !== '' && !/^\d{12}$/.test(contentId))
    throw new Error("SAMSUNG_CONTENT_ID isn't the app's 12-digit content ID");
  return contentId;
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
    const submit = flag('--submit');
    const version = option('--version');
    const code = Number(option('--code'));
    const whatsnew = option('--whatsnew');
    if (version === undefined || !Number.isInteger(code) || code <= 0 || args.length === 0) usage();
    const list = apks(args, version, code);
    for (const apk of list) fs.accessSync(apk.file);
    const { newFeature, note } =
      whatsnew === undefined ? {} : releaseNotes(fs.readFileSync(whatsnew, 'utf8').trim());
    const notes = note === undefined ? [] : [note];
    const contentId = environment(env, dryRun);
    if (dryRun) {
      return [
        ...notes,
        `Galaxy Store, dry run: ${version} would go to the app's update as ${list.map((a) => `${path.basename(a.file)} (versionCode ${a.versionCode}, ${(fs.statSync(a.file).size / 1048576).toFixed(1)} MB)`).join(', ')}, without Google services (gms N)${submit ? ', then submitted for review' : ', not submitted'}`,
      ];
    }
    const c = await connect(env);
    try {
      return notes.concat(
        await release(
          c,
          {
            contentId,
            version,
            code,
            apks: list,
            submit,
            ...(newFeature === undefined ? {} : { newFeature }),
          },
          log,
        ),
      );
    } finally {
      await revoke(c).catch((e: unknown) =>
        console.log(`::warning::Revoking the access token: ${e instanceof Error ? e.message : e}`),
      );
    }
  }

  if (command === 'listing') {
    const imagesDir = option('--images');
    if (args.length > 0) usage();
    const texts = listingTexts();
    const problems = texts.flatMap(textProblems);
    const images = imagesDir === undefined ? null : listingImages(imagesDir);
    const contentId = environment(env, dryRun);
    if (dryRun) {
      return [
        `Galaxy Store, dry run: the listing in ${texts.length} languages${images === null ? ', texts only' : `, with the icon and ${[...images.shots.values()].reduce((n, s) => n + s.length, 0)} screenshots`}`,
        ...problems.map((p) => `Would keep Samsung's text: ${p}`),
      ];
    }
    const c = await connect(env);
    try {
      return await listing(c, contentId, texts, images, log);
    } finally {
      await revoke(c).catch((e: unknown) =>
        console.log(`::warning::Revoking the access token: ${e instanceof Error ? e.message : e}`),
      );
    }
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
