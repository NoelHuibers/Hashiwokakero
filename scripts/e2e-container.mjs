// The browser tests with the browser of the official Playwright image, the one the CI job runs
// in: the only browser whose screenshots match those of e2e/tests/visual.spec.ts, fonts and
// rendering differ from one system to the next.
//
//   pnpm run e2e:container                     every test, the screenshots included
//   pnpm run e2e:container -g "practice"        arguments go to `playwright test`
//   pnpm run e2e:update-snapshots              take the screenshots again, after a change on purpose
//
// Only the browser runs in the container (`playwright run-server`), the tests and the servers
// they start run here and connect to it (PW_TEST_CONNECT_WS_ENDPOINT). The screenshots are the
// container's either way: the browser renders and takes them. The server binary needs no
// container: one built here links against this system's glibc, often newer than the image's,
// and wouldn't start in it.
//
// Needs podman or docker, and the build the tests run on (`cargo leptos build --release`, or
// E2E_BIN and E2E_SITE_ROOT, see the README). The image's tag is the version of @playwright/test
// in node_modules, so it changes with it. E2E_BROWSER_PORT sets the browser server's port (a
// free one by default).

import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import net from 'node:net';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(import.meta.dirname, '..');

/** The Playwright image of a version of @playwright/test: browsers, fonts and all. */
export const imageFor = (version) => `mcr.microsoft.com/playwright:v${version}-noble`;

/** Where the tests find the browser server. */
export const wsEndpoint = (port) => `ws://127.0.0.1:${port}/`;

/** A port nothing listens on yet, on 127.0.0.1. */
export function freePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      server.close(() => resolve(port));
    });
  });
}

/**
 * The arguments of `podman run` (or `docker run`) for the browser server. It shares the host's
 * network (rootless podman too): it listens on the host's 127.0.0.1, and the browser reaches the
 * test servers there.
 */
export function browserServerArgs({ image, root, port, name }) {
  return [
    'run',
    '--rm',
    '--init',
    '--name',
    name,
    '--network=host',
    // Chromium needs more shared memory than a container's default
    '--ipc=host',
    // Fedora's SELinux would keep the container out of the repository
    '--security-opt=label=disable',
    // Only for node_modules: the image has the browsers, not the Playwright of this repository
    '-v',
    `${root}:/work:ro`,
    '-w',
    '/work',
    image,
    // The image has no pnpm, the bin pnpm links runs with the image's node
    'node_modules/.bin/playwright',
    'run-server',
    '--host',
    '127.0.0.1',
    '--port',
    String(port),
  ];
}

/**
 * Resolves once something accepts connections on 127.0.0.1:`port`. Rejects when `exited`
 * settles first (the container is gone) or after `timeout` ms.
 */
export async function waitForPort(port, { exited, timeout = 60_000, interval = 100 }) {
  let gone = false;
  void exited.then(
    () => (gone = true),
    () => (gone = true),
  );
  const deadline = Date.now() + timeout;
  const accepts = () =>
    new Promise((resolve) => {
      const socket = net.connect(port, '127.0.0.1');
      socket.once('connect', () => {
        socket.destroy();
        resolve(true);
      });
      socket.once('error', () => resolve(false));
    });
  while (Date.now() < deadline) {
    if (gone) throw new Error('the browser container exited before it listened');
    if (await accepts()) return;
    await new Promise((r) => setTimeout(r, interval));
  }
  throw new Error(`the browser server didn't listen on 127.0.0.1:${port} within ${timeout / 1000} s`);
}

async function main() {
  const version = JSON.parse(
    fs.readFileSync(path.join(root, 'node_modules/@playwright/test/package.json'), 'utf8'),
  ).version;
  const engine = ['podman', 'docker'].find((cmd) => spawnSync(cmd, ['--version']).status === 0);
  if (!engine) {
    console.error('e2e:container needs podman or docker');
    process.exit(1);
  }

  const port = Number(process.env.E2E_BROWSER_PORT ?? (await freePort()));
  const name = `hashi-e2e-browser-${port}`;
  const args = browserServerArgs({ image: imageFor(version), root, port, name });
  console.log(`${engine} ${args.join(' ')}`);
  const container = spawn(engine, args, { stdio: ['ignore', 'inherit', 'inherit'] });
  const exited = new Promise((resolve, reject) => {
    container.once('exit', resolve);
    container.once('error', reject);
  });
  let removed = false;
  const remove = () => {
    if (removed) return;
    removed = true;
    spawnSync(engine, ['stop', '-t', '2', name], { stdio: 'ignore' });
  };
  // Ctrl-C reaches the tests and the container's client too, the container itself only this way
  for (const signal of ['SIGINT', 'SIGTERM']) {
    process.once(signal, () => {
      remove();
      process.exit(130);
    });
  }

  let status = 1;
  try {
    await waitForPort(port, { exited });
    const tests = spawn(path.join(root, 'node_modules/.bin/playwright'), ['test', '-c', 'e2e', ...process.argv.slice(2)], {
      cwd: root,
      stdio: 'inherit',
      env: { ...process.env, E2E_VISUAL: '1', PW_TEST_CONNECT_WS_ENDPOINT: wsEndpoint(port) },
    });
    status = await new Promise((resolve) => tests.once('exit', (code) => resolve(code ?? 1)));
  } catch (e) {
    console.error(`e2e:container: ${e.message}`);
  } finally {
    remove();
  }
  process.exit(status);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
