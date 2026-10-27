// Play Games Services: the achievements and leaderboards of the game in the Play Console (app/
// README.md, Game achievements), from the catalogue (catalogue.mjs), with the Games Services
// Publishing API (gamesconfiguration v1configuration, developer.android.com/games/services/
// publishing/api) and the service account fastlane uploads with.
//
//   node store/achievements/play.mjs              upload (PLAY_SERVICE_ACCOUNT_JSON: the key
//                                                 file's contents or path), write the IDs
//   node store/achievements/play.mjs --dry-run    read what Play has, change nothing
//   node store/achievements/play.mjs --offline    the same against an empty project, no key
//   --ids <file>                                  where the IDs go (default
//                                                 target/store/achievements/achievements-play.json)
//
// The project is `project_id` of app/achievements-play.json (Play Console, Play Games Services,
// Configuration: the project ID under the game's name), or PLAY_GAMES_PROJECT_ID.
//
// An achievement or leaderboard is found by its ID in app/achievements-play.json, else by its
// English name (Play's names are unique, and the import ZIP of catalogue.mjs creates them with
// these), else created. Then its type, steps, state, points, names and descriptions in every
// locale are set when they differ, so a second run changes nothing.
//
// What the API can't do, done in the Play Console (app/README.md): icons (its image upload is
// gone; the import ZIP has them), the order, and publishing. Type, steps and hidden can't be
// changed once published.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { counting, describe, dryRun, httpClient, jwt } from './api.mjs';
import { buildCatalogue, formatPlayIds, loadPlayIds, loadSources, OUT, validate } from './catalogue.mjs';

export const BASE = 'https://gamesconfiguration.googleapis.com/games/v1configuration';
export const SCOPE = 'https://www.googleapis.com/auth/androidpublisher';

const bundle = (byLocale) => ({ translations: Object.entries(byLocale).map(([locale, value]) => ({ locale, value })) });
const fromBundle = (b) => Object.fromEntries((b?.translations ?? []).map((t) => [t.locale, t.value]));
const sameMap = (a, b) => {
  const ka = Object.keys(a);
  return ka.length === Object.keys(b).length && ka.every((k) => a[k] === b[k]);
};

/** Every item of a list. */
async function all(api, path) {
  const items = [];
  let token = null;
  do {
    const page = await api.get(`${path}?maxResults=200${token ? `&pageToken=${encodeURIComponent(token)}` : ''}`);
    items.push(...(page?.items ?? []));
    token = page?.nextPageToken ?? null;
  } while (token);
  return items;
}

/** The configuration Play gets for an achievement. */
export function achievementBody(a) {
  return {
    achievementType: a.achievementType,
    initialState: a.initialState,
    ...(a.steps ? { stepsToUnlock: a.steps } : {}),
    draft: { pointValue: a.points, name: bundle(a.name), description: bundle(a.description) },
  };
}

export function leaderboardBody(b) {
  return { scoreOrder: b.scoreOrder, draft: { name: bundle(b.name), scoreFormat: b.scoreFormat } };
}

/** What differs between Play's achievement and ours, by field name. */
function achievementChanges(remote, a) {
  const d = remote.draft ?? {};
  const changes = [];
  if (remote.achievementType !== a.achievementType) changes.push('type');
  if (remote.initialState !== a.initialState) changes.push('hidden');
  if ((remote.stepsToUnlock ?? null) !== (a.steps ?? null) && a.steps) changes.push('steps');
  if (d.pointValue !== a.points) changes.push('points');
  if (!sameMap(fromBundle(d.name), a.name)) changes.push('names');
  if (!sameMap(fromBundle(d.description), a.description)) changes.push('descriptions');
  return changes;
}

function leaderboardChanges(remote, b) {
  const d = remote.draft ?? {};
  const changes = [];
  if (remote.scoreOrder !== b.scoreOrder) changes.push('order');
  if (!sameMap(fromBundle(d.name), b.name)) changes.push('names');
  const f = d.scoreFormat ?? {};
  if (f.numberFormatType !== b.scoreFormat.numberFormatType || (f.numDecimalPlaces ?? 0) !== (b.scoreFormat.numDecimalPlaces ?? 0)) {
    changes.push('format');
  }
  return changes;
}

/**
 * Brings Play's project in line with `catalogue.play`. Returns the IDs (as
 * app/achievements-play.json has them, also when some failed), notes for the summary and what
 * failed: one that fails doesn't keep the others from going up.
 */
export async function syncPlay({ api, catalogue, ids, projectId, log = console.log }) {
  const notes = [];
  const failures = [];
  const out = { project_id: projectId, achievements: {}, leaderboards: {} };
  const kinds = [
    ['achievement', 'achievements', catalogue.play.achievements, achievementBody, achievementChanges],
    ['leaderboard', 'leaderboards', catalogue.play.leaderboards, leaderboardBody, leaderboardChanges],
  ];
  for (const [kind, plural, items, body, changesOf] of kinds) {
    const remote = await all(api, `/applications/${projectId}/${plural}`);
    const byId = new Map(remote.map((r) => [r.id, r]));
    const byName = new Map(remote.map((r) => [fromBundle(r.draft?.name)['en-US'], r]));
    for (const item of items) {
      try {
        await syncItem({ api, kind, plural, item, body, changesOf, ids, byId, byName, out, notes, projectId, log });
      } catch (e) {
        // The others still go up; the run fails at the end
        log(`${kind} ${item.id}: ${e.message}`);
        failures.push(`${kind} ${item.id}: ${e.message}`);
      }
    }
  }
  return { ids: out, notes, failures };
}

/** One achievement or leaderboard: found, created or changed; its ID into `out`. */
async function syncItem({ api, kind, plural, item, body, changesOf, ids, byId, byName, out, notes, projectId, log }) {
  const known = ids[plural][item.id];
  if (known && !byId.has(known)) notes.push(`${kind} ${item.id}: ${known} of app/achievements-play.json isn't in the project`);
  const found = byId.get(known) ?? byName.get(item.name['en-US']);
  if (!found) {
    log(`${kind} ${item.id}: new`);
    const created = await api.post(`/applications/${projectId}/${plural}`, body(item));
    out[plural][item.id] = created.id;
    return;
  }
  out[plural][item.id] = found.id;
  const changes = changesOf(found, item);
  if (changes.length > 0) {
    log(`${kind} ${item.id}: ${changes.join(', ')}`);
    if (found.published && changes.some((c) => ['type', 'hidden', 'steps', 'order'].includes(c))) {
      notes.push(`${kind} ${item.id} is published: Play keeps its ${changes.join(', ')} (it refuses to change them)`);
    }
    await api.put(`/${plural}/${found.id}`, { id: found.id, ...body(item) });
  }
}

/** What Play answers a write with, for a dry run: the item with a made-up ID. */
export function playAnswer(method, path, body, id) {
  return method === 'POST' ? { ...body, id } : { ...body };
}

/** Google's token for the service account (its key file's JSON), renewed before it runs out. */
export function playClient(env = process.env) {
  const raw = env.PLAY_SERVICE_ACCOUNT_JSON;
  if (!raw) throw new Error('PLAY_SERVICE_ACCOUNT_JSON is needed');
  const account = JSON.parse(raw.trim().startsWith('{') ? raw : fs.readFileSync(raw, 'utf8'));
  let cached = null;
  const token = async () => {
    const now = Math.floor(Date.now() / 1000);
    if (!cached || cached.until < now) {
      const assertion = jwt({
        alg: 'RS256',
        key: account.private_key,
        payload: { iss: account.client_email, scope: SCOPE, aud: account.token_uri, iat: now, exp: now + 3600 },
      });
      const response = await fetch(account.token_uri, {
        method: 'POST',
        headers: { 'content-type': 'application/x-www-form-urlencoded' },
        body: new URLSearchParams({ grant_type: 'urn:ietf:params:oauth:grant-type:jwt-bearer', assertion }),
      });
      const answer = await response.json();
      if (!response.ok) throw new Error(`Google's token: ${response.status} ${answer.error ?? ''} ${answer.error_description ?? ''}`);
      cached = { until: now + Math.min(answer.expires_in ?? 3600, 3600) - 300, value: answer.access_token };
    }
    return cached.value;
  };
  return httpClient({ base: BASE, token });
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const argv = process.argv.slice(2);
  const offline = argv.includes('--offline');
  const dry = offline || argv.includes('--dry-run');
  const idsFile = argv.includes('--ids') ? argv[argv.indexOf('--ids') + 1] : path.join(OUT, 'achievements-play.json');
  const sources = loadSources();
  const errors = validate(sources);
  if (errors.length > 0) {
    console.error(errors.join('\n'));
    process.exit(1);
  }
  const catalogue = buildCatalogue(sources);
  const ids = loadPlayIds();
  const projectId = process.env.PLAY_GAMES_PROJECT_ID || ids.project_id || (offline ? '0' : null);
  if (!projectId) {
    console.error(
      'No Play Games Services project: put its project ID (Play Console, Play Games Services, Configuration) ' +
        'into project_id of app/achievements-play.json, or set PLAY_GAMES_PROJECT_ID',
    );
    process.exit(1);
  }
  let api;
  if (offline) {
    const { fakePlay } = await import('./fake.mjs');
    api = counting(fakePlay());
  } else if (dry) {
    api = dryRun(playClient(), { answer: playAnswer, empty: () => ({ items: [] }) });
  } else {
    api = counting(playClient());
  }
  const result = await syncPlay({ api, catalogue, ids, projectId });
  const changes = describe(api.writes);
  const verb = dry ? 'would send' : 'sent';
  console.log(`\nPlay Games: ${verb} ${changes.length} change(s)${offline ? ' (to an empty project)' : ''}`);
  for (const c of changes) console.log(`  ${c}`);
  for (const note of result.notes) console.log(`Note: ${note}`);
  for (const failure of result.failures) console.error(`::error::${failure.split('\n')[0]}`);
  const json = formatPlayIds(result.ids, catalogue);
  fs.mkdirSync(path.dirname(idsFile), { recursive: true });
  fs.writeFileSync(idsFile, json);
  const before = formatPlayIds(ids, catalogue);
  console.log(json === before ? 'app/achievements-play.json is up to date' : `The IDs changed: ${idsFile} goes into app/achievements-play.json`);
  if (process.env.GITHUB_STEP_SUMMARY) {
    const lines = [
      '### Play Games',
      '',
      `${verb} ${changes.length} change(s)`,
      ...changes.map((c) => `- \`${c}\``),
      ...result.notes.map((n) => `- ${n}`),
      ...result.failures.map((f) => `- Failed: ${f.split('\n')[0]}`),
      '',
      json === before
        ? 'app/achievements-play.json is up to date.'
        : `The IDs changed${dry ? ' (a dry run: made-up IDs for the new ones, not to commit)' : ''}: this is the new \`app/achievements-play.json\`, also in the artifact \`achievements-play\`.`,
      '',
      ...(json === before ? [] : ['```json', json.trimEnd(), '```']),
      '',
    ];
    fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${lines.join('\n')}\n`);
  }
  if (result.failures.length > 0) {
    console.error(`\n${result.failures.length} failed:\n${result.failures.join('\n')}`);
    process.exit(1);
  }
}
