// The desktop and Android apps on itch.io (docs/stores.md, itch.io), pushed with butler, itch.io's
// command line tool:
//
//   node scripts/itch-io.ts release --version <x.y.z> --game <user/game> --dir <dir>
//                                   [--butler <program>] [--dry-run]
//
// Each release file of `dir` that itch.io gets goes into a channel of its own (CHANNELS): the
// Windows programs without their installers (the zips), the Mac app (its zip, unpacked), the
// Linux AppImages and the foss APKs. butler pushes a folder, so each is staged into one first
// (a zip unpacked, a single file alone in it), and checked with `butler validate`, which shows
// what the itch app would start: the desktop channels must have the program there. Then each
// channel is pushed with the app's version as butler's user version.
//
// A rerun pushes nothing twice: a channel whose latest build has this version already is left
// as it is (itch.io's wharf/latest, no login needed), and --if-changed skips a push of the same
// files anyway. It refuses when a channel has a newer version, before pushing anything.
// A release file missing from `dir` (its build failed) is a warning, the others still go.
//
// --dry-run: the staging, the checks and butler's file lists (push --dry-run), calling nobody.
//
// butler logs in with BUTLER_API_KEY (docs/stores.md, itch.io).
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { compareVersions } from './microsoft-store.ts';

/** butler's platforms, for `butler validate --platform`. */
export type Platform = 'windows' | 'osx' | 'linux';

/** A channel of the game on itch.io and the release file that goes into it. */
export interface Channel {
  /** The channel's name: itch.io tags the upload with the platform it names (see below). */
  name: string;
  /** The release file (scripts of app-release.yml name them). */
  file: (version: string) => string;
  /** zip: unpacked, its contents are the build; file: the file alone is the build. */
  kind: 'zip' | 'file';
  /**
   * The platform the itch app runs it on, and what it starts there (`butler validate`'s launch
   * target, relative to the build); `null` where the itch app doesn't run it (Android, which only
   * downloads from the page, and Linux on arm64: butler's validation knows only x86 there).
   */
  launch: { platform: Platform; target: (version: string) => string } | null;
  /** Make the file executable (the AppImages; artifacts and downloads lose the mode). */
  executable?: true;
}

/**
 * The channels, in the order they are pushed. The names follow itch.io's rules: one containing
 * `win`/`windows`, `mac`/`osx`, `linux` or `android` gets that platform's tag, and nothing tags
 * an architecture (the owner names each upload on the game page, docs/stores.md).
 */
export const CHANNELS: readonly Channel[] = [
  {
    name: 'windows-x64',
    file: (v) => `hashi-${v}-windows-x64.zip`,
    kind: 'zip',
    launch: { platform: 'windows', target: () => 'hashi.exe' },
  },
  {
    name: 'windows-arm64',
    file: (v) => `hashi-${v}-windows-arm64.zip`,
    kind: 'zip',
    launch: { platform: 'windows', target: () => 'hashi.exe' },
  },
  {
    name: 'mac-universal',
    file: (v) => `hashi-${v}-macos-universal.app.zip`,
    kind: 'zip',
    launch: { platform: 'osx', target: () => 'Hashi.app' },
  },
  {
    name: 'linux-x64',
    file: (v) => `hashi-${v}-linux-x64.AppImage`,
    kind: 'file',
    launch: { platform: 'linux', target: (v) => `hashi-${v}-linux-x64.AppImage` },
    executable: true,
  },
  {
    name: 'linux-arm64',
    file: (v) => `hashi-${v}-linux-arm64.AppImage`,
    kind: 'file',
    launch: null,
    executable: true,
  },
  {
    name: 'android-arm64',
    file: (v) => `hashi-${v}-foss-arm64-v8a.apk`,
    kind: 'file',
    launch: null,
  },
  {
    name: 'android-armv7',
    file: (v) => `hashi-${v}-foss-armeabi-v7a.apk`,
    kind: 'file',
    launch: null,
  },
];

/** The words in a channel's name that itch.io tags a platform by. */
export const PLATFORM_WORDS = {
  windows: ['win'],
  osx: ['mac', 'osx'],
  linux: ['linux'],
  android: ['android'],
} as const;

/** The platforms itch.io reads out of a channel's name (substrings, as itch.io matches them). */
export function platformsOf(channel: string): string[] {
  return Object.entries(PLATFORM_WORDS)
    .filter(([, words]) => words.some((w) => channel.includes(w)))
    .map(([platform]) => platform);
}

/** The game, `user/game` as butler takes it: an itch.io account and its game's URL name. */
export function gameOf(value: string | undefined): string {
  const game = value?.trim() ?? '';
  if (!/^[a-z0-9_-]+\/[a-z0-9_-]+$/.test(game))
    throw new Error(
      `the game "${game}" isn't <user>/<game> as in https://<user>.itch.io/<game> (the variable ITCH_GAME)`,
    );
  return game;
}

/** The release files of `files` (names) for each channel; the channels whose file is missing. */
export function pick(
  files: readonly string[],
  version: string,
): { found: { channel: Channel; file: string }[]; missing: Channel[] } {
  const found: { channel: Channel; file: string }[] = [];
  const missing: Channel[] = [];
  for (const channel of CHANNELS) {
    const file = channel.file(version);
    if (files.includes(file)) found.push({ channel, file });
    else missing.push(channel);
  }
  return { found, missing };
}

/** The launch targets `butler validate` lists: their paths, as it prints them. */
export function launchTargets(output: string): string[] {
  return [...output.matchAll(/^\s*\|\s+\((.+)\)\s+\(native\)\s*$/gm)].map((m) => m[1] ?? '');
}

/** Problems of a staged build, by `butler validate`'s output for its folder `dir`. */
export function validationProblems(output: string, dir: string, target: string): string[] {
  const targets = launchTargets(output).map((t) => path.relative(dir, t));
  if (targets.length === 0) return ['the itch app would find nothing to start (no launch target)'];
  if (!targets.includes(target))
    return [`the itch app would start ${targets.join(', ')}, not ${target}`];
  return [];
}

// --- itch.io ---------------------------------------------------------------------------------

export const WHARF_LATEST = 'https://api.itch.io/wharf/latest';

/** Fetches a URL: its status and body. */
export type Fetch = (url: string) => Promise<{ status: number; text: () => Promise<string> }>;

const fetchUrl: Fetch = (url) => fetch(url, { signal: AbortSignal.timeout(30_000) });

/**
 * The user version of the latest build of a channel, from itch.io's wharf/latest (no login):
 * `null` when the channel has no build yet (or one without a user version), `undefined` when
 * itch.io doesn't say (e.g. for a page that isn't public yet): then the push goes ahead, and
 * --if-changed keeps it from pushing the same files twice.
 */
export async function latestVersion(
  fetcher: Fetch,
  game: string,
  channel: string,
  log = console.log,
): Promise<string | null | undefined> {
  const url = `${WHARF_LATEST}?target=${encodeURIComponent(game)}&channel_name=${encodeURIComponent(channel)}`;
  let body: unknown;
  try {
    const response = await fetcher(url);
    body = JSON.parse(await response.text());
  } catch (e) {
    log(
      `::warning::itch.io's latest build of ${channel} unknown: ${e instanceof Error ? e.message : e}`,
    );
    return undefined;
  }
  if (typeof body !== 'object' || body === null) return undefined;
  const { latest, errors } = body as { latest?: unknown; errors?: unknown };
  if (typeof latest === 'string') return latest;
  if (Array.isArray(errors)) {
    if (errors.includes('invalid channel')) return null;
    log(`::warning::itch.io's latest build of ${channel} unknown: ${errors.join(', ')}`);
    return undefined;
  }
  return null;
}

export type Step = { action: 'push' } | { action: 'nothing' };

/**
 * What to do with a channel whose latest build has the user version `latest` (see
 * [`latestVersion`]): nothing when it has this version, push otherwise. Refuses a newer one.
 */
export function plan(latest: string | null | undefined, version: string, channel: string): Step {
  if (latest === null || latest === undefined) return { action: 'push' };
  if (latest === version) return { action: 'nothing' };
  if (/^\d+\.\d+\.\d+$/.test(latest) && compareVersions(latest, version) > 0)
    throw new Error(`the channel ${channel} has ${latest}, newer than ${version}`);
  return { action: 'push' };
}

// --- Running butler --------------------------------------------------------------------------

/** Runs a program; its exit status and output. */
export type Run = (
  program: string,
  args: readonly string[],
) => { status: number | null; stdout: string; stderr: string };

const spawn: Run = (program, args) => {
  const r = spawnSync(program, args, {
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
    maxBuffer: 16 * 1048576,
  });
  if (r.error !== undefined) throw new Error(`${program}: ${r.error.message}`);
  return { status: r.status, stdout: r.stdout, stderr: r.stderr };
};

/** Runs it and fails with its output when it fails. Its output, both streams. */
function must(run: Run, program: string, args: readonly string[]): string {
  const r = run(program, args);
  const out = `${r.stdout}\n${r.stderr}`.trim();
  if (r.status !== 0) throw new Error(`${program} ${args.join(' ')} failed (${r.status}): ${out}`);
  return out;
}

/** A channel's build, staged in its own folder and checked. */
export interface Build {
  channel: Channel;
  /** The release file's name, and where it is */
  file: string;
  source: string;
  /** The folder it is staged in, which butler pushes */
  dir: string;
}

/**
 * Stages each channel's file in a folder of `work` (a zip unpacked, a file alone) and checks it
 * with `butler validate`: the itch app has to find the program to start.
 */
export function stage(
  run: Run,
  o: { butler: string; version: string; dir: string; work: string },
  found: { channel: Channel; file: string }[],
): Build[] {
  return found.map(({ channel, file }) => {
    const from = path.join(o.dir, file);
    const dir = path.join(o.work, channel.name);
    fs.rmSync(dir, { recursive: true, force: true });
    fs.mkdirSync(dir, { recursive: true });
    if (channel.kind === 'zip') {
      must(run, 'unzip', ['-q', from, '-d', dir]);
    } else {
      fs.copyFileSync(from, path.join(dir, file));
      if (channel.executable) fs.chmodSync(path.join(dir, file), 0o755);
    }
    if (channel.launch !== null) {
      // butler's validation knows the architectures 386 and amd64; it takes Windows' arm64
      // program as a launch target there too
      const out = must(run, o.butler, [
        'validate',
        '--platform',
        channel.launch.platform,
        '--arch',
        'amd64',
        dir,
      ]);
      const problems = validationProblems(out, dir, channel.launch.target(o.version));
      if (problems.length > 0) throw new Error(`${file} (${channel.name}): ${problems.join('; ')}`);
    }
    return { channel, file, source: from, dir };
  });
}

export interface ReleaseOptions {
  butler: string;
  game: string;
  version: string;
  dryRun: boolean;
}

/** Pushes each build into its channel (see the top). Its summary. */
export async function release(
  run: Run,
  fetcher: Fetch,
  builds: Build[],
  o: ReleaseOptions,
  log = console.log,
): Promise<string[]> {
  const summary: string[] = [];
  if (o.dryRun) {
    for (const b of builds) {
      const out = must(run, o.butler, ['push', '--dry-run', b.dir, `${o.game}:${b.channel.name}`]);
      log(`${b.channel.name}: ${out}`);
      summary.push(
        `itch.io, dry run: ${b.file} checked, would go to ${o.game}:${b.channel.name} as ${o.version}`,
      );
    }
    return summary;
  }
  // Every channel's step first: a newer version anywhere stops the release before any push
  const steps: Step[] = [];
  for (const b of builds) {
    steps.push(
      plan(await latestVersion(fetcher, o.game, b.channel.name, log), o.version, b.channel.name),
    );
  }
  for (const [i, b] of builds.entries()) {
    const target = `${o.game}:${b.channel.name}`;
    if (steps[i]?.action === 'nothing') {
      summary.push(`itch.io: ${target} has ${o.version} already, nothing to do`);
      continue;
    }
    const size = (fs.statSync(b.source).size / 1048576).toFixed(1);
    log(`${b.file} (${size} MB): pushing to ${target}`);
    const out = must(run, o.butler, [
      'push',
      b.dir,
      target,
      '--userversion',
      o.version,
      '--if-changed',
      '--fix-permissions',
    ]);
    log(out);
    summary.push(`itch.io: ${b.file} pushed to ${target} as ${o.version}`);
  }
  return summary;
}

// --- Command line ----------------------------------------------------------------------------

const USAGE =
  'usage: node scripts/itch-io.ts release --version <x.y.z> --game <user/game> --dir <dir> [--butler <program>] [--dry-run]';

function usage(): never {
  console.error(USAGE);
  process.exit(2);
}

export async function main(
  args: string[],
  env: NodeJS.ProcessEnv,
  run: Run = spawn,
  fetcher: Fetch = fetchUrl,
  log = console.log,
): Promise<string[]> {
  const command = args.shift();
  const flag = (name: string) => {
    const i = args.indexOf(name);
    if (i !== -1) args.splice(i, 1);
    return i !== -1;
  };
  const option = (name: string) => {
    const i = args.indexOf(name);
    if (i === -1) return undefined;
    const value = args[i + 1];
    if (value === undefined) usage();
    args.splice(i, 2);
    return value;
  };
  if (command !== 'release') usage();
  const dryRun = flag('--dry-run');
  const version = option('--version');
  const game = gameOf(option('--game'));
  const dir = option('--dir');
  const butler = option('--butler') ?? 'butler';
  if (version === undefined || dir === undefined || args.length > 0) usage();
  if (!/^\d+\.\d+\.\d+$/.test(version)) throw new Error(`the version ${version} isn't x.y.z`);
  if (!(dryRun || env.BUTLER_API_KEY)) throw new Error('BUTLER_API_KEY not set');

  const { found, missing } = pick(fs.readdirSync(dir), version);
  if (found.length === 0)
    throw new Error(`none of the release files for itch.io in ${dir} (${version})`);
  const warnings = missing.map(
    (c) =>
      `itch.io: no ${c.file(version)}, nothing goes to ${game}:${c.name} (its build failed or was skipped)`,
  );
  for (const w of warnings) log(`::warning::${w}`);

  const work = fs.mkdtempSync(path.join(os.tmpdir(), 'itch-'));
  try {
    const builds = stage(run, { butler, version, dir, work }, found);
    const summary = await release(run, fetcher, builds, { butler, game, version, dryRun }, log);
    return [...summary, ...warnings];
  } finally {
    fs.rmSync(work, { recursive: true, force: true });
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    const summary = await main(process.argv.slice(2), process.env);
    for (const line of summary) console.log(line);
    if (process.env.GITHUB_STEP_SUMMARY)
      fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${summary.join('\n\n')}\n`);
  } catch (e) {
    console.log(`::error::${e instanceof Error ? e.message : e}`);
    process.exit(1);
  }
}
