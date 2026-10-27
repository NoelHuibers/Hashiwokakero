// Where the store screenshots' server runs and where the images go.
import path from 'node:path';

import { ROOT } from '../../e2e/env';

export { ROOT };
/** Away from the browser tests' ports (e2e/env.ts: 3150 and up). */
export const STORE_PORT = Number(process.env.STORE_PORT ?? 3190);
export const STORE_URL = `http://127.0.0.1:${STORE_PORT}`;
export const STORE_DIR = path.join(ROOT, 'target/store');
export const RUN_DIR = path.join(STORE_DIR, 'run');
export const DB_PATH = path.join(RUN_DIR, 'store.db');
/** The screenshots of the bare pages, per language and device. */
export const RAW_DIR = path.join(STORE_DIR, 'raw');
/** The framed images, per store, language and device (store/assemble.mjs sorts them in). */
export const IMAGES_DIR = path.join(STORE_DIR, 'images');
/** Only these site languages, e.g. `STORE_LANGS=en,de,ja`: a quick look while editing. */
export const ONLY_LANGS = process.env.STORE_LANGS?.split(',').filter(Boolean);
