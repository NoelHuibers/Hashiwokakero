// node --test scripts/tests/: itch.io's builds and pushes (scripts/itch-io.ts): the channels and
// their release files, the staging and butler's checks of it, what itch.io's latest builds make
// a release do, against a fake butler, unzip and itch.io
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { after, test } from 'node:test';

import {
  CHANNELS,
  type Fetch,
  gameOf,
  latestVersion,
  launchTargets,
  main,
  PLATFORM_WORDS,
  pick,
  plan,
  platformsOf,
  type Run,
  validationProblems,
  WHARF_LATEST,
} from '../itch-io.ts';

/** No log lines in the tests. */
const quiet = (_line: string) => undefined;

const dirs: string[] = [];
after(() => {
  for (const dir of dirs) fs.rmSync(dir, { recursive: true, force: true });
});
const tempDir = () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'itch-test-'));
  dirs.push(dir);
  return dir;
};

const V = '1.2.0';
/** Every release file the release job has, and some it has that itch.io doesn't get. */
const RELEASE_FILES = [
  `hashi-${V}-windows-x64.zip`,
  `hashi-${V}-windows-arm64.zip`,
  `hashi-${V}-windows-x64-setup.exe`,
  `hashi-${V}-windows-x64.msi`,
  `hashi-${V}-macos-universal.app.zip`,
  `hashi-${V}-macos-universal.dmg`,
  `hashi-${V}-linux-x64.AppImage`,
  `hashi-${V}-linux-arm64.AppImage`,
  `hashi-${V}-linux-x64.deb`,
  `hashi-${V}-foss-arm64-v8a.apk`,
  `hashi-${V}-foss-armeabi-v7a.apk`,
  `hashi-${V}-foss-x86_64.apk`,
];

test('each channel is tagged with its own platform by its name, and only that', () => {
  const expected: Record<string, string> = {
    'windows-x64': 'windows',
    'windows-arm64': 'windows',
    'mac-universal': 'osx',
    'linux-x64': 'linux',
    'linux-arm64': 'linux',
    'android-arm64': 'android',
    'android-armv7': 'android',
  };
  assert.deepEqual(
    CHANNELS.map((c) => c.name),
    Object.keys(expected),
  );
  for (const c of CHANNELS) {
    assert.deepEqual(platformsOf(c.name), [expected[c.name]], c.name);
    assert.match(c.name, /^[a-z0-9]+(-[a-z0-9]+)*$/, 'kebab-case');
    if (c.launch !== null) assert.equal(c.launch.platform, expected[c.name], c.name);
  }
  // A name that says two platforms gets both, as itch.io reads it
  assert.deepEqual(platformsOf('win-linux-mac-stable'), ['windows', 'osx', 'linux']);
  assert.deepEqual(platformsOf('web'), []);
  assert.ok(Object.values(PLATFORM_WORDS).flat().includes('osx'));
});

test('the release files go into their channels, the others stay', () => {
  const { found, missing } = pick(RELEASE_FILES, V);
  assert.deepEqual(
    found.map((f) => [f.channel.name, f.file]),
    [
      ['windows-x64', `hashi-${V}-windows-x64.zip`],
      ['windows-arm64', `hashi-${V}-windows-arm64.zip`],
      ['mac-universal', `hashi-${V}-macos-universal.app.zip`],
      ['linux-x64', `hashi-${V}-linux-x64.AppImage`],
      ['linux-arm64', `hashi-${V}-linux-arm64.AppImage`],
      ['android-arm64', `hashi-${V}-foss-arm64-v8a.apk`],
      ['android-armv7', `hashi-${V}-foss-armeabi-v7a.apk`],
    ],
  );
  assert.deepEqual(missing, []);
  // Another version's files are none of this one's; a failed build's channel is missing
  const partial = pick(['hashi-1.1.0-windows-x64.zip', `hashi-${V}-linux-x64.AppImage`], V);
  assert.deepEqual(
    partial.found.map((f) => f.channel.name),
    ['linux-x64'],
  );
  assert.equal(partial.missing.length, CHANNELS.length - 1);
});

test('the game is user/game as in its address', () => {
  assert.equal(gameOf('noelhuibers/hashi'), 'noelhuibers/hashi');
  assert.equal(gameOf(' noel_h/hashi-puzzle \n'), 'noel_h/hashi-puzzle');
  for (const wrong of [
    undefined,
    '',
    'hashi',
    'https://noelhuibers.itch.io/hashi',
    'a/b/c',
    'Noel/Hashi',
    'a/',
  ]) {
    assert.throws(() => gameOf(wrong), /ITCH_GAME/, String(wrong));
  }
});

/** butler validate's output for a folder with these launch targets (as butler 15.32 prints it). */
const validateOutput = (dir: string, targets: string[]) =>
  [
    `• Validating build directory ${dir}`,
    'For host 64-bit Linux (native) (use --platform and --arch to simulate others)',
    '',
    `No manifest found (expected it to be at ${dir}/.itch.toml)`,
    '',
    'Heuristics will be used to launch your project.',
    '✓ Configured in 8.43ms',
    '',
    '✓ Heuristic results (best first):',
    ...targets.flatMap((t, i) => [
      '',
      `  → Implicit launch target ${i + 1}`,
      `    | (${dir}/${t}) (native)`,
      `    |-- 28.59 MiB ${t} windows-amd64 win(arch=amd64)`,
      '    |-- ↗ Will be launched as a native application',
    ]),
  ].join('\n');

test("butler's launch targets decide whether the itch app could start the build", () => {
  const dir = '/tmp/itch-x/windows-x64';
  assert.deepEqual(launchTargets(validateOutput(dir, ['hashi.exe'])), [`${dir}/hashi.exe`]);
  assert.deepEqual(launchTargets(validateOutput(dir, [])), []);
  assert.deepEqual(validationProblems(validateOutput(dir, ['hashi.exe']), dir, 'hashi.exe'), []);
  assert.deepEqual(
    validationProblems(validateOutput(dir, ['Hashi.app', 'hashi.exe']), dir, 'hashi.exe'),
    [],
  );
  assert.match(
    validationProblems(validateOutput(dir, []), dir, 'hashi.exe')[0] ?? '',
    /nothing to start/,
  );
  assert.match(
    validationProblems(validateOutput(dir, ['uninstall.exe']), dir, 'hashi.exe')[0] ?? '',
    /would start uninstall\.exe, not hashi\.exe/,
  );
});

/** itch.io's wharf/latest, answering for each channel. */
const itch =
  (answers: Record<string, string>, calls: string[] = []): Fetch =>
  async (url) => {
    calls.push(url);
    const channel = new URL(url).searchParams.get('channel_name') ?? '';
    const body = answers[channel] ?? '{"errors":["invalid channel"]}';
    return { status: 200, text: async () => body };
  };

test("itch.io's latest build of a channel", async () => {
  const calls: string[] = [];
  const fetcher = itch(
    {
      'windows-x64': '{"latest":"1.1.0"}',
      'mac-universal': '{}',
      'linux-x64': '{"errors":["invalid target (bad user)"]}',
      'linux-arm64': 'not json',
    },
    calls,
  );
  assert.equal(await latestVersion(fetcher, 'noel/hashi', 'windows-x64', quiet), '1.1.0');
  assert.equal(calls[0], `${WHARF_LATEST}?target=noel%2Fhashi&channel_name=windows-x64`);
  // A channel without builds, a build without a user version
  assert.equal(await latestVersion(fetcher, 'noel/hashi', 'android-arm64', quiet), null);
  assert.equal(await latestVersion(fetcher, 'noel/hashi', 'mac-universal', quiet), null);
  // itch.io doesn't say (a page that isn't public, a wrong game, no answer): unknown
  const lines: string[] = [];
  assert.equal(
    await latestVersion(fetcher, 'noel/hashi', 'linux-x64', (l) => lines.push(l)),
    undefined,
  );
  assert.equal(
    await latestVersion(fetcher, 'noel/hashi', 'linux-arm64', (l) => lines.push(l)),
    undefined,
  );
  const down: Fetch = () => Promise.reject(new Error('timeout'));
  assert.equal(
    await latestVersion(down, 'noel/hashi', 'windows-x64', (l) => lines.push(l)),
    undefined,
  );
  assert.equal(lines.length, 3);
  assert.ok(lines.every((l) => l.startsWith('::warning::')));
});

test('a channel is pushed unless it has this version, and never over a newer one', () => {
  assert.deepEqual(plan(null, '1.2.0', 'windows-x64'), { action: 'push' });
  assert.deepEqual(plan(undefined, '1.2.0', 'windows-x64'), { action: 'push' });
  assert.deepEqual(plan('1.1.0', '1.2.0', 'windows-x64'), { action: 'push' });
  assert.deepEqual(plan('1.2.0', '1.2.0', 'windows-x64'), { action: 'nothing' });
  // butler's own numbers (a push without --userversion) aren't ours
  assert.deepEqual(plan('17', '1.2.0', 'windows-x64'), { action: 'push' });
  assert.throws(
    () => plan('1.10.0', '1.9.0', 'linux-x64'),
    /linux-x64 has 1\.10\.0, newer than 1\.9\.0/,
  );
});

/** The release files of RELEASE_FILES in a folder (zips and all, as empty files). */
function releaseDir(files = RELEASE_FILES): string {
  const dir = tempDir();
  for (const f of files) fs.writeFileSync(path.join(dir, f), f);
  return dir;
}

/**
 * A fake unzip (each zip's contents: the program, or the Mac app) and butler (validate: what the
 * folder has that the itch app would start; push: its arguments), and every call.
 */
function fakeTools(o: { pushFails?: string } = {}) {
  const calls: string[][] = [];
  const run: Run = (program, args) => {
    calls.push([program, ...args]);
    if (program === 'unzip') {
      const [, zip = '', , dir = ''] = args;
      if (zip.endsWith('.app.zip')) {
        fs.mkdirSync(path.join(dir, 'Hashi.app/Contents/MacOS'), { recursive: true });
        fs.writeFileSync(path.join(dir, 'Hashi.app/Contents/MacOS/hashi'), 'mach-o');
      } else {
        fs.writeFileSync(path.join(dir, 'hashi.exe'), 'MZ');
      }
      return { status: 0, stdout: '', stderr: '' };
    }
    if (program === 'butler' && args[0] === 'validate') {
      const dir = args[args.length - 1] ?? '';
      const targets = fs
        .readdirSync(dir)
        .filter((f) => /\.(exe|app|AppImage)$/.test(f) && !f.startsWith('uninstall'));
      return { status: 0, stdout: validateOutput(dir, targets), stderr: '' };
    }
    if (program === 'butler' && args[0] === 'push') {
      if (o.pushFails !== undefined && args.some((a) => a.endsWith(`:${o.pushFails}`)))
        return { status: 1, stdout: '', stderr: 'invalid game' };
      return { status: 0, stdout: '✓ Build is now processing, should be up in a bit.', stderr: '' };
    }
    return { status: 127, stdout: '', stderr: `${program}: not faked` };
  };
  return { run, calls };
}

const pushes = (calls: string[][]) =>
  calls.filter((c) => c[0] === 'butler' && c[1] === 'push').map((c) => c.slice(1));

test('a dry run stages and checks every build and calls nobody', async () => {
  const dir = releaseDir();
  const { run, calls } = fakeTools();
  const fetched: string[] = [];
  const summary = await main(
    ['release', '--version', V, '--game', 'noel/hashi', '--dir', dir, '--dry-run'],
    {},
    run,
    itch({}, fetched),
    quiet,
  );
  assert.equal(fetched.length, 0);
  assert.equal(summary.length, CHANNELS.length);
  assert.ok(summary.every((l) => l.startsWith('itch.io, dry run: ')));
  // The zips unpacked, the desktop builds validated for their platform
  assert.deepEqual(
    calls.filter((c) => c[0] === 'unzip').map((c) => path.basename(c[2] ?? '')),
    [
      `hashi-${V}-windows-x64.zip`,
      `hashi-${V}-windows-arm64.zip`,
      `hashi-${V}-macos-universal.app.zip`,
    ],
  );
  assert.deepEqual(
    calls.filter((c) => c[1] === 'validate').map((c) => [c[3], c[5], path.basename(c[6] ?? '')]),
    [
      ['windows', 'amd64', 'windows-x64'],
      ['windows', 'amd64', 'windows-arm64'],
      ['osx', 'amd64', 'mac-universal'],
      ['linux', 'amd64', 'linux-x64'],
    ],
  );
  assert.deepEqual(
    pushes(calls).map((p) => [p[1], path.basename(p[2] ?? ''), p[3]]),
    CHANNELS.map((c) => ['--dry-run', c.name, `noel/hashi:${c.name}`]),
  );
});

test('a release pushes each build with its version, once', async () => {
  const dir = releaseDir();
  const { run, calls } = fakeTools();
  const env = { BUTLER_API_KEY: 'key' };
  const args = () => ['release', '--version', V, '--game', 'noel/hashi', '--dir', dir];
  const summary = await main(
    args(),
    env,
    run,
    itch({ 'windows-x64': '{"latest":"1.1.0"}' }),
    quiet,
  );
  assert.equal(summary.length, CHANNELS.length);
  const pushed = pushes(calls);
  assert.deepEqual(
    pushed.map((p) => p[2]),
    CHANNELS.map((c) => `noel/hashi:${c.name}`),
  );
  for (const p of pushed)
    assert.deepEqual(p.slice(3), ['--userversion', V, '--if-changed', '--fix-permissions']);
  // What was pushed: the program alone, the app, the AppImage executable, the APK as it is
  const staged = (channel: string) =>
    pushed.find((p) => p[2] === `noel/hashi:${channel}`)?.[1] ?? '';
  assert.ok(
    pushed.every((p) => !fs.existsSync(p[1] ?? '')),
    'the staging is gone afterwards',
  );
  assert.ok(staged('windows-x64').endsWith('/windows-x64'));

  // Again (a rerun): the channels that have this version are left alone
  const again = fakeTools();
  const latest = Object.fromEntries(
    CHANNELS.filter((c) => c.name !== 'android-armv7').map((c) => [c.name, `{"latest":"${V}"}`]),
  );
  const second = await main(args(), env, again.run, itch(latest), quiet);
  assert.deepEqual(
    pushes(again.calls).map((p) => p[2]),
    ['noel/hashi:android-armv7'],
  );
  assert.equal(
    second.filter((l) => l.includes('already, nothing to do')).length,
    CHANNELS.length - 1,
  );
});

test('the staged builds are what the itch app and the page get', async () => {
  const dir = releaseDir();
  const seen = new Map<string, { files: string[]; modes: number[] }>();
  const { run: fake } = fakeTools();
  // Looks into each folder as butler pushes it
  const run: Run = (program, args) => {
    if (program === 'butler' && args[0] === 'push') {
      const folder = args[1] ?? '';
      const files = fs.readdirSync(folder, { recursive: true }).map(String).sort();
      const modes = files
        .map((f) => fs.statSync(path.join(folder, f)))
        .filter((s) => s.isFile())
        .map((s) => s.mode & 0o777);
      seen.set(path.basename(folder), { files, modes });
    }
    return fake(program, args);
  };
  await main(
    ['release', '--version', V, '--game', 'noel/hashi', '--dir', dir],
    { BUTLER_API_KEY: 'key' },
    run,
    itch({}),
    quiet,
  );
  assert.deepEqual(seen.get('windows-x64')?.files, ['hashi.exe']);
  assert.deepEqual(seen.get('mac-universal')?.files, [
    'Hashi.app',
    'Hashi.app/Contents',
    'Hashi.app/Contents/MacOS',
    'Hashi.app/Contents/MacOS/hashi',
  ]);
  assert.deepEqual(seen.get('linux-x64'), {
    files: [`hashi-${V}-linux-x64.AppImage`],
    modes: [0o755],
  });
  assert.deepEqual(seen.get('linux-arm64')?.modes, [0o755]);
  assert.deepEqual(seen.get('android-arm64')?.files, [`hashi-${V}-foss-arm64-v8a.apk`]);
  assert.deepEqual(seen.get('android-armv7')?.files, [`hashi-${V}-foss-armeabi-v7a.apk`]);
});

test('a newer version on itch.io stops the release before any push', async () => {
  const dir = releaseDir();
  const { run, calls } = fakeTools();
  await assert.rejects(
    main(
      ['release', '--version', V, '--game', 'noel/hashi', '--dir', dir],
      { BUTLER_API_KEY: 'key' },
      run,
      itch({ 'android-armv7': '{"latest":"1.3.0"}' }),
      quiet,
    ),
    /android-armv7 has 1\.3\.0, newer than 1\.2\.0/,
  );
  assert.deepEqual(pushes(calls), []);
});

test('missing builds are warnings, a build the itch app could not start an error', async () => {
  // Only the Linux and Android files (the Windows and Mac builds failed)
  const dir = releaseDir(RELEASE_FILES.filter((f) => /linux|foss/.test(f)));
  const { run, calls } = fakeTools();
  const lines: string[] = [];
  const summary = await main(
    ['release', '--version', V, '--game', 'noel/hashi', '--dir', dir],
    { BUTLER_API_KEY: 'key' },
    run,
    itch({}),
    (l) => lines.push(l),
  );
  assert.equal(pushes(calls).length, 4);
  assert.equal(lines.filter((l) => l.startsWith('::warning::')).length, 3);
  assert.ok(summary.some((l) => l.includes('nothing goes to noel/hashi:windows-x64')));

  // A zip without the program
  const broken = releaseDir([`hashi-${V}-windows-x64.zip`]);
  const empty: Run = (program, args) =>
    program === 'unzip' ? { status: 0, stdout: '', stderr: '' } : fakeTools().run(program, args);
  await assert.rejects(
    main(
      ['release', '--version', V, '--game', 'noel/hashi', '--dir', broken, '--dry-run'],
      {},
      empty,
      itch({}),
      quiet,
    ),
    /windows-x64\.zip \(windows-x64\): the itch app would find nothing to start/,
  );
  // Nothing for itch.io at all
  await assert.rejects(
    main(
      ['release', '--version', V, '--game', 'noel/hashi', '--dir', releaseDir([]), '--dry-run'],
      {},
      run,
      itch({}),
      quiet,
    ),
    /none of the release files/,
  );
});

test('a release needs the key, a version and a game; a failed push fails it', async () => {
  const dir = releaseDir();
  const { run } = fakeTools();
  await assert.rejects(
    main(
      ['release', '--version', V, '--game', 'noel/hashi', '--dir', dir],
      {},
      run,
      itch({}),
      quiet,
    ),
    /BUTLER_API_KEY/,
  );
  await assert.rejects(
    main(
      ['release', '--version', 'v1.2', '--game', 'noel/hashi', '--dir', dir, '--dry-run'],
      {},
      run,
      itch({}),
      quiet,
    ),
    /isn't x\.y\.z/,
  );
  await assert.rejects(
    main(
      ['release', '--version', V, '--game', 'hashi', '--dir', dir, '--dry-run'],
      {},
      run,
      itch({}),
      quiet,
    ),
    /ITCH_GAME/,
  );
  const failing = fakeTools({ pushFails: 'mac-universal' });
  await assert.rejects(
    main(
      ['release', '--version', V, '--game', 'noel/hashi', '--dir', dir],
      { BUTLER_API_KEY: 'k' },
      failing.run,
      itch({}),
      quiet,
    ),
    /butler push .*noel\/hashi:mac-universal .*failed \(1\): invalid game/,
  );
});
