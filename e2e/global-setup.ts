// A fresh, seeded database and a server on it for every run, stopped again at the end. The
// screenshot tests get a second one of their own.
import { mkdirSync, readdirSync, readFileSync, rmSync } from 'node:fs';
import path from 'node:path';

import { DB_PATH, RUN_DIR, VISUAL, VISUAL_DB_PATH, VISUAL_PORT } from './env';
import { seed } from './seed';
import { startServer } from './server';

async function seededServer(name: string, options: { port?: number; dbPath: string }) {
  // A first start only creates the schema, the seed goes into it
  await (await startServer(`${name}-schema.log`, { ...options, admins: '' })).stop();
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
  return async () => {
    await Promise.all(servers.map((s) => s.stop()));
    failOnPanics();
  };
}

// A panic in one of the server's background tasks leaves the page as it was, so no test fails
// for it: the run does, with the panics from the servers' logs.
function failOnPanics() {
  const panics = readdirSync(RUN_DIR)
    .filter((name) => name.endsWith('.log'))
    .flatMap((name) => {
      const lines = readFileSync(path.join(RUN_DIR, name), 'utf8').split('\n');
      // The line after "thread ... panicked at <file>" is the message
      return lines.flatMap((line, i) =>
        line.includes(' panicked at ') ? [`${name}: ${line}\n  ${lines[i + 1] ?? ''}`] : [],
      );
    });
  if (panics.length > 0) {
    throw new Error(`the server panicked ${panics.length} times (logs in e2e/.run/):\n${panics.join('\n')}`);
  }
}
