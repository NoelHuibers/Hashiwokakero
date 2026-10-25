// The logic of scripts/e2e-container.mjs that needs no container: `node --test scripts/tests/*.test.mjs`.
import assert from 'node:assert/strict';
import net from 'node:net';
import { test } from 'node:test';

import { browserServerArgs, freePort, imageFor, waitForPort, wsEndpoint } from '../e2e-container.mjs';

const never = new Promise(() => {});

test('the image is the Playwright version of node_modules', () => {
  assert.equal(imageFor('1.64.1'), 'mcr.microsoft.com/playwright:v1.64.1-noble');
});

test('the browser server listens on the host loopback, where the tests connect', () => {
  const args = browserServerArgs({ image: 'img', root: '/repo', port: 4321, name: 'browser' });
  // Rootless podman: 127.0.0.1, not localhost, on the host's network
  assert.ok(args.includes('--network=host'));
  assert.deepEqual(args.slice(args.indexOf('run-server')), ['run-server', '--host', '127.0.0.1', '--port', '4321']);
  assert.equal(wsEndpoint(4321), 'ws://127.0.0.1:4321/');
  // Removed when it stops, stoppable by name
  assert.ok(args.includes('--rm'));
  assert.equal(args[args.indexOf('--name') + 1], 'browser');
  // The repository only for node_modules, nothing written to it
  assert.equal(args[args.indexOf('-v') + 1], '/repo:/work:ro');
  assert.ok(args.indexOf('img') < args.indexOf('node_modules/.bin/playwright'));
});

test('a free port can be listened on', async () => {
  const port = await freePort();
  assert.ok(port > 0);
  const server = net.createServer();
  await new Promise((resolve, reject) => server.once('error', reject).listen(port, '127.0.0.1', resolve));
  server.close();
});

test('waiting for the port ends once something listens', async () => {
  const port = await freePort();
  const server = net.createServer();
  setTimeout(() => server.listen(port, '127.0.0.1'), 200);
  try {
    await waitForPort(port, { exited: never, timeout: 5_000, interval: 20 });
  } finally {
    server.close();
  }
});

test('waiting for the port fails when the container exits', async () => {
  const port = await freePort();
  await assert.rejects(waitForPort(port, { exited: Promise.resolve(125), timeout: 5_000, interval: 20 }), /exited/);
});

test('waiting for the port gives up after the timeout', async () => {
  const port = await freePort();
  await assert.rejects(waitForPort(port, { exited: never, timeout: 200, interval: 20 }), /within/);
});
