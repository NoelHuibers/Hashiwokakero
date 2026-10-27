// Game Center: the achievements and leaderboards of the app in App Store Connect (app/README.md,
// Game achievements), from the catalogue (catalogue.mjs) and the icons, with the App Store
// Connect API (developer.apple.com/documentation/appstoreconnectapi/game-center). fastlane has
// no action for them.
//
//   node store/achievements/apple.mjs              upload (APPLE_API_KEY_ID, APPLE_API_ISSUER,
//                                                  APPLE_API_KEY_P8 in the environment)
//   node store/achievements/apple.mjs --dry-run    read what App Store Connect has, change nothing
//   node store/achievements/apple.mjs --offline    the same against an empty App Store Connect,
//                                                  without a key (pull requests, locally)
//
// What it does, each step only when needed, so a second run changes nothing:
// - Game Center on for the app (its gameCenterDetail), and for the iOS version being prepared
//   (its gameCenterAppVersion), if there is one.
// - Every achievement by its vendor id: points, hidden, not repeatable. Its texts and image
//   per locale hang on its latest version (API 4.2's versions): edited while that is being
//   prepared, a new version once it is live (it then goes to review with the next app version).
// - The leaderboards the same way. The daily's starts over every day at midnight UTC.
// Nothing is deleted, submitted or released: the first achievements go to review with an app
// version, chosen on its page in App Store Connect (app/README.md, Game achievements).
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { counting, describe, dryRun, HttpError, httpClient, jwt, tally } from './api.mjs';
import { buildCatalogue, ICONS, loadSources, validate } from './catalogue.mjs';

export const APP_ID = '6826916346';
export const BASE = 'https://api.appstoreconnect.apple.com';
/** Versions of an achievement or leaderboard whose texts and images can still be changed. */
const EDITABLE = new Set(['PREPARE_FOR_SUBMISSION', 'DEVELOPER_REJECTED', 'REJECTED']);
/** Versions that are live or about to be: a change goes into a new version. */
const RELEASED = new Set(['ACCEPTED', 'PENDING_RELEASE', 'LIVE']);
/** App versions Game Center can still be switched on for. */
const OPEN_VERSIONS = 'PREPARE_FOR_SUBMISSION,DEVELOPER_REJECTED,REJECTED,METADATA_REJECTED,INVALID_BINARY';

/** The two kinds of components, by their names in the API. */
const KINDS = {
  achievement: {
    type: 'gameCenterAchievements',
    list: (detail) => `/v1/gameCenterDetails/${detail}/gameCenterAchievementsV2?limit=200`,
    path: '/v2/gameCenterAchievements',
    versionType: 'gameCenterAchievementVersions',
    versionRel: 'achievement',
    localizationType: 'gameCenterAchievementLocalizations',
    imageType: 'gameCenterAchievementImages',
    attributes: (a) => ({ referenceName: a.referenceName, points: a.points, showBeforeEarned: a.showBeforeEarned, repeatable: a.repeatable }),
    created: () => ({}),
  },
  leaderboard: {
    type: 'gameCenterLeaderboards',
    list: (detail) => `/v1/gameCenterDetails/${detail}/gameCenterLeaderboardsV2?limit=200`,
    path: '/v2/gameCenterLeaderboards',
    versionType: 'gameCenterLeaderboardVersions',
    versionRel: 'leaderboard',
    localizationType: 'gameCenterLeaderboardLocalizations',
    imageType: 'gameCenterLeaderboardImages',
    attributes: (b) => ({
      referenceName: b.referenceName,
      defaultFormatter: b.defaultFormatter,
      submissionType: b.submissionType,
      scoreSortType: b.scoreSortType,
    }),
    // A recurring leaderboard's first day: the next midnight UTC. Only set when it's created,
    // a later run leaves the running days alone.
    created: (b, now) =>
      b.recurrence
        ? {
            recurrenceStartDate: new Date(Math.floor(now / 86_400_000 + 1) * 86_400_000).toISOString().replace('.000Z', 'Z'),
            recurrenceDuration: b.recurrence.duration,
            recurrenceRule: b.recurrence.rule,
          }
        : {},
  },
};

const ref = (type, id) => ({ data: { type, id } });

/** Every page of a list. */
async function all(api, path) {
  const data = [];
  const included = [];
  for (let next = path; next; ) {
    const page = await api.get(next);
    data.push(...(page?.data ?? []));
    included.push(...(page?.included ?? []));
    next = page?.links?.next ?? null;
  }
  return { data, included };
}

async function getOrNull(api, path) {
  try {
    return (await api.get(path))?.data ?? null;
  } catch (e) {
    if (e instanceof HttpError && e.status === 404) return null;
    throw e;
  }
}

/** The bytes of an image and the file name that tells it apart: `<id>-<hash>.png`. */
function image(icons, file) {
  const bytes = fs.readFileSync(path.join(icons, file));
  const hash = crypto.createHash('sha256').update(bytes).digest('hex').slice(0, 12);
  return { bytes, fileName: `${path.basename(file, '.png')}-${hash}.png` };
}

/**
 * Brings App Store Connect in line with `catalogue.apple`. Returns notes for the summary and
 * what failed: one achievement that fails doesn't keep the others from going up.
 * `api` is httpClient's (or a dry run's, or a fake's).
 */
export async function syncApple({ api, catalogue, icons = ICONS, appId = APP_ID, log = console.log, now = Date.now() }) {
  const notes = [];
  const failures = [];
  const apple = catalogue.apple;
  // Every image there before anything is changed
  const missing = [...apple.achievements, ...apple.leaderboards].filter((x) => !fs.existsSync(path.join(icons, x.image)));
  if (missing.length > 0) {
    throw new Error(`no icons in ${icons} for ${missing.map((x) => x.id).join(', ')}: \`pnpm run store:icons\` first`);
  }

  // Game Center for the app
  let detail = await getOrNull(api, `/v1/apps/${appId}/gameCenterDetail`);
  if (!detail) {
    log('Game Center: switched on for the app');
    detail = (await api.post('/v1/gameCenterDetails', { data: { type: 'gameCenterDetails', relationships: { app: ref('apps', appId) } } })).data;
  }

  // ...and for the version being prepared
  const versions = await all(api, `/v1/apps/${appId}/appStoreVersions?filter[platform]=IOS&filter[appVersionState]=${OPEN_VERSIONS}&limit=10`);
  if (versions.data.length === 0) {
    notes.push('No iOS version is being prepared: Game Center is switched on for it by the next run once there is one.');
  }
  for (const version of versions.data) {
    const label = version.attributes?.versionString ?? version.id;
    let gc = await getOrNull(api, `/v1/appStoreVersions/${version.id}/gameCenterAppVersion`);
    if (!gc) {
      log(`Game Center: switched on for version ${label}`);
      gc = (await api.post('/v1/gameCenterAppVersions', {
        data: { type: 'gameCenterAppVersions', relationships: { appStoreVersion: ref('appStoreVersions', version.id) } },
      })).data;
    }
    if (gc.attributes?.enabled !== true) {
      await api.patch(`/v1/gameCenterAppVersions/${gc.id}`, { data: { type: 'gameCenterAppVersions', id: gc.id, attributes: { enabled: true } } });
    }
  }

  for (const [kind, items] of [['achievement', apple.achievements], ['leaderboard', apple.leaderboards]]) {
    const k = KINDS[kind];
    const existing = await all(api, k.list(detail.id));
    const byVendor = new Map(existing.data.map((x) => [x.attributes.vendorIdentifier, x]));
    for (const item of items) {
      try {
        await syncComponent({ api, k, kind, item, remote: byVendor.get(item.id), detail, icons, log, notes, now });
      } catch (e) {
        // The others still go up; the run fails at the end
        log(`${kind} ${item.id}: ${e.message}`);
        failures.push(`${kind} ${item.id}: ${e.message}`);
      }
    }
  }
  return { notes, failures };
}

/** One achievement or leaderboard: created or changed, then its version. */
async function syncComponent({ api, k, kind, item, remote, detail, icons, log, notes, now }) {
  const wanted = k.attributes(item);
  let id;
  if (!remote) {
    log(`${kind} ${item.id}: new`);
    const created = await api.post(k.path, {
      data: {
        type: k.type,
        attributes: { vendorIdentifier: item.id, ...wanted, ...k.created(item, now) },
        relationships: { gameCenterDetail: ref('gameCenterDetails', detail.id), versions: { data: [{ type: k.versionType, id: '${v1}' }] } },
      },
      // The first version, made with it
      included: [{ type: k.versionType, id: '${v1}' }],
    });
    id = created.data.id;
  } else {
    id = remote.id;
    const changed = Object.fromEntries(Object.entries({ ...wanted, archived: false }).filter(([f, v]) => remote.attributes[f] !== v));
    if (Object.keys(changed).length > 0) {
      log(`${kind} ${item.id}: ${Object.keys(changed).join(', ')}`);
      await api.patch(`${k.path}/${id}`, { data: { type: k.type, id, attributes: changed } });
    }
  }
  await syncVersion({ api, k, kind, item, id, icons, log, notes });
}

/** The texts and images of a component's latest version, or of a new one. */
async function syncVersion({ api, k, kind, item, id, icons, log, notes }) {
  const versions = (await all(api, `${k.path}/${id}/versions?limit=200`)).data.sort((a, b) => b.attributes.version - a.attributes.version);
  let version = versions[0];
  const img = image(icons, item.image);
  const plan = version ? await localizationPlan(api, k, version.id, item, img) : null;
  if (version && !EDITABLE.has(version.attributes.state)) {
    if (plan.length === 0) return;
    if (!RELEASED.has(version.attributes.state)) {
      notes.push(`${kind} ${item.id}: version ${version.attributes.version} is ${version.attributes.state}, changed once that is over`);
      return;
    }
    version = null;
  }
  if (!version) {
    log(`${kind} ${item.id}: a new version`);
    version = (await api.post(`/v2/${k.versionType}`, {
      data: { type: k.versionType, relationships: { [k.versionRel]: ref(k.type, id) } },
    })).data;
  }
  const steps = plan && version.id === versions[0]?.id ? plan : await localizationPlan(api, k, version.id, item, img);
  for (const step of steps) await step();
}

/** What a version's localizations need, as steps to run: none when they're all as wanted. */
async function localizationPlan(api, k, versionId, item, img) {
  const { data, included } = await all(api, `/v2/${k.versionType}/${versionId}/localizations?include=image&limit=200`);
  const images = new Map(included.filter((x) => x.type === k.imageType).map((x) => [x.id, x]));
  const byLocale = new Map(data.map((l) => [l.attributes.locale, l]));
  const steps = [];
  for (const [locale, wanted] of Object.entries(item.localizations)) {
    const remote = byLocale.get(locale);
    if (!remote) {
      steps.push(async () => {
        const created = await api.post(`/v2/${k.localizationType}`, {
          data: { type: k.localizationType, attributes: { locale, ...wanted }, relationships: { version: ref(k.versionType, versionId) } },
        });
        await uploadImage(api, k, created.data.id, img);
      });
      continue;
    }
    const changed = Object.fromEntries(Object.entries(wanted).filter(([f, v]) => remote.attributes[f] !== v));
    if (Object.keys(changed).length > 0) {
      steps.push(() => api.patch(`/v2/${k.localizationType}/${remote.id}`, { data: { type: k.localizationType, id: remote.id, attributes: changed } }));
    }
    const imageId = remote.relationships?.image?.data?.id;
    const current = imageId ? images.get(imageId) : null;
    const ok = current && current.attributes.fileName === img.fileName && current.attributes.assetDeliveryState?.state !== 'FAILED';
    if (!ok) {
      steps.push(async () => {
        // An uploaded image can't be replaced, only deleted and uploaded again
        if (imageId) await api.delete(`/v2/${k.imageType}/${imageId}`);
        await uploadImage(api, k, remote.id, img);
      });
    }
  }
  return steps;
}

/** Reserves the image, sends its bytes where App Store Connect says, and commits it. */
async function uploadImage(api, k, localizationId, { bytes, fileName }) {
  const reserved = await api.post(`/v2/${k.imageType}`, {
    data: { type: k.imageType, attributes: { fileName, fileSize: bytes.length }, relationships: { localization: ref(k.localizationType, localizationId) } },
  });
  for (const op of reserved.data.attributes?.uploadOperations ?? []) {
    const headers = Object.fromEntries((op.requestHeaders ?? []).map((h) => [h.name, h.value]));
    await api.upload(op.method, op.url, bytes.subarray(op.offset, op.offset + op.length), headers);
  }
  await api.patch(`/v2/${k.imageType}/${reserved.data.id}`, { data: { type: k.imageType, id: reserved.data.id, attributes: { uploaded: true } } });
}

/** What App Store Connect answers a write with, for a dry run. */
export function appleAnswer(method, path, body, id) {
  if (method !== 'POST') return null;
  const attributes = { ...(body?.data?.attributes ?? {}) };
  if (path.endsWith('Images')) attributes.uploadOperations = [];
  return { data: { type: body?.data?.type, id, attributes: { ...attributes, enabled: false } } };
}

/** What App Store Connect has below an id it doesn't know yet (a dry run's): a first version. */
export function appleEmpty(path) {
  if (/\/versions\?/.test(path)) {
    return { data: [{ type: 'versions', id: 'dry-run-version', attributes: { version: 1, state: 'PREPARE_FOR_SUBMISSION' } }] };
  }
  return { data: [] };
}

/** The API key from the environment, as a client that signs a fresh token every 15 minutes. */
export function appleClient(env = process.env) {
  const { APPLE_API_KEY_ID: kid, APPLE_API_ISSUER: iss, APPLE_API_KEY_P8: p8 } = env;
  if (!kid || !iss || !p8) throw new Error('APPLE_API_KEY_ID, APPLE_API_ISSUER and APPLE_API_KEY_P8 are needed');
  const key = crypto.createPrivateKey(p8.includes('BEGIN') ? p8 : Buffer.from(p8, 'base64').toString());
  let cached = null;
  const token = async () => {
    const now = Math.floor(Date.now() / 1000);
    if (!cached || cached.until < now) {
      cached = { until: now + 15 * 60, value: jwt({ alg: 'ES256', key, kid, payload: { iss, iat: now, exp: now + 20 * 60, aud: 'appstoreconnect-v1' } }) };
    }
    return cached.value;
  };
  return httpClient({ base: BASE, token });
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const args = new Set(process.argv.slice(2));
  const offline = args.has('--offline');
  const sources = loadSources();
  const errors = validate(sources);
  if (errors.length > 0) {
    console.error(errors.join('\n'));
    process.exit(1);
  }
  const catalogue = buildCatalogue(sources);
  let api;
  if (offline) {
    const { fakeApple } = await import('./fake.mjs');
    api = counting(fakeApple());
  } else if (args.has('--dry-run')) {
    api = dryRun(appleClient(), { answer: appleAnswer, empty: appleEmpty });
  } else {
    api = counting(appleClient());
  }
  const { notes, failures } = await syncApple({ api, catalogue });
  const changes = describe(api.writes);
  const verb = offline || args.has('--dry-run') ? 'would send' : 'sent';
  console.log(`\nGame Center: ${verb} ${changes.length} change(s)${offline ? ' (to an empty App Store Connect)' : ''}`);
  const counts = tally(api.writes);
  for (const [what, n] of Object.entries(counts)) console.log(`  ${n} × ${what}`);
  for (const note of notes) console.log(`Note: ${note}`);
  for (const failure of failures) console.error(`::error::${failure.split('\n')[0]}`);
  if (process.env.GITHUB_STEP_SUMMARY) {
    const lines = [`### Game Center`, '', `${verb} ${changes.length} change(s)`, '', ...Object.entries(counts).map(([w, n]) => `- ${n} × \`${w}\``), ...notes.map((n) => `- ${n}`), ...failures.map((f) => `- Failed: ${f.split('\n')[0]}`), ''];
    fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${lines.join('\n')}\n`);
  }
  if (failures.length > 0) {
    console.error(`\n${failures.length} failed:\n${failures.join('\n')}`);
    process.exit(1);
  }
}
