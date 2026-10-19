// The browser tests in the official Playwright container, the image the CI job runs in: the
// only place the screenshots of e2e/tests/visual.spec.ts match, fonts and rendering differ
// from one system to the next.
//
//   npm run e2e:container                      every test, the screenshots included
//   npm run e2e:container -- -g "practice"     arguments go to `playwright test`
//   npm run e2e:update-snapshots               take the screenshots again, after a change on purpose
//
// Needs podman or docker, and the build the tests run on (`cargo leptos build --release`, or
// E2E_BIN and E2E_SITE_ROOT, see the README). The image's tag is the version of @playwright/test
// in node_modules, so it changes with it.

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

const root = path.resolve(import.meta.dirname, '..');
const version = JSON.parse(
  fs.readFileSync(path.join(root, 'node_modules/@playwright/test/package.json'), 'utf8'),
).version;
const image = `mcr.microsoft.com/playwright:v${version}-noble`;

const engine = ['podman', 'docker'].find((cmd) => spawnSync(cmd, ['--version']).status === 0);
if (!engine) {
  console.error('e2e:container needs podman or docker');
  process.exit(1);
}

// The repository is /work in the container. A build elsewhere (E2E_BIN, E2E_SITE_ROOT) is mounted
// next to it, the binary with its directory: hash.txt lies next to it.
const mounts = ['-v', `${root}:/work`];
const env = { E2E_VISUAL: '1', E2E_PORT: process.env.E2E_PORT, CI: process.env.CI };
const outside = (variable, mountPoint, { file }) => {
  const value = process.env[variable];
  if (!value) return;
  const full = path.resolve(root, value);
  const relative = path.relative(root, full);
  if (!relative.startsWith('..')) {
    env[variable] = relative;
    return;
  }
  const dir = file ? path.dirname(full) : full;
  mounts.push('-v', `${dir}:${mountPoint}:ro`);
  env[variable] = file ? path.posix.join(mountPoint, path.basename(full)) : mountPoint;
};
outside('E2E_BIN', '/e2e/bin', { file: true });
outside('E2E_SITE_ROOT', '/e2e/site', { file: false });

const args = [
  'run',
  '--rm',
  // The server and the browser share the host's network (rootless podman too), the tests reach
  // the server on 127.0.0.1
  '--network=host',
  // Chromium needs more shared memory than a container's default
  '--ipc=host',
  // Fedora's SELinux would keep the container out of the repository
  '--security-opt=label=disable',
  ...mounts,
  '-w',
  '/work',
  ...Object.entries(env).flatMap(([k, v]) => (v === undefined ? [] : ['-e', `${k}=${v}`])),
  image,
  'npx',
  'playwright',
  'test',
  '-c',
  'e2e',
  ...process.argv.slice(2),
];
console.log(`${engine} ${args.join(' ')}`);
const run = spawnSync(engine, args, { stdio: 'inherit' });
process.exit(run.status ?? 1);
