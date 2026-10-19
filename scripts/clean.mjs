// Frees disk space in cargo's target dirs, without touching anything a build still needs to be
// fast: by default it only lists what it would delete, with sizes.
//
//   npm run clean                         dry run: list what would go
//   npm run clean -- --yes                delete it
//   npm run clean -- --days 14            deps artifacts unused for 14 days (default 30)
//   npm run clean -- --target ~/other     another target dir (repeatable) instead of ./target
//   npm run clean -- --verbose            every incremental dir and deps file, not only sums
//   npm run clean -- --worktrees          also worktrees whose branch is merged into origin/main
//
// What goes, in every profile dir of a target dir (target/debug, target/front/wasm-release, ...):
// - fingerprint outputs (.fingerprint/*/output-*) of at least --min-output-mb (default 50 MB).
//   They hold the compiler's messages, replayed when a crate is fresh. A lint that echoed the
//   5 MB line of the generated i18n module made them tens of gigabytes. Without it a fresh crate
//   shows no warnings until it compiles again, nothing rebuilds.
// - incremental caches (incremental/<crate>-<hash>) except the newest of each crate. The others
//   belong to old builds, features or toolchains; a build that needs one compiles that crate
//   from scratch once.
// - artifacts in deps/ neither modified nor read for --days days (like cargo-sweep). Cargo
//   rebuilds what is missing, so this only costs a rebuild of things not used for that long.
// - with --worktrees: worktrees of this repository whose branch was merged into origin/main (after
//   a `git fetch`), with their target dirs. Never the main checkout, never one with uncommitted or
//   untracked changes, never one that is only at a commit of main without commits of its own.
//   `git worktree remove` deletes them; their branches stay.
//
// A target dir has to have cargo's CACHEDIR.TAG, the script deletes nothing outside of one
// (except the worktrees, through git).

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const root = path.resolve(import.meta.dirname, '..');

const usage =
  'usage: npm run clean -- [--yes] [--days N] [--min-output-mb N] [--target DIR]... ' +
  '[--worktrees] [--verbose]';
const opts = {
  yes: false,
  days: 30,
  minOutputMb: 50,
  targets: [],
  worktrees: false,
  verbose: false,
};
const argv = process.argv.slice(2);
for (let i = 0; i < argv.length; i++) {
  const arg = argv[i];
  const value = () => {
    if (i + 1 >= argv.length) fail(`${arg} needs a value\n${usage}`);
    return argv[++i];
  };
  const number = () => {
    const n = Number(value());
    if (!Number.isFinite(n) || n < 0) fail(`${arg} needs a number\n${usage}`);
    return n;
  };
  if (arg === '--yes' || arg === '-y') opts.yes = true;
  else if (arg === '--days') opts.days = number();
  else if (arg === '--min-output-mb') opts.minOutputMb = number();
  else if (arg === '--target')
    opts.targets.push(path.resolve(value().replace(/^~(?=$|\/)/, os.homedir())));
  else if (arg === '--worktrees') opts.worktrees = true;
  else if (arg === '--verbose' || arg === '-v') opts.verbose = true;
  else if (arg === '--help' || arg === '-h') {
    console.log(usage);
    process.exit(0);
  } else fail(`unknown argument ${arg}\n${usage}`);
}
if (opts.targets.length === 0) {
  opts.targets.push(
    process.env.CARGO_TARGET_DIR
      ? path.resolve(root, process.env.CARGO_TARGET_DIR)
      : path.join(root, 'target'),
  );
}

function fail(message) {
  console.error(message);
  process.exit(1);
}

const GB = 1024 ** 3;
const human = (bytes) =>
  bytes >= GB
    ? `${(bytes / GB).toFixed(1)} GB`
    : bytes >= 1024 ** 2
      ? `${(bytes / 1024 ** 2).toFixed(0)} MB`
      : `${(bytes / 1024).toFixed(0)} KB`;

// Disk usage like du: allocated blocks, not the apparent size. Never follows symlinks.
function usage_of(p) {
  let st;
  try {
    st = fs.lstatSync(p);
  } catch {
    return 0;
  }
  let total = st.blocks * 512;
  if (st.isDirectory()) {
    for (const entry of fs.readdirSync(p)) total += usage_of(path.join(p, entry));
  }
  return total;
}

const entries = (dir) => {
  try {
    return fs.readdirSync(dir, { withFileTypes: true });
  } catch {
    return [];
  }
};

// A target dir cargo made: it writes this tag into every one
function isTargetDir(dir) {
  try {
    return fs
      .readFileSync(path.join(dir, 'CACHEDIR.TAG'), 'utf8')
      .startsWith('Signature: 8a477f597d28d172789f06886806bc55');
  } catch {
    return false;
  }
}

// The dirs with a .fingerprint, at most a few levels down (target/debug,
// target/wasm32-unknown-unknown/debug, target/front/wasm32-unknown-unknown/debug, ...)
function profileDirs(dir, depth = 0) {
  const children = entries(dir).filter((e) => e.isDirectory());
  if (children.some((e) => e.name === '.fingerprint')) return [dir];
  if (depth >= 4) return [];
  return children.flatMap((e) => profileDirs(path.join(dir, e.name), depth + 1));
}

const candidates = []; // { kind, path, bytes, label, target }

function scanTarget(target) {
  const now = Date.now();
  const cutoff = now - opts.days * 24 * 3600 * 1000;
  for (const profile of profileDirs(target)) {
    const where = path.relative(target, profile) || '.';

    for (const unit of entries(path.join(profile, '.fingerprint'))) {
      if (!unit.isDirectory()) continue;
      const unitDir = path.join(profile, '.fingerprint', unit.name);
      for (const file of entries(unitDir)) {
        if (!file.isFile() || !file.name.startsWith('output-')) continue;
        const p = path.join(unitDir, file.name);
        const bytes = usage_of(p);
        if (bytes >= opts.minOutputMb * 1024 ** 2) {
          candidates.push({
            kind: 'fingerprint output',
            path: p,
            bytes,
            target,
            label: `${where}/.fingerprint/${unit.name}/${file.name}`,
          });
        }
      }
    }

    // incremental/<crate>-<hash>: keep the newest of each crate
    const byCrate = new Map();
    for (const e of entries(path.join(profile, 'incremental'))) {
      if (!e.isDirectory()) continue;
      const dash = e.name.lastIndexOf('-');
      if (dash <= 0) continue;
      const p = path.join(profile, 'incremental', e.name);
      // The dir's own mtime changes when rustc adds or removes a session in it
      const mtime = Math.max(
        fs.statSync(p).mtimeMs,
        ...entries(p).map((s) => fs.lstatSync(path.join(p, s.name)).mtimeMs),
      );
      const crate = e.name.slice(0, dash);
      if (!byCrate.has(crate)) byCrate.set(crate, []);
      byCrate.get(crate).push({ p, mtime, name: e.name });
    }
    for (const [crate, dirs] of byCrate) {
      dirs.sort((a, b) => b.mtime - a.mtime);
      for (const d of dirs.slice(1)) {
        candidates.push({
          kind: 'incremental',
          path: d.p,
          bytes: usage_of(d.p),
          target,
          label: `${where}/incremental/${d.name}`,
          group: `${where}/incremental/${crate}`,
        });
      }
    }

    // deps/: neither written nor read since the cut-off
    for (const e of entries(path.join(profile, 'deps'))) {
      const p = path.join(profile, 'deps', e.name);
      const st = fs.lstatSync(p);
      if (Math.max(st.mtimeMs, st.atimeMs) >= cutoff) continue;
      candidates.push({
        kind: `deps unused for ${opts.days} days`,
        path: p,
        bytes: usage_of(p),
        target,
        label: `${where}/deps/${e.name}`,
        group: `${where}/deps`,
      });
    }
  }
}

for (const target of opts.targets) {
  if (!fs.existsSync(target)) {
    console.log(`${target}: not there, skipped`);
    continue;
  }
  // Never the source tree itself, or anything above it
  if (!path.relative(target, root).startsWith('..') || !isTargetDir(target)) {
    console.log(`${target}: not a cargo target dir (no CACHEDIR.TAG), skipped`);
    continue;
  }
  scanTarget(target);
}

// Worktrees whose branch is merged into origin/main
const git = (args, cwd = root) => spawnSync('git', args, { cwd, encoding: 'utf8' });
const worktrees = [];
const keptWorktrees = [];
if (opts.worktrees) {
  if (git(['fetch', '--quiet', 'origin', 'main']).status !== 0)
    console.log('git fetch origin main failed, using the local origin/main');
  const main = git(['rev-parse', 'origin/main']).stdout.trim();
  // A branch merged with a merge commit is off this line; one at a commit on it has no
  // commits of its own (a fresh worktree), or was fast-forwarded, which isn't told apart
  const mainLine = new Set(git(['rev-list', '--first-parent', 'origin/main']).stdout.split('\n'));
  const list = git(['worktree', 'list', '--porcelain']).stdout.trim().split('\n\n');
  const mainCheckout = list[0]?.match(/^worktree (.*)$/m)?.[1];
  for (const block of list.slice(1)) {
    const dir = block.match(/^worktree (.*)$/m)?.[1];
    const head = block.match(/^HEAD (.*)$/m)?.[1];
    const branch = block.match(/^branch refs\/heads\/(.*)$/m)?.[1];
    const keep = (why) => keptWorktrees.push(`${dir} [${branch ?? 'detached'}]: ${why}`);
    if (!dir || dir === mainCheckout || path.resolve(dir) === root) continue;
    if (/^locked/m.test(block)) keep('locked');
    else if (/^prunable/m.test(block)) keep('missing, `git worktree prune` forgets it');
    else if (!branch) keep('detached HEAD');
    else if (git(['merge-base', '--is-ancestor', head, main]).status !== 0) keep('not merged');
    else if (mainLine.has(head)) keep('no commits of its own');
    else {
      const status = git(['status', '--porcelain'], dir);
      if (status.status !== 0) keep('git status failed');
      else if (status.stdout.trim()) keep('uncommitted changes');
      else
        worktrees.push({
          dir,
          branch,
          bytes: usage_of(dir),
          target: usage_of(path.join(dir, 'target')),
        });
    }
  }
}

// The listing
let total = 0;
const groups = new Map();
for (const c of candidates) {
  const key = `${c.target}\t${c.kind}\t${c.group ?? c.label}`;
  if (!groups.has(key)) groups.set(key, { ...c, count: 0, bytes: 0, items: [] });
  const g = groups.get(key);
  g.count++;
  g.bytes += c.bytes;
  g.items.push(c);
  total += c.bytes;
}
let lastTarget;
let lastKind;
for (const g of [...groups.values()].sort(
  (a, b) => a.target.localeCompare(b.target) || a.kind.localeCompare(b.kind) || b.bytes - a.bytes,
)) {
  if (g.target !== lastTarget) {
    console.log(`\n${g.target}`);
    lastTarget = g.target;
    lastKind = undefined;
  }
  if (g.kind !== lastKind) {
    const sum = candidates
      .filter((c) => c.target === g.target && c.kind === g.kind)
      .reduce((s, c) => s + c.bytes, 0);
    console.log(`  ${g.kind}: ${human(sum)}`);
    lastKind = g.kind;
  }
  const what = g.group
    ? `${g.group} (${g.count} ${g.kind === 'incremental' ? 'old dirs' : 'files'})`
    : g.label;
  console.log(`    ${human(g.bytes).padStart(9)}  ${what}`);
  if (opts.verbose && g.group)
    for (const c of g.items)
      console.log(`               ${human(c.bytes).padStart(9)}  ${c.label}`);
}
if (opts.worktrees) {
  console.log('\nworktrees merged into origin/main, without changes:');
  if (worktrees.length === 0) console.log('    none');
  for (const w of worktrees) {
    console.log(
      `    ${human(w.bytes).padStart(9)}  ${w.dir} [${w.branch}] (target ${human(w.target)})`,
    );
    total += w.bytes;
  }
  if (keptWorktrees.length) {
    console.log('kept:');
    for (const k of keptWorktrees) console.log(`               ${k}`);
  }
}
console.log(
  `\ntotal: ${human(total)}${opts.yes ? '' : ', nothing deleted (dry run, --yes deletes)'}`,
);
if (!opts.yes) process.exit(0);

// The deletion, each path checked against its target dir once more
let freed = 0;
for (const c of candidates) {
  const relative = path.relative(c.target, c.path);
  if (!relative || relative.startsWith('..') || path.isAbsolute(relative))
    fail(`refusing to delete ${c.path}: outside ${c.target}`);
  fs.rmSync(c.path, { recursive: true, force: true });
  freed += c.bytes;
}
for (const w of worktrees) {
  // Without --force git refuses a worktree that got changes in the meantime
  const removed = git(['worktree', 'remove', w.dir]);
  if (removed.status === 0) freed += w.bytes;
  else console.log(`kept ${w.dir}: ${removed.stderr.trim()}`);
}
console.log(`freed ${human(freed)}`);
