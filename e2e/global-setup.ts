// A fresh, seeded database and a server on it for every run, stopped again at the end.
import { mkdirSync, rmSync } from 'node:fs';

import { DB_PATH, RUN_DIR } from './env';
import { seed } from './seed';
import { startServer } from './server';

export default async function globalSetup() {
  rmSync(RUN_DIR, { recursive: true, force: true });
  mkdirSync(RUN_DIR, { recursive: true });
  // A first start only creates the schema, the seed goes into it
  await (await startServer('schema.log')).stop();
  seed(DB_PATH);
  const server = await startServer('server.log');
  return () => server.stop();
}
