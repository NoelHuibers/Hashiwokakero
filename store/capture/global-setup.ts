// A fresh local database with the store's players (seed.ts) and the built site on it, as the
// browser tests start theirs (e2e/global-setup.ts). Never Turso: startServer clears
// TURSO_DATABASE_URL.
import { mkdirSync, rmSync } from 'node:fs';
import { DatabaseSync } from 'node:sqlite';
import path from 'node:path';

import { startServer } from '../../e2e/server';
import { DB_PATH, ROOT, RUN_DIR, STORE_PORT } from './env';
import { ME, seed } from './seed';

export default async function globalSetup() {
  rmSync(RUN_DIR, { recursive: true, force: true });
  mkdirSync(RUN_DIR, { recursive: true });
  // startServer writes its log to e2e/.run
  mkdirSync(path.join(ROOT, 'e2e/.run'), { recursive: true });
  const options = { port: STORE_PORT, dbPath: DB_PATH, admins: '' };
  // A first start only creates the schema, the seed goes into it
  await (await startServer('store-schema.log', options)).stop();
  seed(DB_PATH);
  const server = await startServer('store.log', options);
  await stampsTold();
  return () => server.stop();
}

/**
 * The server hands out the seeded players' stamps on startup (server/daily/stamps.rs,
 * sync_recent); a player's first page would then announce each of them in a toast, over the
 * screenshot. Marked as told once they are there.
 */
async function stampsTold() {
  const db = new DatabaseSync(DB_PATH, { timeout: 5_000 });
  try {
    const count = () =>
      Number(
        (
          db
            .prepare('SELECT COUNT(*) AS n FROM stamps WHERE user_id = (SELECT id FROM users WHERE username = ?)')
            .get(ME) as { n: number }
        ).n,
      );
    const deadline = Date.now() + 30_000;
    let last = -1;
    // Until the count holds still for a moment
    for (;;) {
      const n = count();
      if (n > 0 && n === last) break;
      if (Date.now() > deadline) throw new Error(`the server didn't hand out the stamps (${n})`);
      last = n;
      await new Promise((r) => setTimeout(r, 1_000));
    }
    db.prepare('UPDATE stamps SET announced = 1').run();
  } finally {
    db.close();
  }
}
