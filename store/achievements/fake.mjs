// App Store Connect and Play Games Services in memory, as far as apple.mjs and play.mjs use
// them: for their tests (scripts/tests/achievements.test.mjs) and their `--offline` runs. They
// answer like the real APIs and refuse what those refuse (texts of a version under review or
// live, a second image, changing what a published achievement can't change).
import { HttpError } from './api.mjs';

const parse = (url) => {
  const u = new URL(url, 'https://fake.invalid');
  return { path: u.pathname, query: u.searchParams };
};

function router(routes) {
  return async (method, url, body) => {
    const { path, query } = parse(url);
    for (const [m, pattern, handler] of routes) {
      if (m !== method) continue;
      const match = path.match(pattern);
      if (match) return handler({ params: match.slice(1), query, body });
    }
    throw new HttpError(method, url, 404, { errors: [{ code: 'NOT_FOUND' }] });
  };
}

const conflict = (what) => {
  throw new HttpError('', what, 409, { errors: [{ code: 'STATE_ERROR', detail: what }] });
};

/**
 * App Store Connect with one app and `versions` (iOS versions: `{ id, versionString, state }`),
 * Game Center off. `release()` makes every achievement's and leaderboard's version live, as
 * App Review does.
 */
export function fakeApple({ versions = [{ id: 'asv-1', versionString: '1.1', state: 'PREPARE_FOR_SUBMISSION' }] } = {}) {
  let n = 0;
  const next = (prefix) => `${prefix}-${++n}`;
  const state = { detail: null, gcAppVersions: new Map(), components: new Map(), versions: new Map(), localizations: new Map(), images: new Map(), uploads: new Map() };
  const KIND = {
    Achievement: { type: 'gameCenterAchievements', versionType: 'gameCenterAchievementVersions', rel: 'achievement' },
    Leaderboard: { type: 'gameCenterLeaderboards', versionType: 'gameCenterLeaderboardVersions', rel: 'leaderboard' },
  };
  const editable = (versionId) => ['PREPARE_FOR_SUBMISSION', 'DEVELOPER_REJECTED', 'REJECTED'].includes(state.versions.get(versionId).attributes.state);
  const newVersion = (componentId, type) => {
    const c = state.components.get(componentId);
    if (c.versions.some((v) => editable(v))) conflict('a version is being prepared already');
    const id = next('version');
    state.versions.set(id, { id, type, attributes: { version: c.versions.length + 1, state: 'PREPARE_FOR_SUBMISSION' }, component: componentId, localizations: [] });
    c.versions.push(id);
    return state.versions.get(id);
  };
  const localizationOut = (l) => ({
    type: l.type,
    id: l.id,
    attributes: l.attributes,
    relationships: { image: { data: l.image ? { type: state.images.get(l.image).type, id: l.image } : null } },
  });

  const routes = [
    ['GET', /^\/v1\/apps\/[^/]+\/gameCenterDetail$/, () => {
      if (!state.detail) throw new HttpError('GET', 'gameCenterDetail', 404, {});
      return { data: state.detail };
    }],
    ['POST', /^\/v1\/gameCenterDetails$/, () => {
      if (state.detail) conflict('Game Center is on already');
      state.detail = { type: 'gameCenterDetails', id: next('detail'), attributes: {} };
      return { data: state.detail };
    }],
    ['GET', /^\/v1\/apps\/[^/]+\/appStoreVersions$/, ({ query }) => {
      const states = query.get('filter[appVersionState]')?.split(',');
      return {
        data: versions
          .filter((v) => !states || states.includes(v.state))
          .map((v) => ({ type: 'appStoreVersions', id: v.id, attributes: { versionString: v.versionString, appVersionState: v.state } })),
      };
    }],
    ['GET', /^\/v1\/appStoreVersions\/([^/]+)\/gameCenterAppVersion$/, ({ params: [id] }) => {
      const gc = state.gcAppVersions.get(id);
      if (!gc) throw new HttpError('GET', 'gameCenterAppVersion', 404, {});
      return { data: gc };
    }],
    ['POST', /^\/v1\/gameCenterAppVersions$/, ({ body }) => {
      const version = body.data.relationships.appStoreVersion.data.id;
      if (state.gcAppVersions.has(version)) conflict('the version has one');
      const gc = { type: 'gameCenterAppVersions', id: next('gcav'), attributes: { enabled: false } };
      state.gcAppVersions.set(version, gc);
      return { data: gc };
    }],
    ['PATCH', /^\/v1\/gameCenterAppVersions\/([^/]+)$/, ({ params: [id], body }) => {
      const gc = [...state.gcAppVersions.values()].find((g) => g.id === id);
      Object.assign(gc.attributes, body.data.attributes);
      return { data: gc };
    }],
  ];

  for (const [name, k] of Object.entries(KIND)) {
    routes.push(
      ['GET', new RegExp(`^/v1/gameCenterDetails/([^/]+)/gameCenter${name}sV2$`), () => ({
        data: [...state.components.values()].filter((c) => c.type === k.type).map((c) => ({ type: c.type, id: c.id, attributes: c.attributes })),
      })],
      ['POST', new RegExp(`^/v2/gameCenter${name}s$`), ({ body }) => {
        const a = body.data.attributes;
        if (!state.detail || body.data.relationships.gameCenterDetail.data.id !== state.detail.id) conflict('no such Game Center');
        if ([...state.components.values()].some((c) => c.attributes.vendorIdentifier === a.vendorIdentifier)) conflict(`${a.vendorIdentifier} exists`);
        if (body.included?.[0]?.type !== k.versionType || body.data.relationships.versions.data[0].id !== body.included[0].id) {
          throw new HttpError('POST', name, 422, { errors: [{ detail: 'a first version is required' }] });
        }
        const id = next(name.toLowerCase());
        state.components.set(id, { type: k.type, id, attributes: { ...a, archived: false }, versions: [] });
        newVersion(id, k.versionType);
        return { data: { type: k.type, id, attributes: state.components.get(id).attributes } };
      }],
      ['PATCH', new RegExp(`^/v2/gameCenter${name}s/([^/]+)$`), ({ params: [id], body }) => {
        const c = state.components.get(id);
        const live = c.versions.some((v) => state.versions.get(v).attributes.state === 'LIVE');
        if (live && 'points' in body.data.attributes && body.data.attributes.points !== c.attributes.points) conflict('points of a live achievement');
        Object.assign(c.attributes, body.data.attributes);
        return { data: { type: c.type, id, attributes: c.attributes } };
      }],
      ['GET', new RegExp(`^/v2/gameCenter${name}s/([^/]+)/versions$`), ({ params: [id] }) => ({
        data: state.components.get(id).versions.map((v) => {
          const { component, localizations, ...out } = state.versions.get(v);
          return out;
        }),
      })],
      ['POST', new RegExp(`^/v2/gameCenter${name}Versions$`), ({ body }) => {
        const v = newVersion(body.data.relationships[k.rel].data.id, k.versionType);
        return { data: { type: v.type, id: v.id, attributes: v.attributes } };
      }],
      ['GET', new RegExp(`^/v2/gameCenter${name}Versions/([^/]+)/localizations$`), ({ params: [id] }) => {
        const locs = state.versions.get(id).localizations.map((l) => state.localizations.get(l));
        return {
          data: locs.map(localizationOut),
          included: locs.filter((l) => l.image).map((l) => state.images.get(l.image)),
        };
      }],
      ['POST', new RegExp(`^/v2/gameCenter${name}Localizations$`), ({ body }) => {
        const versionId = body.data.relationships.version.data.id;
        if (!editable(versionId)) conflict('the version is not being prepared');
        const v = state.versions.get(versionId);
        if (v.localizations.some((l) => state.localizations.get(l).attributes.locale === body.data.attributes.locale)) conflict('the locale exists');
        const l = { type: body.data.type, id: next('loc'), attributes: { ...body.data.attributes }, version: versionId, image: null };
        state.localizations.set(l.id, l);
        v.localizations.push(l.id);
        return { data: localizationOut(l) };
      }],
      ['PATCH', new RegExp(`^/v2/gameCenter${name}Localizations/([^/]+)$`), ({ params: [id], body }) => {
        const l = state.localizations.get(id);
        if (!editable(l.version)) conflict('the version is not being prepared');
        if ('locale' in body.data.attributes) conflict('the locale stays');
        Object.assign(l.attributes, body.data.attributes);
        return { data: localizationOut(l) };
      }],
      ['POST', new RegExp(`^/v2/gameCenter${name}Images$`), ({ body }) => {
        const l = state.localizations.get(body.data.relationships.localization.data.id);
        if (!editable(l.version)) conflict('the version is not being prepared');
        if (l.image) conflict('the localization has an image');
        const id = next('image');
        const { fileName, fileSize } = body.data.attributes;
        const half = Math.ceil(fileSize / 2);
        const image = {
          type: body.data.type,
          id,
          attributes: {
            fileName,
            fileSize,
            assetDeliveryState: { state: 'AWAITING_UPLOAD' },
            uploadOperations: [
              { method: 'PUT', url: `https://upload.fake.invalid/${id}/0`, offset: 0, length: half, requestHeaders: [{ name: 'Content-Type', value: 'image/png' }] },
              { method: 'PUT', url: `https://upload.fake.invalid/${id}/1`, offset: half, length: fileSize - half, requestHeaders: [] },
            ],
          },
        };
        state.images.set(id, image);
        state.uploads.set(id, 0);
        l.image = id;
        return { data: image };
      }],
      ['PATCH', new RegExp(`^/v2/gameCenter${name}Images/([^/]+)$`), ({ params: [id], body }) => {
        const image = state.images.get(id);
        if (body.data.attributes.uploaded) {
          image.attributes.assetDeliveryState.state = state.uploads.get(id) === image.attributes.fileSize ? 'COMPLETE' : 'FAILED';
          delete image.attributes.uploadOperations;
        }
        return { data: image };
      }],
      ['DELETE', new RegExp(`^/v2/gameCenter${name}Images/([^/]+)$`), ({ params: [id] }) => {
        const l = [...state.localizations.values()].find((x) => x.image === id);
        if (!editable(l.version)) conflict('the version is not being prepared');
        l.image = null;
        state.images.delete(id);
        return null;
      }],
    );
  }

  const handle = router(routes);
  const call = (method) => async (path, body) => structuredClone(await handle(method, path, body));
  return {
    state,
    get: call('GET'),
    post: call('POST'),
    patch: call('PATCH'),
    put: call('PUT'),
    delete: call('DELETE'),
    upload: async (method, url, bytes) => {
      const [, id] = new URL(url).pathname.split('/');
      if (method !== 'PUT' || !state.uploads.has(id)) throw new HttpError(method, url, 404, {});
      state.uploads.set(id, state.uploads.get(id) + bytes.length);
      return null;
    },
    /** App Review accepted everything: every version being prepared goes live. */
    release() {
      for (const v of state.versions.values()) {
        if (v.attributes.state === 'PREPARE_FOR_SUBMISSION') v.attributes.state = 'LIVE';
      }
    },
  };
}

/**
 * Play Games Services with one project. `publish()` publishes the drafts, as the console does.
 * Each item has a `token` that changes with every write; a PUT must send the one it read (else
 * 410 UpdateTokenInvalid, as Play answers).
 */
export function fakePlay() {
  let n = 0;
  let version = 0;
  const state = { achievements: new Map(), leaderboards: new Map() };
  const LOCKED = { achievements: ['achievementType', 'initialState', 'stepsToUnlock'], leaderboards: ['scoreOrder'] };
  const routes = [];
  for (const plural of ['achievements', 'leaderboards']) {
    routes.push(
      ['GET', new RegExp(`^/applications/([^/]+)/${plural}$`), () => ({ items: [...state[plural].values()] })],
      ['POST', new RegExp(`^/applications/([^/]+)/${plural}$`), ({ body }) => {
        const name = body.draft.name.translations.find((t) => t.locale === 'en-US')?.value;
        if ([...state[plural].values()].some((x) => x.draft.name.translations.some((t) => t.locale === 'en-US' && t.value === name))) {
          throw new HttpError('POST', plural, 400, { error: { message: `the name ${name} is taken` } });
        }
        const id = `CgkIfake${String(++n).padStart(4, '0')}`;
        const item = { ...structuredClone(body), id, token: `v${++version}` };
        state[plural].set(id, item);
        return item;
      }],
      ['PUT', new RegExp(`^/${plural}/([^/]+)$`), ({ params: [id], body }) => {
        const old = state[plural].get(id);
        if (!old) throw new HttpError('PUT', id, 404, {});
        if (body.token !== old.token) {
          throw new HttpError('PUT', id, 410, { error: { code: 410, errors: [{ reason: 'UpdateTokenInvalid' }] } });
        }
        if (old.published && LOCKED[plural].some((f) => old[f] !== body[f])) {
          throw new HttpError('PUT', id, 400, { error: { message: `${LOCKED[plural].join(', ')} can't change once published` } });
        }
        const item = { ...structuredClone(body), id, token: `v${++version}`, ...(old.published ? { published: old.published } : {}) };
        state[plural].set(id, item);
        return item;
      }],
    );
  }
  const handle = router(routes);
  const call = (method) => async (path, body) => structuredClone(await handle(method, path, body));
  return {
    state,
    get: call('GET'),
    post: call('POST'),
    put: call('PUT'),
    patch: call('PATCH'),
    delete: call('DELETE'),
    upload: async () => {
      throw new Error('Play has no uploads here');
    },
    publish() {
      for (const items of Object.values(state)) for (const x of items.values()) x.published = structuredClone(x.draft);
    },
  };
}
