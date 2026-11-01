// Play Games Services: the achievements and leaderboards of the game in the Play Console (app/
// README.md, Game achievements), from the catalogue (catalogue.ts), with the Games Services
// Publishing API (gamesconfiguration v1configuration, developer.android.com/games/services/
// publishing/api) and the service account fastlane uploads with.
//
//   node store/achievements/play.ts              upload (PLAY_SERVICE_ACCOUNT_JSON: the key
//                                                 file's contents or path), write the IDs
//   node store/achievements/play.ts --dry-run    read what Play has, change nothing
//   node store/achievements/play.ts --offline    the same against an empty project, no key
//   --ids <file>                                  where the IDs go (default
//                                                 target/store/achievements/achievements-play.json)
//
// The project is `project_id` of app/achievements-play.json (Play Console, Play Games Services,
// Configuration: the project ID under the game's name), or PLAY_GAMES_PROJECT_ID.
//
// An achievement or leaderboard is found by its ID in app/achievements-play.json, else by its
// English name (Play's names are unique, and the import ZIP of catalogue.ts creates them with
// these), else created. Then its type, steps, state, points, names and descriptions in every
// locale are set when they differ, so a second run changes nothing.
//
// What the API can't do, done in the Play Console (app/README.md): icons (its image upload is
// gone; the import ZIP has them), the order, and publishing. Type, steps and hidden can't be
// changed once published.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  type Api,
  asArray,
  asNumber,
  asString,
  counting,
  dig,
  dryRun,
  httpClient,
  isRecord,
  jwt,
  listWrites,
  messageOf,
  needString,
  type Recording,
} from './api.ts';
import {
  buildCatalogue,
  type Catalogue,
  formatPlayIds,
  loadPlayIds,
  loadSources,
  OUT,
  type PlayAchievement,
  type PlayIds,
  type PlayLeaderboard,
  type Texts,
  validate,
} from './catalogue.ts';

export const BASE = 'https://gamesconfiguration.googleapis.com/games/v1configuration';
export const SCOPE = 'https://www.googleapis.com/auth/androidpublisher';

const bundle = (byLocale: Texts) => ({
  translations: Object.entries(byLocale).map(([locale, value]) => ({ locale, value })),
});
/** A localized string bundle of Play's answers, by locale. */
const fromBundle = (b: unknown): Record<string, unknown> =>
  Object.fromEntries(
    asArray(dig(b, 'translations')).map((t) => [String(dig(t, 'locale')), dig(t, 'value')]),
  );
const sameMap = (a: Record<string, unknown>, b: Record<string, unknown>) => {
  const ka = Object.keys(a);
  return ka.length === Object.keys(b).length && ka.every((k) => a[k] === b[k]);
};

/** Every item of a list. */
async function all(api: Api, path: string): Promise<Remote[]> {
  const items: Remote[] = [];
  let token: string | undefined;
  do {
    const page = await api.get(
      `${path}?maxResults=200${token ? `&pageToken=${encodeURIComponent(token)}` : ''}`,
    );
    items.push(...asArray(dig(page, 'items')).map(remoteItem));
    token = asString(dig(page, 'nextPageToken'));
  } while (token);
  return items;
}

/** An achievement or leaderboard as Play has it: its fields as Play sent them, and its ID. */
interface Remote {
  id: string;
  fields: Record<string, unknown>;
}

function remoteItem(value: unknown): Remote {
  return { id: needString(dig(value, 'id'), 'id'), fields: isRecord(value) ? value : {} };
}

/** The configuration Play gets for an achievement. */
export function achievementBody(a: PlayAchievement) {
  return {
    achievementType: a.achievementType,
    initialState: a.initialState,
    ...(a.steps ? { stepsToUnlock: a.steps } : {}),
    draft: { pointValue: a.points, name: bundle(a.name), description: bundle(a.description) },
  };
}

export function leaderboardBody(b: PlayLeaderboard) {
  return { scoreOrder: b.scoreOrder, draft: { name: bundle(b.name), scoreFormat: b.scoreFormat } };
}

/** What differs between Play's achievement and ours, by field name. */
function achievementChanges(remote: Record<string, unknown>, a: PlayAchievement) {
  const changes: string[] = [];
  if (remote.achievementType !== a.achievementType) changes.push('type');
  if (remote.initialState !== a.initialState) changes.push('hidden');
  if ((remote.stepsToUnlock ?? null) !== (a.steps ?? null) && a.steps) changes.push('steps');
  if (dig(remote, 'draft', 'pointValue') !== a.points) changes.push('points');
  if (!sameMap(fromBundle(dig(remote, 'draft', 'name')), a.name)) changes.push('names');
  if (!sameMap(fromBundle(dig(remote, 'draft', 'description')), a.description))
    changes.push('descriptions');
  return changes;
}

function leaderboardChanges(remote: Record<string, unknown>, b: PlayLeaderboard) {
  const changes: string[] = [];
  if (remote.scoreOrder !== b.scoreOrder) changes.push('order');
  if (!sameMap(fromBundle(dig(remote, 'draft', 'name')), b.name)) changes.push('names');
  const f = (field: string) => dig(remote, 'draft', 'scoreFormat', field);
  if (
    f('numberFormatType') !== b.scoreFormat.numberFormatType ||
    (f('numDecimalPlaces') ?? 0) !== (b.scoreFormat.numDecimalPlaces ?? 0)
  ) {
    changes.push('format');
  }
  return changes;
}

/**
 * Brings Play's project in line with `catalogue.play`. Returns the IDs (as
 * app/achievements-play.json has them, also when some failed), notes for the summary and what
 * failed: one that fails doesn't keep the others from going up.
 */
export async function syncPlay({
  api,
  catalogue,
  ids,
  projectId,
  log = console.log,
}: {
  api: Api;
  catalogue: Catalogue;
  ids: PlayIds;
  projectId: string;
  log?: (message: string) => void;
}): Promise<{ ids: PlayIds; notes: string[]; failures: string[] }> {
  const notes: string[] = [];
  const failures: string[] = [];
  const out: PlayIds = { project_id: projectId, achievements: {}, leaderboards: {} };
  async function syncKind<T extends PlayAchievement | PlayLeaderboard>(
    kind: string,
    plural: Plural,
    items: readonly T[],
    body: (item: T) => object,
    changesOf: (remote: Record<string, unknown>, item: T) => string[],
  ) {
    const remote = await all(api, `/applications/${projectId}/${plural}`);
    const byId = new Map(remote.map((r) => [r.id, r]));
    const byName = new Map(
      remote.map((r) => [fromBundle(dig(r.fields, 'draft', 'name'))['en-US'], r]),
    );
    for (const item of items) {
      try {
        await syncItem({
          api,
          kind,
          plural,
          item,
          body,
          changesOf,
          ids,
          byId,
          byName,
          out,
          notes,
          projectId,
          log,
        });
      } catch (e) {
        // The others still go up; the run fails at the end
        log(`${kind} ${item.id}: ${messageOf(e)}`);
        failures.push(`${kind} ${item.id}: ${messageOf(e)}`);
      }
    }
  }
  await syncKind(
    'achievement',
    'achievements',
    catalogue.play.achievements,
    achievementBody,
    achievementChanges,
  );
  await syncKind(
    'leaderboard',
    'leaderboards',
    catalogue.play.leaderboards,
    leaderboardBody,
    leaderboardChanges,
  );
  return { ids: out, notes, failures };
}

type Plural = 'achievements' | 'leaderboards';

/** One achievement or leaderboard: found, created or changed; its ID into `out`. */
async function syncItem<T extends PlayAchievement | PlayLeaderboard>({
  api,
  kind,
  plural,
  item,
  body,
  changesOf,
  ids,
  byId,
  byName,
  out,
  notes,
  projectId,
  log,
}: {
  api: Api;
  kind: string;
  plural: Plural;
  item: T;
  body: (item: T) => object;
  changesOf: (remote: Record<string, unknown>, item: T) => string[];
  ids: PlayIds;
  byId: Map<string, Remote>;
  byName: Map<unknown, Remote>;
  out: PlayIds;
  notes: string[];
  projectId: string;
  log: (message: string) => void;
}) {
  const known = ids[plural][item.id];
  if (known && !byId.has(known))
    notes.push(`${kind} ${item.id}: ${known} of app/achievements-play.json isn't in the project`);
  const found = (known && byId.get(known)) || byName.get(item.name['en-US']);
  if (!found) {
    log(`${kind} ${item.id}: new`);
    const created = await api.post(`/applications/${projectId}/${plural}`, body(item));
    out[plural][item.id] = needString(dig(created, 'id'), 'id');
    return;
  }
  out[plural][item.id] = found.id;
  const changes = changesOf(found.fields, item);
  if (changes.length > 0) {
    log(`${kind} ${item.id}: ${changes.join(', ')}`);
    if (
      found.fields.published &&
      changes.some((c) => ['type', 'hidden', 'steps', 'order'].includes(c))
    ) {
      notes.push(
        `${kind} ${item.id} is published: Play keeps its ${changes.join(', ')} (it refuses to change them)`,
      );
    }
    // `token` is the version read: Play refuses an update without it (410 UpdateTokenInvalid)
    await api.put(`/${plural}/${found.id}`, {
      id: found.id,
      token: found.fields.token,
      ...body(item),
    });
  }
}

/** What Play answers a write with, for a dry run: the item with a made-up ID. */
export function playAnswer(method: string, _path: string, body: unknown, id: string): unknown {
  const fields = isRecord(body) ? body : {};
  return method === 'POST' ? { ...fields, id } : { ...fields };
}

/** Google's token for the service account (its key file's JSON), renewed before it runs out. */
export function playClient(env = process.env) {
  const raw = env.PLAY_SERVICE_ACCOUNT_JSON;
  if (!raw) throw new Error('PLAY_SERVICE_ACCOUNT_JSON is needed');
  const account: unknown = JSON.parse(
    raw.trim().startsWith('{') ? raw : fs.readFileSync(raw, 'utf8'),
  );
  const privateKey = needString(dig(account, 'private_key'), 'private_key in the key file');
  const email = needString(dig(account, 'client_email'), 'client_email in the key file');
  const tokenUri = needString(dig(account, 'token_uri'), 'token_uri in the key file');
  let cached: { until: number; value: string } | null = null;
  const token = async () => {
    const now = Math.floor(Date.now() / 1000);
    if (!cached || cached.until < now) {
      const assertion = jwt({
        alg: 'RS256',
        key: privateKey,
        payload: {
          iss: email,
          scope: SCOPE,
          aud: tokenUri,
          iat: now,
          exp: now + 3600,
        },
      });
      const response = await fetch(tokenUri, {
        method: 'POST',
        headers: { 'content-type': 'application/x-www-form-urlencoded' },
        body: new URLSearchParams({
          grant_type: 'urn:ietf:params:oauth:grant-type:jwt-bearer',
          assertion,
        }),
      });
      const answer: unknown = await response.json();
      if (!response.ok)
        throw new Error(
          `Google's token: ${response.status} ${asString(dig(answer, 'error')) ?? ''} ${asString(dig(answer, 'error_description')) ?? ''}`,
        );
      cached = {
        until: now + Math.min(asNumber(dig(answer, 'expires_in')) ?? 3600, 3600) - 300,
        value: needString(dig(answer, 'access_token'), "Google's access_token"),
      };
    }
    return cached.value;
  };
  return httpClient({ base: BASE, token });
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const argv = process.argv.slice(2);
  const offline = argv.includes('--offline');
  const dry = offline || argv.includes('--dry-run');
  const idsFile = argv.includes('--ids')
    ? argv[argv.indexOf('--ids') + 1]
    : path.join(OUT, 'achievements-play.json');
  if (!idsFile) {
    console.error('--ids needs a file');
    process.exit(1);
  }
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
  let api: Recording;
  if (offline) {
    const { fakePlay } = await import('./fake.ts');
    api = counting(fakePlay());
  } else if (dry) {
    api = dryRun(playClient(), { answer: playAnswer, empty: () => ({ items: [] }) });
  } else {
    api = counting(playClient());
  }
  const result = await syncPlay({ api, catalogue, ids, projectId });
  const changes = listWrites(api.writes);
  const verb = dry ? 'would send' : 'sent';
  console.log(
    `\nPlay Games: ${verb} ${changes.length} change(s)${offline ? ' (to an empty project)' : ''}`,
  );
  for (const c of changes) console.log(`  ${c}`);
  for (const note of result.notes) console.log(`Note: ${note}`);
  for (const failure of result.failures) console.error(`::error::${failure.split('\n')[0]}`);
  const json = formatPlayIds(result.ids, catalogue);
  fs.mkdirSync(path.dirname(idsFile), { recursive: true });
  fs.writeFileSync(idsFile, json);
  const before = formatPlayIds(ids, catalogue);
  console.log(
    json === before
      ? 'app/achievements-play.json is up to date'
      : `The IDs changed: ${idsFile} goes into app/achievements-play.json`,
  );
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
