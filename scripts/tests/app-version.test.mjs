// node --test scripts/tests/: scripts/app-version.mjs on copies of the real files

import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { after, beforeEach, test } from 'node:test';
import { FILES, check, parse, read, set, versionCode } from '../app-version.mjs';

const root = path.resolve(import.meta.dirname, '../..');
const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'app-version-'));
after(() => fs.rmSync(scratch, { recursive: true, force: true }));

// A fresh copy of the files for every test
let dir;
let count = 0;
beforeEach(() => {
  dir = path.join(scratch, String(count++));
  for (const { file } of FILES) {
    fs.mkdirSync(path.dirname(path.join(dir, file)), { recursive: true });
    fs.copyFileSync(path.join(root, file), path.join(dir, file));
  }
});
const text = (file) => fs.readFileSync(path.join(dir, file), 'utf8');

test('the repository agrees on one version', () => {
  assert.match(check(root), /^\d+\.\d+\.\d+$/);
});

test('versionCode is major * 1,000,000 + minor * 1,000 + patch', () => {
  assert.equal(versionCode('0.1.0'), 1000);
  assert.equal(versionCode('1.2.3'), 1_002_003);
  assert.equal(versionCode('2.999.999'), 2_999_999);
});

test('a version is x.y.z with minor and patch below 1000', () => {
  assert.deepEqual(parse('10.0.7'), { major: 10, minor: 0, patch: 7 });
  for (const bad of ['1.2', '1.2.3.4', 'v1.2.3', '1.2.3-beta', '01.2.3', '1.02.3', '', ' 1.2.3']) {
    assert.throws(() => parse(bad), /not a version/, bad);
  }
  assert.throws(() => parse('1.1000.0'), /below 1000/);
  assert.throws(() => parse('1.0.1000'), /below 1000/);
  assert.throws(() => parse('2101.0.0'), /Play's limit/);
});

test('set changes every file, and only the app crates in the lock files', () => {
  // The app crates start with the version of another crate in Cargo.lock, which has to keep it
  const packages = /\[\[package\]\]\nname = "([^"]*)"\nversion = "([^"]*)"/g;
  const old = [...text('Cargo.lock').matchAll(packages)]
    .map(([, name, version]) => ({ name, version }))
    .find(({ name, version }) => name !== 'hashi_app' && /^[0-6]\.\d{1,3}\.\d{1,3}$/.test(version))
    .version;
  for (const { file, pattern } of FILES) {
    fs.writeFileSync(path.join(dir, file), text(file).replace(pattern, `$1${old}$3`));
  }
  assert.equal(check(dir), old);
  const before = Object.fromEntries(FILES.map(({ file }) => [file, text(file)]));
  set('7.8.9', dir);
  assert.equal(check(dir), '7.8.9');
  for (const file of new Set(FILES.map((entry) => entry.file))) {
    // One line per entry differs: the old version became the new one, nothing else moved
    const was = before[file].split('\n');
    const now = text(file).split('\n');
    assert.equal(now.length, was.length, file);
    const changed = now.filter((line, i) => line !== was[i]);
    assert.equal(changed.length, FILES.filter((entry) => entry.file === file).length, file);
    for (const line of changed) assert.match(line, /7\.8\.9/, file);
  }
  // The other crates with the old version keep it
  const olds = (lock) => lock.split('\n').filter((l) => l === `version = "${old}"`).length;
  assert.ok(olds(before['Cargo.lock']) > 1);
  assert.equal(olds(text('Cargo.lock')), olds(before['Cargo.lock']) - 1);
  assert.equal(JSON.parse(text('app/src-tauri/tauri.conf.json')).version, '7.8.9');
  // The iOS project as `tauri ios build` (Info.plist) and `tauri ios init` (project.yml) write it
  const plist = text('app/src-tauri/gen/apple/hashi_tauri_iOS/Info.plist');
  for (const key of ['CFBundleShortVersionString', 'CFBundleVersion']) {
    assert.ok(plist.includes(`<key>${key}</key>\n\t<string>7.8.9</string>`), key);
  }
  const yml = text('app/src-tauri/gen/apple/project.yml');
  assert.match(
    yml,
    /\n {8}CFBundleShortVersionString: 7\.8\.9\n {8}CFBundleVersion: "7\.8\.9"\n/,
  );
});

test('the iOS project has the version nowhere else', () => {
  // A key Tauri starts to write the version to would be missed by set and check
  const version = check(root);
  const ios = new Set(FILES.map((entry) => entry.file).filter((f) => f.includes('/gen/apple/')));
  for (const file of ios) {
    const count = text(file).split(version).length - 1;
    assert.equal(count, FILES.filter((entry) => entry.file === file).length, file);
  }
});

test('set refuses a versionCode that is not higher', () => {
  set('1.2.3', dir);
  for (const version of ['1.2.3', '1.2.2', '0.999.999']) {
    assert.throws(() => set(version, dir), /isn't above/, version);
  }
  assert.throws(() => set('1.2.1000', dir), /below 1000/);
  assert.equal(check(dir), '1.2.3');
  set('1.2.4', dir);
  assert.equal(check(dir), '1.2.4');
});

test('check fails when one file differs, and names it', () => {
  set('3.0.0', dir);
  const file = 'app/src-tauri/Cargo.lock';
  fs.writeFileSync(
    path.join(dir, file),
    text(file).replace(/(name = "hashi_tauri"\nversion = ")3\.0\.0/, '$13.0.1'),
  );
  assert.throws(() => check(dir), /differs[\s\S]*app\/src-tauri\/Cargo\.lock: 3\.0\.1/);
  // Setting a version above both repairs it
  assert.throws(() => set('3.0.1', dir), /isn't above 3\.0\.1/);
  set('3.0.2', dir);
  assert.equal(check(dir), '3.0.2');
});

test('check names the key of the iOS project that differs', () => {
  set('3.0.0', dir);
  const file = 'app/src-tauri/gen/apple/hashi_tauri_iOS/Info.plist';
  // What a build of another version would leave in the project
  fs.writeFileSync(
    path.join(dir, file),
    text(file).replace(/(<key>CFBundleVersion<\/key>\n\t<string>)3\.0\.0/, '$13.1.0'),
  );
  assert.throws(() => check(dir), /differs[\s\S]*Info\.plist \(CFBundleVersion\): 3\.1\.0/);
  // The other key of the file is still the version
  const short = read(dir).find((e) => e.file === file && e.key === 'CFBundleShortVersionString');
  assert.equal(short.version, '3.0.0');
});

test('read fails when a file has no version where expected', () => {
  const file = 'app/src-tauri/tauri.conf.json';
  fs.writeFileSync(path.join(dir, file), text(file).replace('"version"', '"versio"'));
  assert.throws(() => read(dir), /tauri\.conf\.json: the version is there 0 times/);
});
