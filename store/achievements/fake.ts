// App Store Connect and Play Games Services in memory, as far as apple.ts and play.ts use
// them: for their tests (scripts/tests/achievements.test.ts) and their `--offline` runs. They
// answer like the real APIs and refuse what those refuse (texts of a version under review or
// live, a second image, changing what a published achievement can't change).
import {
  type Api,
  asArray,
  asNumber,
  asString,
  dig,
  HttpError,
  isRecord,
  needString,
} from './api.ts';

/** A request as a route sees it: the groups of its path pattern, the query and the body. */
interface Request {
  params: string[];
  query: URLSearchParams;
  body: unknown;
}
type Route = [method: string, pattern: RegExp, handler: (request: Request) => unknown];

const parse = (url: string) => {
  const u = new URL(url, 'https://fake.invalid');
  return { path: u.pathname, query: u.searchParams };
};

function router(routes: readonly Route[]) {
  return async (method: string, url: string, body: unknown) => {
    const { path, query } = parse(url);
    for (const [m, pattern, handler] of routes) {
      if (m !== method) continue;
      const match = path.match(pattern);
      if (match) return handler({ params: match.slice(1), query, body });
    }
    throw new HttpError(method, url, 404, { errors: [{ code: 'NOT_FOUND' }] });
  };
}

const conflict = (what: string): never => {
  throw new HttpError('', what, 409, { errors: [{ code: 'STATE_ERROR', detail: what }] });
};

/** What `map` has under `id`; a 404 as the APIs answer when it has nothing. */
function found<T>(map: Map<string, T>, id: string): T {
  const value = map.get(id);
  if (value === undefined) throw new HttpError('', id, 404, {});
  return value;
}

/** The attributes of a request's `data`. */
function attributesOf(body: unknown): Record<string, unknown> {
  const attributes = dig(body, 'data', 'attributes');
  return isRecord(attributes) ? attributes : {};
}

/** The id a request's `data` points to with the relationship `name`. */
const related = (body: unknown, name: string) =>
  needString(dig(body, 'data', 'relationships', name, 'data', 'id'), `relationship ${name}`);

/** An iOS version of the app: `{ id, versionString, state }`. */
export interface AppVersion {
  id: string;
  versionString: string;
  state: string;
}
/** A JSON:API resource as App Store Connect answers with it. */
interface Resource {
  type: string;
  id: string;
  attributes: Record<string, unknown>;
}
interface Component extends Resource {
  versions: string[];
}
interface Version extends Resource {
  attributes: { version: number; state: string };
  component: string;
  localizations: string[];
}
interface Localization extends Resource {
  version: string;
  image: string | null;
}
interface UploadOperation {
  method: string;
  url: string;
  offset: number;
  length: number;
  requestHeaders: { name: string; value: string }[];
}
interface Image extends Resource {
  attributes: {
    fileName: string;
    fileSize: number;
    assetDeliveryState: { state: string };
    uploadOperations?: UploadOperation[];
  };
}

/**
 * App Store Connect with one app and `versions` (iOS versions: `{ id, versionString, state }`),
 * Game Center off. `release()` makes every achievement's and leaderboard's version live, as
 * App Review does.
 */
export function fakeApple({
  versions = [{ id: 'asv-1', versionString: '1.1', state: 'PREPARE_FOR_SUBMISSION' }],
}: {
  versions?: AppVersion[];
} = {}) {
  let n = 0;
  const next = (prefix: string) => `${prefix}-${++n}`;
  const state: {
    detail: Resource | null;
    /** By the id of their app version */
    gcAppVersions: Map<string, Resource>;
    components: Map<string, Component>;
    versions: Map<string, Version>;
    localizations: Map<string, Localization>;
    images: Map<string, Image>;
    /** The bytes uploaded so far, by image */
    uploads: Map<string, number>;
  } = {
    detail: null,
    gcAppVersions: new Map(),
    components: new Map(),
    versions: new Map(),
    localizations: new Map(),
    images: new Map(),
    uploads: new Map(),
  };
  const KIND = {
    Achievement: {
      type: 'gameCenterAchievements',
      versionType: 'gameCenterAchievementVersions',
      rel: 'achievement',
    },
    Leaderboard: {
      type: 'gameCenterLeaderboards',
      versionType: 'gameCenterLeaderboardVersions',
      rel: 'leaderboard',
    },
  };
  const editable = (versionId: string) =>
    ['PREPARE_FOR_SUBMISSION', 'DEVELOPER_REJECTED', 'REJECTED'].includes(
      found(state.versions, versionId).attributes.state,
    );
  const newVersion = (componentId: string, type: string) => {
    const c = found(state.components, componentId);
    if (c.versions.some((v) => editable(v))) conflict('a version is being prepared already');
    const id = next('version');
    const version: Version = {
      id,
      type,
      attributes: { version: c.versions.length + 1, state: 'PREPARE_FOR_SUBMISSION' },
      component: componentId,
      localizations: [],
    };
    state.versions.set(id, version);
    c.versions.push(id);
    return version;
  };
  const localizationOut = (l: Localization) => ({
    type: l.type,
    id: l.id,
    attributes: l.attributes,
    relationships: {
      image: { data: l.image ? { type: found(state.images, l.image).type, id: l.image } : null },
    },
  });

  const routes: Route[] = [
    [
      'GET',
      /^\/v1\/apps\/[^/]+\/gameCenterDetail$/,
      () => {
        if (!state.detail) throw new HttpError('GET', 'gameCenterDetail', 404, {});
        return { data: state.detail };
      },
    ],
    [
      'POST',
      /^\/v1\/gameCenterDetails$/,
      () => {
        if (state.detail) conflict('Game Center is on already');
        state.detail = { type: 'gameCenterDetails', id: next('detail'), attributes: {} };
        return { data: state.detail };
      },
    ],
    [
      'GET',
      /^\/v1\/apps\/[^/]+\/appStoreVersions$/,
      ({ query }) => {
        const states = query.get('filter[appVersionState]')?.split(',');
        return {
          data: versions
            .filter((v) => !states || states.includes(v.state))
            .map((v) => ({
              type: 'appStoreVersions',
              id: v.id,
              attributes: { versionString: v.versionString, appVersionState: v.state },
            })),
        };
      },
    ],
    [
      'GET',
      /^\/v1\/appStoreVersions\/([^/]+)\/gameCenterAppVersion$/,
      ({ params: [id = ''] }) => {
        const gc = state.gcAppVersions.get(id);
        if (!gc) throw new HttpError('GET', 'gameCenterAppVersion', 404, {});
        return { data: gc };
      },
    ],
    [
      'POST',
      /^\/v1\/gameCenterAppVersions$/,
      ({ body }) => {
        const version = related(body, 'appStoreVersion');
        if (state.gcAppVersions.has(version)) conflict('the version has one');
        const gc = {
          type: 'gameCenterAppVersions',
          id: next('gcav'),
          attributes: { enabled: false },
        };
        state.gcAppVersions.set(version, gc);
        return { data: gc };
      },
    ],
    [
      'PATCH',
      /^\/v1\/gameCenterAppVersions\/([^/]+)$/,
      ({ params: [id = ''], body }) => {
        const gc = [...state.gcAppVersions.values()].find((g) => g.id === id);
        if (!gc) throw new HttpError('PATCH', id, 404, {});
        Object.assign(gc.attributes, attributesOf(body));
        return { data: gc };
      },
    ],
  ];

  for (const [name, k] of Object.entries(KIND)) {
    routes.push(
      [
        'GET',
        new RegExp(`^/v1/gameCenterDetails/([^/]+)/gameCenter${name}sV2$`),
        () => ({
          data: [...state.components.values()]
            .filter((c) => c.type === k.type)
            .map((c) => ({ type: c.type, id: c.id, attributes: c.attributes })),
        }),
      ],
      [
        'POST',
        new RegExp(`^/v2/gameCenter${name}s$`),
        ({ body }) => {
          const a = attributesOf(body);
          if (!state.detail || related(body, 'gameCenterDetail') !== state.detail.id)
            conflict('no such Game Center');
          if (
            [...state.components.values()].some(
              (c) => c.attributes.vendorIdentifier === a.vendorIdentifier,
            )
          )
            conflict(`${a.vendorIdentifier} exists`);
          // As App Store Connect: a recurrence's duration needs a time part ('PT24H', not 'P1D')
          const duration = a.recurrenceDuration;
          if (duration !== undefined && !/^P(\d+[YMWD])*T\d+[HMS]/.test(asString(duration) ?? '')) {
            throw new HttpError('POST', name, 409, {
              errors: [
                {
                  code: 'ENTITY_ERROR.ATTRIBUTE.TYPE',
                  detail: `'${duration}' is not a valid value for the attribute 'recurrenceDuration'. Expected an ISO 8601 duration with time components`,
                },
              ],
            });
          }
          const first = dig(body, 'included', 0);
          if (
            dig(first, 'type') !== k.versionType ||
            dig(body, 'data', 'relationships', 'versions', 'data', 0, 'id') !== dig(first, 'id')
          ) {
            throw new HttpError('POST', name, 422, {
              errors: [{ detail: 'a first version is required' }],
            });
          }
          const id = next(name.toLowerCase());
          const component: Component = {
            type: k.type,
            id,
            attributes: { ...a, archived: false },
            versions: [],
          };
          state.components.set(id, component);
          newVersion(id, k.versionType);
          return { data: { type: k.type, id, attributes: component.attributes } };
        },
      ],
      [
        'PATCH',
        new RegExp(`^/v2/gameCenter${name}s/([^/]+)$`),
        ({ params: [id = ''], body }) => {
          const c = found(state.components, id);
          const changes = attributesOf(body);
          const live = c.versions.some((v) => found(state.versions, v).attributes.state === 'LIVE');
          if (live && 'points' in changes && changes.points !== c.attributes.points)
            conflict('points of a live achievement');
          Object.assign(c.attributes, changes);
          return { data: { type: c.type, id, attributes: c.attributes } };
        },
      ],
      [
        'GET',
        new RegExp(`^/v2/gameCenter${name}s/([^/]+)/versions$`),
        ({ params: [id = ''] }) => ({
          data: found(state.components, id).versions.map((v) => {
            const { component, localizations, ...out } = found(state.versions, v);
            return out;
          }),
        }),
      ],
      [
        'POST',
        new RegExp(`^/v2/gameCenter${name}Versions$`),
        ({ body }) => {
          const v = newVersion(related(body, k.rel), k.versionType);
          return { data: { type: v.type, id: v.id, attributes: v.attributes } };
        },
      ],
      [
        'GET',
        new RegExp(`^/v2/gameCenter${name}Versions/([^/]+)/localizations$`),
        ({ params: [id = ''] }) => {
          const locs = found(state.versions, id).localizations.map((l) =>
            found(state.localizations, l),
          );
          return {
            data: locs.map(localizationOut),
            included: locs.flatMap((l) => (l.image ? [found(state.images, l.image)] : [])),
          };
        },
      ],
      [
        'POST',
        new RegExp(`^/v2/gameCenter${name}Localizations$`),
        ({ body }) => {
          const versionId = related(body, 'version');
          if (!editable(versionId)) conflict('the version is not being prepared');
          const v = found(state.versions, versionId);
          const attributes = attributesOf(body);
          if (
            v.localizations.some(
              (l) => found(state.localizations, l).attributes.locale === attributes.locale,
            )
          )
            conflict('the locale exists');
          const l: Localization = {
            type: needString(dig(body, 'data', 'type'), 'type'),
            id: next('loc'),
            attributes: { ...attributes },
            version: versionId,
            image: null,
          };
          state.localizations.set(l.id, l);
          v.localizations.push(l.id);
          return { data: localizationOut(l) };
        },
      ],
      [
        'PATCH',
        new RegExp(`^/v2/gameCenter${name}Localizations/([^/]+)$`),
        ({ params: [id = ''], body }) => {
          const l = found(state.localizations, id);
          if (!editable(l.version)) conflict('the version is not being prepared');
          const changes = attributesOf(body);
          if ('locale' in changes) conflict('the locale stays');
          Object.assign(l.attributes, changes);
          return { data: localizationOut(l) };
        },
      ],
      [
        'POST',
        new RegExp(`^/v2/gameCenter${name}Images$`),
        ({ body }) => {
          const l = found(state.localizations, related(body, 'localization'));
          if (!editable(l.version)) conflict('the version is not being prepared');
          if (l.image) conflict('the localization has an image');
          const id = next('image');
          const attributes = attributesOf(body);
          const fileName = needString(attributes.fileName, 'fileName');
          const fileSize = asNumber(attributes.fileSize) ?? 0;
          const half = Math.ceil(fileSize / 2);
          const image: Image = {
            type: needString(dig(body, 'data', 'type'), 'type'),
            id,
            attributes: {
              fileName,
              fileSize,
              assetDeliveryState: { state: 'AWAITING_UPLOAD' },
              uploadOperations: [
                {
                  method: 'PUT',
                  url: `https://upload.fake.invalid/${id}/0`,
                  offset: 0,
                  length: half,
                  requestHeaders: [{ name: 'Content-Type', value: 'image/png' }],
                },
                {
                  method: 'PUT',
                  url: `https://upload.fake.invalid/${id}/1`,
                  offset: half,
                  length: fileSize - half,
                  requestHeaders: [],
                },
              ],
            },
          };
          state.images.set(id, image);
          state.uploads.set(id, 0);
          l.image = id;
          return { data: image };
        },
      ],
      [
        'PATCH',
        new RegExp(`^/v2/gameCenter${name}Images/([^/]+)$`),
        ({ params: [id = ''], body }) => {
          const image = found(state.images, id);
          if (attributesOf(body).uploaded) {
            image.attributes.assetDeliveryState.state =
              state.uploads.get(id) === image.attributes.fileSize ? 'COMPLETE' : 'FAILED';
            delete image.attributes.uploadOperations;
          }
          return { data: image };
        },
      ],
      [
        'DELETE',
        new RegExp(`^/v2/gameCenter${name}Images/([^/]+)$`),
        ({ params: [id = ''] }) => {
          const l = [...state.localizations.values()].find((x) => x.image === id);
          if (!l) throw new HttpError('DELETE', id, 404, {});
          if (!editable(l.version)) conflict('the version is not being prepared');
          l.image = null;
          state.images.delete(id);
          return null;
        },
      ],
    );
  }

  const handle = router(routes);
  const call = (method: string) => async (path: string, body?: unknown) =>
    structuredClone(await handle(method, path, body));
  const api = {
    get: call('GET'),
    post: call('POST'),
    patch: call('PATCH'),
    put: call('PUT'),
    delete: call('DELETE'),
    upload: async (method: string, url: string, bytes: Uint8Array) => {
      const [, id] = new URL(url).pathname.split('/');
      const sent = id === undefined ? undefined : state.uploads.get(id);
      if (method !== 'PUT' || id === undefined || sent === undefined)
        throw new HttpError(method, url, 404, {});
      state.uploads.set(id, sent + bytes.length);
      return null;
    },
  } satisfies Api;
  return {
    ...api,
    state,
    /** App Review accepted everything: every version being prepared goes live. */
    release() {
      for (const v of state.versions.values()) {
        if (v.attributes.state === 'PREPARE_FOR_SUBMISSION') v.attributes.state = 'LIVE';
      }
    },
  };
}

/** An achievement or leaderboard as Play keeps it: what was sent, its ID and version token. */
interface PlayItem extends Record<string, unknown> {
  id: string;
  token: string;
  published?: unknown;
}

/** The English name of a Play item or request body. */
const englishName = (item: unknown) =>
  dig(
    asArray(dig(item, 'draft', 'name', 'translations')).find((t) => dig(t, 'locale') === 'en-US'),
    'value',
  );

/**
 * Play Games Services with one project. `publish()` publishes the drafts, as the console does.
 * Each item has a `token` that changes with every write; a PUT must send the one it read (else
 * 410 UpdateTokenInvalid, as Play answers).
 */
export function fakePlay() {
  let n = 0;
  let version = 0;
  const state = {
    achievements: new Map<string, PlayItem>(),
    leaderboards: new Map<string, PlayItem>(),
  };
  const LOCKED = {
    achievements: ['achievementType', 'initialState', 'stepsToUnlock'],
    leaderboards: ['scoreOrder'],
  };
  const routes: Route[] = [];
  for (const plural of ['achievements', 'leaderboards'] as const) {
    const items = state[plural];
    routes.push(
      [
        'GET',
        new RegExp(`^/applications/([^/]+)/${plural}$`),
        () => ({ items: [...items.values()] }),
      ],
      [
        'POST',
        new RegExp(`^/applications/([^/]+)/${plural}$`),
        ({ body }) => {
          const name = englishName(body);
          if ([...items.values()].some((x) => englishName(x) === name)) {
            throw new HttpError('POST', plural, 400, {
              error: { message: `the name ${name} is taken` },
            });
          }
          const id = `CgkIfake${String(++n).padStart(4, '0')}`;
          const item: PlayItem = {
            ...(isRecord(body) ? structuredClone(body) : {}),
            id,
            token: `v${++version}`,
          };
          items.set(id, item);
          return item;
        },
      ],
      [
        'PUT',
        new RegExp(`^/${plural}/([^/]+)$`),
        ({ params: [id = ''], body }) => {
          const old = items.get(id);
          if (!old) throw new HttpError('PUT', id, 404, {});
          if (dig(body, 'token') !== old.token) {
            throw new HttpError('PUT', id, 410, {
              error: { code: 410, errors: [{ reason: 'UpdateTokenInvalid' }] },
            });
          }
          if (old.published && LOCKED[plural].some((f) => old[f] !== dig(body, f))) {
            throw new HttpError('PUT', id, 400, {
              error: { message: `${LOCKED[plural].join(', ')} can't change once published` },
            });
          }
          const item: PlayItem = {
            ...(isRecord(body) ? structuredClone(body) : {}),
            id,
            token: `v${++version}`,
            ...(old.published ? { published: old.published } : {}),
          };
          items.set(id, item);
          return item;
        },
      ],
    );
  }
  const handle = router(routes);
  const call = (method: string) => async (path: string, body?: unknown) =>
    structuredClone(await handle(method, path, body));
  const api = {
    get: call('GET'),
    post: call('POST'),
    put: call('PUT'),
    patch: call('PATCH'),
    delete: call('DELETE'),
    upload: async (): Promise<unknown> => {
      throw new Error('Play has no uploads here');
    },
  } satisfies Api;
  return {
    ...api,
    state,
    publish() {
      for (const items of Object.values(state))
        for (const x of items.values()) x.published = structuredClone(x.draft);
    },
  };
}
