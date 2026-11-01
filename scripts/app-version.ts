// The mobile app's version, the same in every file that has it (app/README.md, Releases):
//
//   node scripts/app-version.mjs           prints the version; fails when the files disagree
//   node scripts/app-version.mjs --code    prints Android's versionCode of it
//   node scripts/app-version.mjs 1.2.3     sets 1.2.3 everywhere
//
// The files: app/Cargo.toml (the version the app sends in x-hashi-app), app/src-tauri/Cargo.toml,
// app/src-tauri/tauri.conf.json (the store's versionName, and the versionCode Tauri derives from
// it), the two crates' entries in Cargo.lock and app/src-tauri/Cargo.lock, and the iOS project's
// CFBundleShortVersionString and CFBundleVersion in gen/apple: its Info.plist, which every
// `tauri ios build` rewrites with the version of tauri.conf.json, and project.yml, which
// `tauri ios init` writes with it. Only those entries change: other packages of the lock files
// have the same version, they are left alone.
//
// A new version has to be x.y.z with minor and patch below 1000, and its versionCode
// (major * 1,000,000 + minor * 1,000 + patch, as Tauri computes it) above the current one:
// Play refuses an upload whose versionCode isn't higher than every earlier one. CI ("Test & lint")
// runs the script without arguments, so the files can't drift apart.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

// The largest versionCode Play accepts
const MAX_CODE = 2_100_000_000;

// Where the version is, per file (and key, for a file that has it twice): a pattern whose groups
// are the text before the version, the version, and the text after it. Each matches exactly once.
const tomlPackage = (name) =>
  new RegExp(`(\\[package\\]\\nname = "${name}"\\nversion = ")([^"]*)(")`);
const lockPackage = (name) =>
  new RegExp(`(\\[\\[package\\]\\]\\nname = "${name}"\\nversion = ")([^"]*)(")`);
const plistString = (key) => new RegExp(`(<key>${key}</key>\\n\\t<string>)([^<]*)(</string>)`);
const infoPlist = 'app/src-tauri/gen/apple/hashi_tauri_iOS/Info.plist';
const projectYml = 'app/src-tauri/gen/apple/project.yml';
export const FILES = [
  { file: 'app/Cargo.toml', pattern: tomlPackage('hashi_app') },
  { file: 'app/src-tauri/Cargo.toml', pattern: tomlPackage('hashi_tauri') },
  // The top-level "version", indented by two spaces; nested objects are indented further
  { file: 'app/src-tauri/tauri.conf.json', pattern: /(\n {2}"version": ")([^"]*)(")/ },
  { file: 'Cargo.lock', pattern: lockPackage('hashi_app') },
  { file: 'app/src-tauri/Cargo.lock', pattern: lockPackage('hashi_tauri') },
  // The build writes both from the version (agvtool for CFBundleVersion); the release passes the
  // build number App Store Connect counts as bundle.iOS.bundleVersion instead (app/README.md)
  {
    file: infoPlist,
    key: 'CFBundleShortVersionString',
    pattern: plistString('CFBundleShortVersionString'),
  },
  { file: infoPlist, key: 'CFBundleVersion', pattern: plistString('CFBundleVersion') },
  // The target's info properties, indented by eight spaces
  {
    file: projectYml,
    key: 'CFBundleShortVersionString',
    pattern: /(\n {8}CFBundleShortVersionString: )([^\n]*)(\n)/,
  },
  { file: projectYml, key: 'CFBundleVersion', pattern: /(\n {8}CFBundleVersion: ")([^"]*)(")/ },
];

/** A file, and the key when it has the version twice. */
const where = ({ file, key }) => (key ? `${file} (${key})` : file);

/** Parses `x.y.z` (no leading zeros, no pre-release); throws when it isn't one. */
export function parse(version) {
  const match = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.exec(version);
  if (!match) throw new Error(`${version} is not a version x.y.z`);
  const [major, minor, patch] = match.slice(1).map(Number);
  if (minor >= 1000 || patch >= 1000) {
    throw new Error(`${version}: minor and patch must be below 1000 (Android's versionCode)`);
  }
  if (versionCodeOf({ major, minor, patch }) > MAX_CODE) {
    throw new Error(`${version}: the versionCode would be above Play's limit of ${MAX_CODE}`);
  }
  return { major, minor, patch };
}

const versionCodeOf = ({ major, minor, patch }) => major * 1_000_000 + minor * 1_000 + patch;

/** Android's versionCode of a version, as Tauri derives it. */
export const versionCode = (version) => versionCodeOf(parse(version));

function find(text, entry) {
  const global = new RegExp(entry.pattern.source, 'g');
  const count = [...text.matchAll(global)].length;
  if (count !== 1) {
    throw new Error(`${where(entry)}: the version is there ${count} times, not once`);
  }
  return entry.pattern.exec(text)[2];
}

/** The version in each file, `[{ file, key, version }]` (`key` only where a file has two). */
export function read(dir = root) {
  return FILES.map((entry) => ({
    file: entry.file,
    ...(entry.key && { key: entry.key }),
    version: find(fs.readFileSync(path.join(dir, entry.file), 'utf8'), entry),
  }));
}

/** The one version of all files; throws, listing them, when they disagree. */
export function check(dir = root) {
  const found = read(dir);
  const versions = new Set(found.map(({ version }) => version));
  if (versions.size !== 1) {
    const list = found.map((entry) => `  ${where(entry)}: ${entry.version}`).join('\n');
    throw new Error(
      `the app's version differs between the files:\n${list}\n` +
        'set it with: node scripts/app-version.mjs <x.y.z>',
    );
  }
  return found[0].version;
}

/** Sets `version` in every file. Its versionCode must be above that of every file. */
export function set(version, dir = root) {
  const code = versionCode(version);
  for (const entry of read(dir)) {
    const old = entry.version;
    // A version that doesn't parse can't be compared: replacing it is fine
    let oldCode;
    try {
      oldCode = versionCode(old);
    } catch {
      continue;
    }
    if (code <= oldCode) {
      throw new Error(
        `${version} (versionCode ${code}) isn't above ${old} (${oldCode}) in ${where(entry)}; ` +
          'Play only takes higher versionCodes',
      );
    }
  }
  for (const { file, pattern } of FILES) {
    const target = path.join(dir, file);
    const text = fs.readFileSync(target, 'utf8');
    fs.writeFileSync(target, text.replace(pattern, `$1${version}$3`));
  }
}

function main(argv) {
  if (argv.length === 0) {
    console.log(check());
  } else if (argv.length === 1 && argv[0] === '--code') {
    console.log(versionCode(check()));
  } else if (argv.length === 1 && !argv[0].startsWith('-')) {
    set(argv[0]);
    console.log(`app version ${argv[0]} (versionCode ${versionCode(argv[0])}) in:`);
    for (const entry of FILES) console.log(`  ${where(entry)}`);
  } else {
    throw new Error('usage: node scripts/app-version.mjs [--code | <x.y.z>]');
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main(process.argv.slice(2));
  } catch (error) {
    console.error(`error: ${error.message}`);
    process.exit(1);
  }
}
