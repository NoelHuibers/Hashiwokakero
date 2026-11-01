// `cargo leptos watch` with a lid on it: fewer parallel compiler jobs, and, where systemd-run is
// there (Linux with systemd), a memory limit for the whole build, so a big compile fails
// instead of pushing the desktop into swap.
//
//   pnpm run dev                             http://127.0.0.1:3000
//   HASHI_LOCALES=en,ja pnpm run dev         only these languages translated, a lighter build
//   pnpm run dev --hot-reload                arguments go to `cargo leptos watch`
//
// CARGO_BUILD_JOBS (default 4) and HASHI_DEV_MEMORY (default 10G, systemd's MemoryMax syntax,
// "infinity" for none) override the limits. A first build of everything peaked at about 5 GB
// with 2 jobs, the two builds of the website crate (server and browser) run side by side.

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const root = path.resolve(import.meta.dirname, '..');
const env = { ...process.env };
env.CARGO_BUILD_JOBS ??= '4';
const memory = env.HASHI_DEV_MEMORY ?? '10G';

// rustup's default install dir, often not on the PATH of a shell pnpm starts
const cargoBin = path.join(env.CARGO_HOME ?? path.join(os.homedir(), '.cargo'), 'bin');
if (fs.existsSync(cargoBin) && !(env.PATH ?? '').split(path.delimiter).includes(cargoBin)) {
  env.PATH = `${cargoBin}${path.delimiter}${env.PATH ?? ''}`;
}

const watch = ['cargo', 'leptos', 'watch', ...process.argv.slice(2)];
const scope =
  spawnSync('systemd-run', ['--user', '--scope', '-q', 'true'], { stdio: 'ignore' }).status === 0;
const command = scope
  ? [
      'systemd-run',
      '--user',
      '--scope',
      '-q',
      '-p',
      `MemoryMax=${memory}`,
      '-p',
      'MemorySwapMax=0',
      '--',
      ...watch,
    ]
  : watch;

console.log(
  `jobs ${env.CARGO_BUILD_JOBS}, memory ${scope ? memory : 'not limited (no systemd-run)'}, ` +
    `languages ${env.HASHI_LOCALES ? `${env.HASHI_LOCALES} (HASHI_LOCALES)` : 'all'}`,
);
const run = spawnSync(command[0], command.slice(1), { cwd: root, env, stdio: 'inherit' });
if (run.error) {
  console.error(`${command[0]}: ${run.error.message}`);
  process.exit(1);
}
process.exit(run.status ?? 1);
