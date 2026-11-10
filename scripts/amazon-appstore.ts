// Uploads the app to the Amazon Appstore (docs/stores.md, Amazon Appstore) through Amazon's App
// Submission API (https://developer.amazon.com/docs/app-submission-api/overview.html, the
// reference https://amzndevresources.com/appsubapi/swagger-en/index.html):
//
//   node scripts/amazon-appstore.ts release --version <x.y.z> --code <n> [--whatsnew <file>]
//                                           [--submit] [--dry-run] <apk>...
//   node scripts/amazon-appstore.ts listing [--images <dir>] [--dry-run]
//
// release: the foss APKs `hashi-<version>-foss-<abi>.apk` replace the binaries of the app's edit
// (the next version: it opens one, or takes the one a run before or the Developer Console left
// open), each the binary of its ABI, which keeps the devices it targets; the release notes go
// into every language's "recent changes"; with --submit the edit is validated and committed,
// which sends it to Amazon's review. It only replaces binaries: which ABIs the app has (and their
// devices) is set in the Developer Console, an APK without a binary of its ABI stays out. A re-run
// skips the binaries that are this version already. It refuses an edit in review, and binaries of
// a newer version.
//
// listing: the texts of Google Play's listing (fastlane/metadata/android: title, the description,
// its first paragraph as the short description and the first point of each of its lists as the
// feature bullets) in Amazon's languages (AMAZON below), with --images the icons, the promotional
// image and each language's tablet screenshots from a folder like target/store/images (the "Store
// listing" workflow's artifact). Into the open edit, or a new one; never committed (the next
// release commits it).
//
// --dry-run: the local checks and the plan, no request to Amazon.
//
// The security profile (Developer Console, Settings, API Access, App Submission API) in the
// environment: AMAZON_CLIENT_ID, AMAZON_CLIENT_SECRET, and AMAZON_APP_ID (the app's ID,
// amzn1.devportal.mobileapp.<...>). AMAZON_APPSTORE_API and AMAZON_TOKEN_URL point it at another
// server (the tests' mock).
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { pngInfo } from '../store/assemble.ts';
import { PLAY, PLAY_METADATA } from '../store/locales.ts';
import { ABI_DIGITS } from './foss-apk.ts';
import { type Apk, apks, otherStores } from './galaxy-store.ts';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
export const API = 'https://developer.amazon.com/api/appstore/v1';
export const TOKEN_URL = 'https://api.amazon.com/auth/o2/token';
export const SCOPE = 'appstore::apps:readwrite';
const APK_TYPE = 'application/vnd.android.package-archive';
const LARGE_ICON = path.join(ROOT, 'public/icon-512.png');
/** The 16×16 pixels of the icon at 7 px each, on a pixel of its background (docs/stores.md). */
const SMALL_ICON = path.join(ROOT, 'app/store/amazon/small-icon.png');

/**
 * The listing's languages: Amazon's locale and the Google Play locale whose texts and
 * screenshots it gets. en-US is the default, and what every other language falls back to
 * (English of Britain, Australia and India too). Amazon has one Spanish, for Spain and Mexico,
 * and no Chinese, Korean, Russian, ... store listing.
 */
export const AMAZON: readonly { code: string; play: string }[] = [
  { code: 'en-US', play: 'en-US' },
  { code: 'de-DE', play: 'de-DE' },
  { code: 'fr-FR', play: 'fr-FR' },
  { code: 'it-IT', play: 'it-IT' },
  { code: 'es-ES', play: 'es-ES' },
  { code: 'pt-BR', play: 'pt-BR' },
  { code: 'pt-PT', play: 'pt-PT' },
  { code: 'nl-NL', play: 'nl-NL' },
  { code: 'pl-PL', play: 'pl-PL' },
  { code: 'ja-JP', play: 'ja-JP' },
];
export const DEFAULT_LANGUAGE = 'en-US';

/**
 * Amazon's limits of the texts (Appstore details): the short description 2000 bytes (Amazon
 * counts about 1200 English characters), the long one 4000 characters, 3 to 5 feature bullets.
 */
export const LIMITS = {
  title: 250,
  shortBytes: 2000,
  shortChars: 1200,
  fullDescription: 4000,
  recentChanges: 4000,
  bullets: { min: 3, max: 5 },
} as const;
/** Screenshots: 3 to 10, each of these sizes, upright or sideways. */
export const SCREENSHOTS = {
  min: 3,
  max: 10,
  sizes: [
    [800, 480],
    [1024, 600],
    [1280, 720],
    [1280, 800],
    [1920, 1080],
    [1920, 1200],
    [2560, 1600],
  ],
} as const;
/** The frame of the store images whose screenshots go to Amazon: Play's 7" tablet, 1080×1920. */
export const SCREENSHOT_FRAME = 'play-tablet-7';
/** The images of a listing: Amazon's image type, and the size of the single ones. */
export const IMAGE_TYPES = {
  small: { type: 'small-icons', width: 114, height: 114 },
  large: { type: 'large-icons', width: 512, height: 512 },
  promo: { type: 'promo-images', width: 1024, height: 500 },
  screenshots: { type: 'screenshots' },
} as const;

/** The status of an open edit, whose binaries and texts can be changed. */
export const IN_PROGRESS = 'IN_PROGRESS';

const bytes = (text: string) => Buffer.byteLength(text, 'utf8');
const chars = (text: string) => [...text].length;

// --- Requests and responses ----------------------------------------------------------------

export class AmazonError extends Error {
  readonly status: number;
  constructor(message: string, status: number) {
    super(message);
    this.status = status;
  }
}

const isObject = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown): string | null =>
  value === null || value === undefined || value === '' ? null : String(value);

/**
 * The message of an error response. Amazon's come as `{message}`, `{errorMessage}`, OAuth's
 * `{error, error_description}`, the validation's as a list of errors; else the body itself.
 */
export function errorMessage(status: number, body: unknown): string {
  const messages = (value: unknown): string[] => {
    if (typeof value === 'string') return value === '' ? [] : [value];
    if (Array.isArray(value)) return value.flatMap(messages);
    if (!isObject(value)) return [];
    const own = [value.message, value.errorMessage, value.error_description]
      .map(text)
      .filter((m): m is string => m !== null);
    if (own.length === 0 && typeof value.error === 'string') own.push(value.error);
    return [...own, ...messages(value.errors), ...messages(value.validationErrors)];
  };
  const found = messages(body);
  if (found.length > 0) return [...new Set(found)].join('; ').slice(0, 2000);
  if (isObject(body) || Array.isArray(body)) return JSON.stringify(body).slice(0, 1000);
  return `HTTP ${status}`;
}

/** What to do about an error, for the ones that need the owner. */
function hint(status: number, what: string): string {
  if (what === 'accessToken')
    return ' (AMAZON_CLIENT_ID and AMAZON_CLIENT_SECRET: the client ID and secret of the security profile)';
  if ((what === 'validateEdit' || what === 'commitEdit') && status === 403)
    return ' (Amazon refuses the edit: fix it in the Developer Console, Your apps, the upcoming version, then run this again)';
  if (status === 401 || status === 403)
    return ' (the security profile has to be attached to the App Submission API, Settings, API Access; AMAZON_APP_ID an app of the same account)';
  if (status === 404) return ' (AMAZON_APP_ID: no such app, or not yet submitted once)';
  if (status === 412)
    return ' (the app is in review, or its edit changed meanwhile: wait for the review, then run this again)';
  return '';
}

export interface Connection {
  fetch: typeof fetch;
  api: string;
  appId: string;
  token: string;
}

export interface Answer {
  status: number;
  body: unknown;
  /** The ETag of a GET's resource, which a change of it sends back as If-Match. */
  etag: string | null;
}

export interface Request {
  json?: unknown;
  /** A file as the body. */
  file?: { path: string; type: string; name?: string };
  ifMatch?: string | null;
  /** 404 is an answer, not an error (a resource that isn't there). */
  allow404?: boolean;
}

/** A call of the API (a path below the app) with the access token. */
export async function call(
  c: Connection,
  what: string,
  method: 'GET' | 'POST' | 'PUT' | 'DELETE',
  sub: string,
  r: Request = {},
): Promise<Answer> {
  const headers: Record<string, string> = {
    Authorization: `Bearer ${c.token}`,
    Accept: 'application/json',
  };
  let body: string | Blob | undefined;
  if (r.json !== undefined) {
    headers['Content-Type'] = 'application/json';
    body = JSON.stringify(r.json);
  } else if (r.file !== undefined) {
    headers['Content-Type'] = r.file.type;
    if (r.file.name !== undefined) headers.fileName = r.file.name;
    body = await fs.openAsBlob(r.file.path, { type: r.file.type });
  }
  if (r.ifMatch !== undefined && r.ifMatch !== null) headers['If-Match'] = r.ifMatch;
  const url = `${c.api}/applications/${encodeURIComponent(c.appId)}${sub}`;
  const response = await c.fetch(url, {
    method,
    headers,
    ...(body === undefined ? {} : { body }),
  });
  const answer = await parse(response);
  if (response.status === 404 && r.allow404 === true) return answer;
  if (!response.ok) {
    throw new AmazonError(
      `${what}: HTTP ${response.status}: ${errorMessage(response.status, answer.body)}${hint(response.status, what)}`,
      response.status,
    );
  }
  return answer;
}

async function parse(response: Response): Promise<Answer> {
  const raw = await response.text();
  let body: unknown = null;
  if (raw !== '') {
    try {
      body = JSON.parse(raw);
    } catch {
      body = raw;
    }
  }
  return { status: response.status, body, etag: response.headers.get('etag') };
}

/** An access token for the security profile (OAuth client credentials, valid for an hour). */
export async function accessToken(
  f: typeof fetch,
  tokenUrl: string,
  clientId: string,
  clientSecret: string,
): Promise<string> {
  const response = await f(tokenUrl, {
    method: 'POST',
    headers: { 'Content-Type': 'application/x-www-form-urlencoded', Accept: 'application/json' },
    body: new URLSearchParams({
      grant_type: 'client_credentials',
      client_id: clientId,
      client_secret: clientSecret,
      scope: SCOPE,
    }).toString(),
  });
  const { body } = await parse(response);
  if (!response.ok)
    throw new AmazonError(
      `accessToken: HTTP ${response.status}: ${errorMessage(response.status, body)}${hint(response.status, 'accessToken')}`,
      response.status,
    );
  const token = isObject(body) ? body.access_token : null;
  if (typeof token !== 'string' || token === '')
    throw new AmazonError('accessToken: no access_token in the answer', response.status);
  return token;
}

// --- The edit ------------------------------------------------------------------------------

export interface Edit {
  id: string;
  status: string;
}

/** The edit in an answer, none for an empty one (getActiveEdit without an open edit). */
export function editOf(body: unknown): Edit | null {
  if (!isObject(body)) return null;
  const id = text(body.id);
  return id === null ? null : { id, status: text(body.status) ?? IN_PROGRESS };
}

/** The app's open edit, or a new one (with the live version's binaries and texts). */
export async function openEdit(c: Connection, log: Log): Promise<Edit> {
  const active = editOf((await call(c, 'getActiveEdit', 'GET', '/edits', { allow404: true })).body);
  if (active !== null) {
    if (active.status !== IN_PROGRESS)
      throw new Error(
        `The app's edit ${active.id} is ${active.status}, not open: it is in review or being published. Wait for Amazon, then run this again`,
      );
    log(`The open edit ${active.id}`);
    return active;
  }
  const created = editOf((await call(c, 'createEdit', 'POST', '/edits')).body);
  if (created === null) throw new AmazonError('createEdit: no edit in the answer', 200);
  log(`A new edit ${created.id}`);
  return created;
}

// --- Release: the binaries -----------------------------------------------------------------

/** A binary of the edit. */
export interface Binary {
  id: string;
  versionCode: number;
  name: string | null;
}

/** The binaries in listApks's answer. */
export function binaries(body: unknown): Binary[] {
  return (Array.isArray(body) ? body : []).filter(isObject).map((b) => ({
    id: String(b.id),
    versionCode: Number(b.versionCode),
    name: text(b.name),
  }));
}

const ABI_OF_DIGIT = new Map(Object.entries(ABI_DIGITS).map(([abi, digit]) => [digit, abi]));

export interface BinaryPlan {
  /** The edit's binaries and the APK of their ABI that replaces them. */
  replace: { binary: Binary; apk: Apk }[];
  /** Binaries that are this version's APK already (a re-run). */
  current: { binary: Binary; apk: Apk }[];
  /** APKs without a binary of their ABI: not uploaded. */
  unused: Apk[];
}

/**
 * What to do with the edit's binaries: each, a foss APK (versionCode = version × 10 + the ABI's
 * digit), gets this version's APK of its ABI. Refuses binaries of a newer version, of no foss
 * ABI, two of one ABI, and one whose APK is missing.
 */
export function planBinaries(existing: Binary[], list: Apk[], code: number): BinaryPlan {
  if (existing.length === 0)
    throw new Error(
      'the edit has no binary: the first version is submitted in the Developer Console (docs/stores.md)',
    );
  const plan: BinaryPlan = { replace: [], current: [], unused: [] };
  const seen = new Set<string>();
  for (const b of existing) {
    const label = `the binary ${b.name ?? b.id} (versionCode ${b.versionCode})`;
    if (Math.floor(b.versionCode / 10) > code)
      throw new Error(`the Amazon Appstore has a newer version: ${label}`);
    const abi = ABI_OF_DIGIT.get(b.versionCode % 10);
    if (abi === undefined || !Number.isInteger(b.versionCode))
      throw new Error(
        `${label} isn't a foss APK (hashi-<version>-foss-<abi>.apk): replace it in the Developer Console`,
      );
    if (seen.has(abi))
      throw new Error(`two binaries of ${abi}: delete one in the Developer Console`);
    seen.add(abi);
    const apk = list.find((a) => a.abi === abi);
    if (apk === undefined)
      throw new Error(
        `the app has a binary of ${abi} (${label}), and no ${abi} APK was given to replace it`,
      );
    (b.versionCode === apk.versionCode ? plan.current : plan.replace).push({ binary: b, apk });
  }
  plan.unused = list.filter((a) => !seen.has(a.abi));
  return plan;
}

/** The text of every language's "recent changes": the release notes, if Amazon may show them. */
export function releaseNotes(notes: string, version: string): { text: string; note?: string } {
  const stores = otherStores(notes);
  if (stores.length > 0)
    return {
      text: `Hashi ${version}`,
      note: `The release notes name ${stores.join(', ')}: the Amazon Appstore's recent changes are "Hashi ${version}"`,
    };
  if (chars(notes) > LIMITS.recentChanges)
    throw new Error(
      `the release notes have ${chars(notes)} characters, Amazon takes ${LIMITS.recentChanges}`,
    );
  return { text: notes === '' ? `Hashi ${version}` : notes };
}

export type Log = (line: string) => void;

export interface ReleaseOptions {
  version: string;
  code: number;
  apks: Apk[];
  recentChanges: string;
  submit: boolean;
}

/** Puts `recentChanges` into each listing of the edit. */
async function setRecentChanges(c: Connection, editId: string, recentChanges: string, log: Log) {
  const all = (await call(c, 'getAllListings', 'GET', `/edits/${editId}/listings`)).body;
  const languages = isObject(all) && isObject(all.listings) ? Object.keys(all.listings) : [];
  for (const language of languages) {
    const sub = `/edits/${editId}/listings/${encodeURIComponent(language)}`;
    const current = await call(c, `getListing ${language}`, 'GET', sub);
    const listing = isObject(current.body) ? current.body : {};
    if (listing.recentChanges === recentChanges) continue;
    await call(c, `updateListing ${language}`, 'PUT', sub, {
      json: { ...listing, recentChanges },
      ifMatch: current.etag,
    });
  }
  log(`Recent changes in ${languages.length} listing${languages.length === 1 ? '' : 's'}`);
}

/** Validates the edit and commits it, which submits it for review. */
async function commit(c: Connection, editId: string, log: Log) {
  await call(c, 'validateEdit', 'POST', `/edits/${editId}/validate`);
  const edit = await call(c, 'getEdit', 'GET', `/edits/${editId}`);
  await call(c, 'commitEdit', 'POST', `/edits/${editId}/commit`, { ifMatch: edit.etag });
  log('Committed: submitted for review');
}

/** The release: the APKs as the edit's binaries, committed with `submit`. Its summary. */
export async function release(c: Connection, o: ReleaseOptions, log: Log): Promise<string[]> {
  const edit = await openEdit(c, log);
  const apksOf = async () =>
    binaries((await call(c, 'listApks', 'GET', `/edits/${edit.id}/apks`)).body);
  const existing = await apksOf();
  log(
    `Binaries: ${existing.map((b) => `${b.name ?? b.id} (${b.versionCode})`).join(', ') || 'none'}`,
  );
  const plan = planBinaries(existing, o.apks, o.code);

  for (const { binary, apk } of plan.replace) {
    const sub = `/edits/${edit.id}/apks/${encodeURIComponent(binary.id)}`;
    const current = await call(c, `getApk ${binary.id}`, 'GET', sub);
    await call(c, `replaceApk ${path.basename(apk.file)}`, 'PUT', `${sub}/replace`, {
      file: { path: apk.file, type: APK_TYPE, name: path.basename(apk.file) },
      ifMatch: current.etag,
    });
    log(
      `Replaced the binary ${binary.id} (versionCode ${binary.versionCode}) with ${path.basename(apk.file)} (${apk.versionCode})`,
    );
  }
  // What Amazon has now: every binary this version's
  const after = await apksOf();
  const stale = after.filter((b) => !o.apks.some((a) => a.versionCode === b.versionCode));
  if (stale.length > 0 || after.length !== existing.length)
    throw new Error(
      `after the upload the edit has the binaries ${after.map((b) => b.versionCode).join(', ') || 'none'}, not those of ${o.version}`,
    );
  await setRecentChanges(c, edit.id, o.recentChanges, log);

  const summary = [
    `Amazon Appstore: ${o.version} in the edit ${edit.id}, the binaries ${[...plan.replace, ...plan.current].map(({ apk }) => `${apk.abi} (${apk.versionCode})`).join(', ')}${plan.current.length > 0 ? ` (${plan.current.map(({ apk }) => apk.abi).join(', ')} already there)` : ''}`,
  ];
  if (plan.unused.length > 0)
    summary.push(
      `Not uploaded, the app has no binary of their ABI: ${plan.unused.map((a) => a.abi).join(', ')}`,
    );
  if (o.submit) {
    await commit(c, edit.id, log);
    summary.push('Submitted for review. It goes live after it.');
  } else {
    summary.push(
      'Not submitted (AMAZON_APPSTORE_SUBMIT is false): submit it in the Developer Console.',
    );
  }
  return summary;
}

// --- Listing -------------------------------------------------------------------------------

export interface ListingText {
  code: string;
  play: string;
  title: string;
  shortDescription: string;
  fullDescription: string;
  featureBullets: string[];
}

/** The description's first paragraph: the short description. */
export function firstParagraph(description: string): string {
  return (
    description
      .split(/\n\s*\n/)[0]
      ?.replace(/\s*\n\s*/g, ' ')
      .trim() ?? ''
  );
}

/** The first point of each list of the description ("• ..."): the feature bullets. */
export function featureBullets(description: string): string[] {
  const bullets: string[] = [];
  for (const block of description.split(/\n\s*\n/)) {
    const first = block
      .split('\n')
      .map((line) => line.trim())
      .find((line) => line.startsWith('•'));
    if (first !== undefined) bullets.push(first.replace(/^•\s*/, ''));
  }
  return bullets;
}

/** The texts of each Amazon language, from Google Play's listing. */
export function listingTexts(metadata = path.join(ROOT, PLAY_METADATA)): ListingText[] {
  const read = (locale: string, file: string) =>
    fs.readFileSync(path.join(metadata, locale, file), 'utf8').trim();
  return AMAZON.map(({ code, play }) => {
    const description = read(play, 'full_description.txt');
    return {
      code,
      play,
      title: read(play, 'title.txt'),
      shortDescription: firstParagraph(description),
      fullDescription: description,
      featureBullets: featureBullets(description),
    };
  });
}

/** A text's problems with Amazon's limits and rules (no other stores). */
export function textProblems(t: ListingText): string[] {
  const problems: string[] = [];
  const at = `${t.code} (${t.play})`;
  const all = [t.title, t.shortDescription, t.fullDescription, ...t.featureBullets].join('\n');
  const stores = otherStores(all);
  if (stores.length > 0) problems.push(`${at}: names ${stores.join(', ')}`);
  if (t.title === '') problems.push(`${at}: no title`);
  if (chars(t.title) > LIMITS.title)
    problems.push(
      `${at}: the title has ${chars(t.title)} characters, Amazon takes ${LIMITS.title}`,
    );
  if (t.shortDescription === '') problems.push(`${at}: no short description`);
  if (
    bytes(t.shortDescription) > LIMITS.shortBytes ||
    chars(t.shortDescription) > LIMITS.shortChars
  )
    problems.push(
      `${at}: the short description has ${chars(t.shortDescription)} characters (${bytes(t.shortDescription)} bytes), Amazon takes ${LIMITS.shortChars} (${LIMITS.shortBytes} bytes)`,
    );
  if (t.fullDescription === '') problems.push(`${at}: no description`);
  if (chars(t.fullDescription) > LIMITS.fullDescription)
    problems.push(
      `${at}: the description has ${chars(t.fullDescription)} characters, Amazon takes ${LIMITS.fullDescription}`,
    );
  const n = t.featureBullets.length;
  if (n < LIMITS.bullets.min || n > LIMITS.bullets.max)
    problems.push(
      `${at}: ${n} feature bullets (the description's lists), Amazon takes ${LIMITS.bullets.min} to ${LIMITS.bullets.max}`,
    );
  return problems;
}

export interface ListingPlan {
  texts: ListingText[];
  /** Languages left as Amazon has them, and why. */
  skipped: string[];
}

/** Which languages get their texts: those within Amazon's limits; the default one has to. */
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

/**
 * The listing as Amazon takes it: what Amazon has (keywords, recent changes) with these texts.
 */
export function listingBody(
  t: ListingText,
  existing: Record<string, unknown> | null,
): Record<string, unknown> {
  return {
    ...(existing ?? {}),
    language: t.code,
    title: t.title,
    shortDescription: t.shortDescription,
    fullDescription: t.fullDescription,
    featureBullets: t.featureBullets,
  };
}

export interface ListingImages {
  small: string;
  large: string;
  /** By Amazon's language code. */
  promo: Map<string, string>;
  screenshots: Map<string, string[]>;
}

/** An image's problems with Amazon's rules for screenshots. */
export function screenshotProblems(file: string, width: number, height: number): string[] {
  const fits = SCREENSHOTS.sizes.some(
    ([w, h]) => (width === w && height === h) || (width === h && height === w),
  );
  if (fits) return [];
  return [
    `${path.relative(ROOT, file)} is ${width}×${height}, Amazon takes ${SCREENSHOTS.sizes.map(([w, h]) => `${w}×${h}`).join(', ')} (either way up)`,
  ];
}

/** The listing's images from an images folder, checked. */
export function listingImages(images: string): ListingImages {
  const problems: string[] = [];
  const exact = (file: string, width: number, height: number) => {
    const info = pngInfo(file);
    if (info.width !== width || info.height !== height)
      problems.push(
        `${path.relative(ROOT, file)} is ${info.width}×${info.height}, not ${width}×${height}`,
      );
  };
  exact(SMALL_ICON, IMAGE_TYPES.small.width, IMAGE_TYPES.small.height);
  exact(LARGE_ICON, IMAGE_TYPES.large.width, IMAGE_TYPES.large.height);
  const result: ListingImages = {
    small: SMALL_ICON,
    large: LARGE_ICON,
    promo: new Map(),
    screenshots: new Map(),
  };
  for (const { code, play } of AMAZON) {
    const site = PLAY.find((p) => p.locale === play)?.site ?? play;
    const promo = path.join(images, site, 'feature.png');
    if (fs.existsSync(promo)) {
      exact(promo, IMAGE_TYPES.promo.width, IMAGE_TYPES.promo.height);
      result.promo.set(code, promo);
    }
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
        `${code}: ${files.length} screenshots in ${path.relative(ROOT, dir) || dir}, Amazon takes ${SCREENSHOTS.min} to ${SCREENSHOTS.max}`,
      );
    for (const f of files) {
      const { width, height } = pngInfo(f);
      problems.push(...screenshotProblems(f, width, height));
    }
    result.screenshots.set(code, files);
  }
  if (problems.length > 0) throw new Error(`the images have problems:\n${problems.join('\n')}`);
  return result;
}

/** How many images listImages lists (a list, or `{images}`), null when it can't tell. */
export function imageCount(body: unknown): number | null {
  if (Array.isArray(body)) return body.length;
  if (isObject(body) && Array.isArray(body.images)) return body.images.length;
  return null;
}

/**
 * A listing's images of one type: the single ones (icons, the promotional image) replace
 * Amazon's, the screenshots replace all of Amazon's.
 */
async function putImages(
  c: Connection,
  editId: string,
  language: string,
  type: string,
  files: string[],
  multiple: boolean,
) {
  const sub = `/edits/${editId}/listings/${encodeURIComponent(language)}/${type}`;
  const list = () => call(c, `listImages ${language} ${type}`, 'GET', sub, { allow404: true });
  const etag = async () => (await list()).etag;
  if (multiple) {
    const current = await list();
    if (current.status !== 404 && imageCount(current.body) !== 0)
      await call(c, `deleteAllImages ${language} ${type}`, 'DELETE', sub, {
        ifMatch: current.etag,
      });
  }
  for (const file of files) {
    await call(
      c,
      `uploadImage ${language} ${type} ${path.basename(file)}`,
      'POST',
      `${sub}/upload`,
      {
        file: { path: file, type: 'image/png' },
        ifMatch: await etag(),
      },
    );
  }
}

/** The listing into the edit: texts, and the images when there are. Its summary. */
export async function listing(
  c: Connection,
  texts: ListingText[],
  images: ListingImages | null,
  log: Log,
): Promise<string[]> {
  const edit = await openEdit(c, log);
  const plan = planListing(texts);
  const done: string[] = [];
  for (const t of plan.texts) {
    const sub = `/edits/${edit.id}/listings/${encodeURIComponent(t.code)}`;
    const current = await call(c, `getListing ${t.code}`, 'GET', sub, { allow404: true });
    const exists = current.status !== 404 && isObject(current.body);
    try {
      await call(c, `updateListing ${t.code}`, 'PUT', sub, {
        json: listingBody(t, exists && isObject(current.body) ? current.body : null),
        ifMatch: exists ? current.etag : null,
      });
    } catch (e) {
      // A language the app doesn't have yet: added in the Developer Console
      if (exists || !(e instanceof AmazonError)) throw e;
      plan.skipped.push(
        `${t.code} (${t.play}): Amazon didn't add the language (${e.message}): add it in the Developer Console (Appstore details, the language menu), then run this again`,
      );
      continue;
    }
    if (images !== null) {
      await putImages(c, edit.id, t.code, IMAGE_TYPES.small.type, [images.small], false);
      await putImages(c, edit.id, t.code, IMAGE_TYPES.large.type, [images.large], false);
      const promo = images.promo.get(t.code);
      if (promo !== undefined)
        await putImages(c, edit.id, t.code, IMAGE_TYPES.promo.type, [promo], false);
      const shots = images.screenshots.get(t.code) ?? [];
      await putImages(c, edit.id, t.code, IMAGE_TYPES.screenshots.type, shots, true);
      log(`${t.code}: the texts, the icons and ${shots.length} screenshots`);
    } else {
      log(`${t.code}: the texts`);
    }
    done.push(t.code);
  }
  return [
    `Amazon Appstore: the listing in ${done.length} languages (${done.join(', ')})${images === null ? ', texts only' : ', with the icons, the promotional image and the screenshots'}, in the edit ${edit.id} (not submitted: the next release submits it, or the Developer Console)`,
    ...plan.skipped.map((s) => `Not changed: ${s}`),
  ];
}

// --- Command line --------------------------------------------------------------------------

const USAGE = `usage: node scripts/amazon-appstore.ts release --version <x.y.z> --code <n> [--whatsnew <file>] [--submit] [--dry-run] <apk>...
       node scripts/amazon-appstore.ts listing [--images <dir>] [--dry-run]`;

function usage(): never {
  console.error(USAGE);
  process.exit(2);
}

/** The security profile and the app from the environment; checked unless a dry run. */
function environment(env: NodeJS.ProcessEnv, dryRun: boolean) {
  const clientId = env.AMAZON_CLIENT_ID ?? '';
  const clientSecret = env.AMAZON_CLIENT_SECRET ?? '';
  const appId = env.AMAZON_APP_ID ?? '';
  const missing = [
    ['AMAZON_CLIENT_ID', clientId],
    ['AMAZON_CLIENT_SECRET', clientSecret],
    ['AMAZON_APP_ID', appId],
  ]
    .filter(([, value]) => value === '')
    .map(([name]) => name);
  if (missing.length > 0 && !dryRun) throw new Error(`${missing.join(', ')} not set`);
  if (clientId !== '' && !clientId.startsWith('amzn1.application-oa2-client.'))
    throw new Error(
      "AMAZON_CLIENT_ID isn't a security profile's client ID (amzn1.application-oa2-client.<...>)",
    );
  if (appId !== '' && !appId.startsWith('amzn1.devportal.mobileapp.'))
    throw new Error("AMAZON_APP_ID isn't the app's ID (amzn1.devportal.mobileapp.<...>)");
  return { clientId, clientSecret, appId };
}

async function connect(env: NodeJS.ProcessEnv): Promise<Connection> {
  const { clientId, clientSecret, appId } = environment(env, false);
  const token = await accessToken(fetch, env.AMAZON_TOKEN_URL ?? TOKEN_URL, clientId, clientSecret);
  return { fetch, api: env.AMAZON_APPSTORE_API ?? API, appId, token };
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
    const notes = releaseNotes(
      whatsnew === undefined ? '' : fs.readFileSync(whatsnew, 'utf8').trim(),
      version,
    );
    const lines = notes.note === undefined ? [] : [notes.note];
    environment(env, dryRun);
    if (dryRun) {
      return [
        ...lines,
        `Amazon Appstore, dry run: ${version} would replace the edit's binaries of ${list.map((a) => `${a.abi} with ${path.basename(a.file)} (versionCode ${a.versionCode}, ${(fs.statSync(a.file).size / 1048576).toFixed(1)} MB)`).join(', ')}, as far as the app has them, the recent changes "${notes.text.slice(0, 80)}${notes.text.length > 80 ? '...' : ''}"${submit ? ', then submitted for review' : ', not submitted'}`,
      ];
    }
    const c = await connect(env);
    return lines.concat(
      await release(c, { version, code, apks: list, recentChanges: notes.text, submit }, log),
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
    environment(env, dryRun);
    if (dryRun) {
      return [
        `Amazon Appstore, dry run: the listing in ${texts.length} languages${images === null ? ', texts only' : `, with the icons, ${images.promo.size} promotional images and ${[...images.screenshots.values()].reduce((n, s) => n + s.length, 0)} screenshots`}`,
        ...problems.map((p) => `Would keep Amazon's text: ${p}`),
      ];
    }
    const c = await connect(env);
    return await listing(c, texts, images, log);
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
