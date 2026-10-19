// A fresh, seeded database and a server on it for every run, stopped again at the end. The
// screenshot tests get a second one of their own.
import { mkdirSync, rmSync } from 'node:fs';

import { DB_PATH, RUN_DIR, VISUAL, VISUAL_DB_PATH, VISUAL_PORT } from './env';
import { seed } from './seed';
import { startServer } from './server';

async function seededServer(name: string, options: { port?: number; dbPath: string }) {
  // A first start only creates the schema, the seed goes into it
  await (await startServer(`${name}-schema.log`, options)).stop();
  seed(options.dbPath);
  return startServer(`${name}.log`, options);
}

export default async function globalSetup() {
  rmSync(RUN_DIR, { recursive: true, force: true });
  mkdirSync(RUN_DIR, { recursive: true });
  const servers = await Promise.all([
    seededServer('server', { dbPath: DB_PATH }),
    ...(VISUAL ? [seededServer('visual', { port: VISUAL_PORT, dbPath: VISUAL_DB_PATH })] : []),
  ]);
  return () => Promise.all(servers.map((s) => s.stop()));
}
