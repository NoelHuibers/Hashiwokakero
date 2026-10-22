// A server on a fresh database for the server functions, and the app's bundle in front of it.
import { mkdirSync, rmSync } from 'node:fs';
import path from 'node:path';

import { BASE_URL, RUN_DIR } from '../env';
import { startServer } from '../server';
import { serveApp } from './serve';

export default async function globalSetup() {
  mkdirSync(RUN_DIR, { recursive: true });
  const dbPath = path.join(RUN_DIR, 'app.db');
  for (const file of [dbPath, `${dbPath}-wal`, `${dbPath}-shm`]) rmSync(file, { force: true });
  const server = await startServer('app-server.log', { dbPath, admins: '' });
  const app = await serveApp(BASE_URL);
  return async () => {
    await new Promise((resolve) => app.close(resolve));
    await server.stop();
  };
}
