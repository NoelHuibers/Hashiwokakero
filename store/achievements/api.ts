// HTTP for the achievements' uploads (apple.ts, play.ts): JSON requests with a bearer token,
// retried when the store says to slow down, and a dry run that sends only the reads.

import crypto from 'node:crypto';
import { setTimeout as sleep } from 'node:timers/promises';

/**
 * A store's API as the uploads use it. Answers are parsed JSON, unchecked: the uploads read them
 * with `dig` and the helpers below.
 */
export interface Api {
  get(path: string): Promise<unknown>;
  post(path: string, body: unknown): Promise<unknown>;
  put(path: string, body: unknown): Promise<unknown>;
  patch(path: string, body: unknown): Promise<unknown>;
  delete(path: string): Promise<unknown>;
  upload(
    method: string,
    url: string,
    bytes: Uint8Array<ArrayBuffer>,
    headers: Record<string, string>,
  ): Promise<unknown>;
}

/** A write a sync sent (or would have): for its summary and the tests. */
export interface Write {
  method: string;
  path: string;
  body: unknown;
}

/** An API that writes its writes down. */
export interface Recording extends Api {
  writes: Write[];
}

export class HttpError extends Error {
  status: number;
  body: unknown;

  constructor(method: string, url: string, status: number, body: unknown) {
    super(
      `${method} ${url}: ${status} ${typeof body === 'string' ? body : JSON.stringify(body)}`.slice(
        0,
        2000,
      ),
    );
    this.status = status;
    this.body = body;
  }
}

/** An object of parsed JSON (not an array, not null). */
export const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

/** What is at `keys` in parsed JSON: object keys and array indexes; undefined where nothing is. */
export function dig(value: unknown, ...keys: (string | number)[]): unknown {
  let at = value;
  for (const key of keys) {
    if (typeof key === 'number') {
      if (!Array.isArray(at)) return undefined;
      at = at[key];
    } else {
      if (!isRecord(at)) return undefined;
      at = at[key];
    }
  }
  return at;
}

/** A string of parsed JSON, else undefined. */
export const asString = (value: unknown): string | undefined =>
  typeof value === 'string' ? value : undefined;

/** A number of parsed JSON, else undefined. */
export const asNumber = (value: unknown): number | undefined =>
  typeof value === 'number' ? value : undefined;

/** An array of parsed JSON, else empty. */
export const asArray = (value: unknown): unknown[] => (Array.isArray(value) ? value : []);

/** A string the answer must have: `what` names it in the error. */
export function needString(value: unknown, what: string): string {
  if (typeof value !== 'string') throw new Error(`no ${what} in the answer`);
  return value;
}

/** The message of something thrown. */
export const messageOf = (e: unknown): string => (e instanceof Error ? e.message : String(e));

/** What the client hands fetch. */
export interface FetchInit {
  method: string;
  headers: Record<string, string>;
  body?: string | Uint8Array<ArrayBuffer>;
}

/**
 * A client of a JSON API at `base`: `get`, `post`, `put`, `patch`, `delete` with a path, and
 * `upload` of bytes to an absolute URL with its own headers (no token). `token()` gives the
 * bearer token, asked for again on every request (it may renew it). 429 and 5xx are retried,
 * after Retry-After or a growing pause: App Store Connect allows 3600 requests an hour.
 */
export function httpClient({
  base,
  token,
  fetchImpl = fetch,
  log = console.log,
  wait = sleep,
  retries = 8,
}: {
  base: string;
  token: () => Promise<string>;
  fetchImpl?: (url: string, init: FetchInit) => Promise<Response>;
  log?: (message: string) => void;
  wait?: (ms: number) => Promise<unknown>;
  retries?: number;
}): Api {
  async function send(
    method: string,
    url: string,
    {
      json,
      bytes,
      headers = {},
    }: {
      /** A body sent as JSON, with the token */
      json?: unknown;
      /** A body sent as it is, without the token */
      bytes?: Uint8Array<ArrayBuffer>;
      headers?: Record<string, string>;
    } = {},
  ): Promise<unknown> {
    for (let attempt = 0; ; attempt++) {
      const auth: Record<string, string> = bytes
        ? {}
        : { authorization: `Bearer ${await token()}` };
      const init: FetchInit = { method, headers: { ...auth, ...headers } };
      if (bytes) {
        init.body = bytes;
      } else if (json !== undefined) {
        init.body = JSON.stringify(json);
        init.headers['content-type'] = 'application/json';
      }
      const response = await fetchImpl(url, init);
      if ((response.status === 429 || response.status >= 500) && attempt < retries) {
        const after = Number(response.headers.get('retry-after'));
        const pause =
          Number.isFinite(after) && after > 0
            ? after * 1000
            : Math.min(60_000 * 2 ** attempt, 15 * 60_000);
        log(`${method} ${url}: ${response.status}, again in ${Math.round(pause / 1000)} s`);
        await wait(pause);
        continue;
      }
      const text = await response.text();
      let parsed: unknown = text;
      try {
        parsed = text ? JSON.parse(text) : null;
      } catch {
        // Not JSON: kept as text for the error
      }
      if (!response.ok) throw new HttpError(method, url, response.status, parsed);
      return parsed;
    }
  }
  const at = (path: string) => (path.startsWith('https://') ? path : `${base}${path}`);
  return {
    get: (path) => send('GET', at(path)),
    post: (path, json) => send('POST', at(path), { json }),
    put: (path, json) => send('PUT', at(path), { json }),
    patch: (path, json) => send('PATCH', at(path), { json }),
    delete: (path) => send('DELETE', at(path)),
    upload: (method, url, bytes, headers) => send(method, url, { bytes, headers }),
  };
}

/**
 * `api` for a dry run: reads go to the store, writes are only written down in `writes` and
 * answered with what the store would answer, with `dry-run-<n>` ids. Reads below such an id are
 * answered empty, the store doesn't know it. `answer(method, path, body, id)` shapes the answers.
 */
export function dryRun(
  api: Api,
  {
    answer,
    empty,
  }: {
    answer: (method: string, path: string, body: unknown, id: string) => unknown;
    empty: (path: string) => unknown;
  },
): Recording {
  const writes: Write[] = [];
  let n = 0;
  const write = (method: string) => async (path: string, body?: unknown) => {
    writes.push({ method, path, body });
    return answer(method, path, body, `dry-run-${++n}`);
  };
  return {
    writes,
    get: async (path) => (path.includes('dry-run-') ? empty(path) : api.get(path)),
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
export function counting(api: Api): Recording {
  const writes: Write[] = [];
  const wrap =
    (method: string, send: (path: string, body: unknown) => Promise<unknown>) =>
    async (path: string, body?: unknown) => {
      writes.push({ method, path, body });
      return send(path, body);
    };
  return {
    writes,
    get: (path) => api.get(path),
    post: wrap('POST', (path, body) => api.post(path, body)),
    put: wrap('PUT', (path, body) => api.put(path, body)),
    patch: wrap('PATCH', (path, body) => api.patch(path, body)),
    delete: wrap('DELETE', (path) => api.delete(path)),
    upload: async (method, url, bytes, headers) => {
      writes.push({ method, path: url, body: `${bytes.length} bytes` });
      return api.upload(method, url, bytes, headers);
    },
  };
}

const base64url = (data: string | Uint8Array) => Buffer.from(data).toString('base64url');

/** A signed JWT: `alg` ES256 (App Store Connect) or RS256 (Google's service accounts). */
export function jwt({
  alg,
  key,
  kid,
  payload,
}: {
  alg: 'ES256' | 'RS256';
  key: crypto.KeyObject | string;
  kid?: string;
  payload: Record<string, unknown>;
}): string {
  const header = { alg, typ: 'JWT', ...(kid ? { kid } : {}) };
  const data = `${base64url(JSON.stringify(header))}.${base64url(JSON.stringify(payload))}`;
  const privateKey = typeof key === 'string' ? crypto.createPrivateKey(key) : key;
  const signature =
    alg === 'ES256'
      ? crypto.sign('sha256', Buffer.from(data), { key: privateKey, dsaEncoding: 'ieee-p1363' })
      : crypto.sign('sha256', Buffer.from(data), privateKey);
  return `${data}.${base64url(signature)}`;
}

/** The write-ups of a sync's changes, one line each, for the log and the job summary. */
export function listWrites(writes: readonly Write[]): string[] {
  return writes.map((w) => `${w.method} ${w.path.replace(/^https:\/\/[^/]+/, '')}`);
}

/** The writes by kind, `{ 'PATCH /v2/gameCenterAchievements/{id}': 3 }`; bytes sent are uploads. */
export function tally(writes: readonly Write[]): Record<string, number> {
  const counts: Record<string, number> = {};
  for (const w of writes) {
    const key = w.path.startsWith('https://')
      ? `${w.method} (image upload)`
      : `${w.method} ${w.path.replace(/\?.*$/, '').replace(/\/(?!v\d\/)[^/]*\d[^/]*/g, '/{id}')}`;
    counts[key] = (counts[key] ?? 0) + 1;
  }
  return counts;
}
