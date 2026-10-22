// Serves the app's bundle like the app's WebView will: the files of the dist, index.html for
// every other path (the router takes it from there), and /api/... passed on to the server, so
// the server functions are same-origin calls.
import { createReadStream, existsSync, statSync } from 'node:fs';
import { createServer, request, type Server } from 'node:http';
import path from 'node:path';

import { PORT, ROOT } from '../env';

/** `pnpm run app:build` writes it, CI unpacks the artifact of the "Build app (web)" job there. */
export const APP_DIST = path.resolve(ROOT, process.env.E2E_APP_DIST ?? 'target/app-dist');
export const APP_PORT = PORT + 2;
export const APP_URL = `http://127.0.0.1:${APP_PORT}`;

const TYPES: Record<string, string> = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript',
  '.css': 'text/css',
  // Needed by WebAssembly.instantiateStreaming
  '.wasm': 'application/wasm',
  '.json': 'application/json',
  '.webmanifest': 'application/manifest+json',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.ico': 'image/x-icon',
  '.woff2': 'font/woff2',
  '.txt': 'text/plain',
};

export function serveApp(api: string): Promise<Server> {
  if (!existsSync(path.join(APP_DIST, 'index.html'))) {
    throw new Error(`no app bundle in ${APP_DIST}: run \`pnpm run app:build\` or set E2E_APP_DIST`);
  }
  const target = new URL(api);
  const server = createServer((req, res) => {
    const url = new URL(req.url ?? '/', APP_URL);
    if (url.pathname.startsWith('/api/')) {
      const headers = { ...req.headers, host: target.host };
      const upstream = request(
        { host: target.hostname, port: target.port, method: req.method, path: req.url, headers },
        (answer) => {
          res.writeHead(answer.statusCode ?? 502, answer.headers);
          answer.pipe(res);
        },
      );
      upstream.on('error', (e) => res.writeHead(502).end(String(e)));
      req.pipe(upstream);
      return;
    }
    let file = path.join(APP_DIST, path.normalize(decodeURIComponent(url.pathname)));
    if (!file.startsWith(APP_DIST) || !existsSync(file) || !statSync(file).isFile()) {
      file = path.join(APP_DIST, 'index.html');
    }
    res.writeHead(200, { 'content-type': TYPES[path.extname(file)] ?? 'application/octet-stream' });
    createReadStream(file).pipe(res);
  });
  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(APP_PORT, '127.0.0.1', () => resolve(server));
  });
}
