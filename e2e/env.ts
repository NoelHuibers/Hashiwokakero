// Paths and settings shared by the config, the global setup and the tests.
import path from 'node:path';

export const ROOT = path.resolve(import.meta.dirname, '..');
// The database and the server's log of a run, recreated every run
export const RUN_DIR = path.join(import.meta.dirname, '.run');
export const DB_PATH = path.join(RUN_DIR, 'hashi.db');

export const PORT = Number(process.env.E2E_PORT ?? 3150);
export const BASE_URL = `http://127.0.0.1:${PORT}`;

// The screenshot tests (`visual` project), only where their screenshots were taken: in the
// Playwright container (`pnpm run e2e:container`, the CI job). Their own server and database on
// the next port, so the journeys running next to them don't change the pages they take.
export const VISUAL = !!process.env.E2E_VISUAL;
export const VISUAL_PORT = PORT + 1;
export const VISUAL_URL = `http://127.0.0.1:${VISUAL_PORT}`;
export const VISUAL_DB_PATH = path.join(RUN_DIR, 'visual.db');

// A `cargo leptos build --release` by default. CI runs the binary and site of its build job
// (E2E_BIN=dist/hashi E2E_SITE_ROOT=dist/site); hash.txt has to lie next to the binary.
export const SERVER_BIN = path.resolve(ROOT, process.env.E2E_BIN ?? 'target/release/hashi');
export const SITE_ROOT = path.resolve(ROOT, process.env.E2E_SITE_ROOT ?? 'target/site');
