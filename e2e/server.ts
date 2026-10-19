// Starts the built server binary on a fresh local database. It needs no secrets: without
// TURSO_DATABASE_URL it uses the SQLite file in HASHI_DB_PATH, the other secrets have local
// defaults, and without DOJO_URL nothing is sent to a Dojo worker.
import { spawn, type ChildProcess } from 'node:child_process';
import { existsSync, openSync, readFileSync } from 'node:fs';
import path from 'node:path';

import { BASE_URL, DB_PATH, PORT, ROOT, RUN_DIR, SERVER_BIN, SITE_ROOT } from './env';

/** The usernames that see /admin (see `seed.ts`). */
export const ADMINS = 'admin';

export interface Server {
  stop(): Promise<void>;
}

export async function startServer(logName: string): Promise<Server> {
  if (!existsSync(SERVER_BIN)) {
    throw new Error(`no server binary at ${SERVER_BIN}: run \`cargo leptos build --release\` or set E2E_BIN`);
  }
  // Otherwise the tests would quietly run against that other server
  if (await answers()) throw new Error(`something already listens on ${BASE_URL}, set E2E_PORT`);
  const log = path.join(RUN_DIR, logName);
  const out = openSync(log, 'a');
  const child = spawn(SERVER_BIN, [], {
    cwd: ROOT,
    stdio: ['ignore', out, out],
    env: {
      ...process.env,
      LEPTOS_HASH_FILES: 'true',
      LEPTOS_SITE_ADDR: `127.0.0.1:${PORT}`,
      LEPTOS_SITE_ROOT: SITE_ROOT,
      LEPTOS_OUTPUT_NAME: 'hashi',
      LEPTOS_SITE_PKG_DIR: 'pkg',
      HASHI_DB_PATH: DB_PATH,
      HASHI_ADMINS: ADMINS,
      // Never a remote database or worker, whatever the shell has set
      TURSO_DATABASE_URL: '',
      DOJO_URL: '',
      DOJO_SEASON: '1',
    },
  });
  const exited = new Promise<number | null>((resolve) => child.on('exit', (code) => resolve(code)));
  try {
    await waitUntilUp(exited);
  } catch (e) {
    child.kill('SIGKILL');
    throw new Error(`${(e as Error).message}\n--- ${log} ---\n${readFileSync(log, 'utf8').slice(-4000)}`);
  }
  return { stop: () => stop(child, exited) };
}

async function waitUntilUp(exited: Promise<number | null>) {
  let gone = false;
  void exited.then(() => (gone = true));
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    if (gone) throw new Error('the server exited during startup');
    // Listening means migrated, the server creates the schema before it binds
    if (await answers()) return;
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error(`the server didn't answer on ${BASE_URL} within 30 s`);
}

const answers = () => fetch(`${BASE_URL}/favicon.svg`).then((r) => r.ok, () => false);

async function stop(child: ChildProcess, exited: Promise<number | null>) {
  if (child.exitCode !== null) return;
  child.kill('SIGTERM');
  const timeout = setTimeout(() => child.kill('SIGKILL'), 5_000);
  await exited;
  clearTimeout(timeout);
}
