// HTTP for the achievements' uploads (apple.mjs, play.mjs): JSON requests with a bearer token,
// retried when the store says to slow down, and a dry run that sends only the reads.
import crypto from 'node:crypto';

export class HttpError extends Error {
  constructor(method, url, status, body) {
    super(`${method} ${url}: ${status} ${typeof body === 'string' ? body : JSON.stringify(body)}`.slice(0, 2000));
    this.status = status;
    this.body = body;
  }
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/**
 * A client of a JSON API at `base`: `get`, `post`, `put`, `patch`, `delete` with a path, and
 * `upload` of bytes to an absolute URL with its own headers (no token). `token()` gives the
 * bearer token, asked for again on every request (it may renew it). 429 and 5xx are retried,
 * after Retry-After or a growing pause: App Store Connect allows 3600 requests an hour.
 */
export function httpClient({ base, token, fetchImpl = fetch, log = console.log, wait = sleep, retries = 8 }) {
  async function send(method, url, { body, headers = {}, raw = false } = {}) {
    for (let attempt = 0; ; attempt++) {
      const auth = raw ? {} : { authorization: `Bearer ${await token()}` };
      const init = { method, headers: { ...auth, ...headers } };
      if (body !== undefined) {
        init.body = raw ? body : JSON.stringify(body);
        if (!raw) init.headers['content-type'] = 'application/json';
      }
      const response = await fetchImpl(url, init);
      if ((response.status === 429 || response.status >= 500) && attempt < retries) {
        const after = Number(response.headers.get('retry-after'));
        const pause = Number.isFinite(after) && after > 0 ? after * 1000 : Math.min(60_000 * 2 ** attempt, 15 * 60_000);
        log(`${method} ${url}: ${response.status}, again in ${Math.round(pause / 1000)} s`);
        await wait(pause);
        continue;
      }
      const text = await response.text();
      let parsed = text;
      try {
        parsed = text ? JSON.parse(text) : null;
      } catch {
        // Not JSON: kept as text for the error
      }
      if (!response.ok) throw new HttpError(method, url, response.status, parsed);
      return parsed;
    }
  }
  const at = (path) => (path.startsWith('https://') ? path : `${base}${path}`);
  return {
    get: (path) => send('GET', at(path)),
    post: (path, body) => send('POST', at(path), { body }),
    put: (path, body) => send('PUT', at(path), { body }),
    patch: (path, body) => send('PATCH', at(path), { body }),
    delete: (path) => send('DELETE', at(path)),
    upload: (method, url, bytes, headers) => send(method, url, { body: bytes, headers, raw: true }),
  };
}

/**
 * `api` for a dry run: reads go to the store, writes are only written down in `writes` and
 * answered with what the store would answer, with `dry-run-<n>` ids. Reads below such an id are
 * answered empty, the store doesn't know it. `answer(method, path, body, id)` shapes the answers.
 */
export function dryRun(api, { answer, empty }) {
  const writes = [];
  let n = 0;
  const write = (method) => async (path, body) => {
    writes.push({ method, path, body });
    return answer(method, path, body, `dry-run-${++n}`);
  };
  return {
    writes,
    get: (path) => (path.includes('dry-run-') ? empty(path) : api.get(path)),
    post: write('POST'),
    put: write('PUT'),
    patch: write('PATCH'),
    delete: write('DELETE'),
    upload: async (method, url, bytes) => {
      writes.push({ method, path: url, body: `${bytes.length} bytes` });
      return null;
    },
  };
}

/** Counts the writes a sync sends, for its summary: `{ POST: 3, PATCH: 1 }`. */
export function counting(api) {
  const writes = [];
  const wrap = (method) => async (path, body) => {
    writes.push({ method, path, body });
    return api[method.toLowerCase()](path, body);
  };
  return {
    writes,
    get: (path) => api.get(path),
    post: wrap('POST'),
    put: wrap('PUT'),
    patch: wrap('PATCH'),
    delete: wrap('DELETE'),
    upload: async (method, url, bytes, headers) => {
      writes.push({ method, path: url, body: `${bytes.length} bytes` });
      return api.upload(method, url, bytes, headers);
    },
  };
}

const base64url = (data) => Buffer.from(data).toString('base64url');

/** A signed JWT: `alg` ES256 (App Store Connect) or RS256 (Google's service accounts). */
export function jwt({ alg, key, kid, payload }) {
  const header = { alg, typ: 'JWT', ...(kid ? { kid } : {}) };
  const data = `${base64url(JSON.stringify(header))}.${base64url(JSON.stringify(payload))}`;
  const signature =
    alg === 'ES256'
      ? crypto.sign('sha256', Buffer.from(data), { key, dsaEncoding: 'ieee-p1363' })
      : crypto.sign('sha256', Buffer.from(data), key);
  return `${data}.${base64url(signature)}`;
}

/** The write-ups of a sync's changes, one line each, for the log and the job summary. */
export function describe(writes) {
  return writes.map((w) => `${w.method} ${w.path.replace(/^https:\/\/[^/]+/, '')}`);
}

/** The writes by kind, `{ 'PATCH /v2/gameCenterAchievements/{id}': 3 }`; bytes sent are uploads. */
export function tally(writes) {
  const counts = {};
  for (const w of writes) {
    const key = w.path.startsWith('https://')
      ? `${w.method} (image upload)`
      : `${w.method} ${w.path.replace(/\?.*$/, '').replace(/\/(?!v\d\/)[^/]*\d[^/]*/g, '/{id}')}`;
    counts[key] = (counts[key] ?? 0) + 1;
  }
  return counts;
}
