// node --test scripts/tests/: the Snap Store's snaps and upload and the Linux MetaInfo
// (scripts/snap-store.ts): the MetaInfo from the store texts, the snaps' checks, the store's
// revisions and what a release does with them, against a fake snapcraft and unsquashfs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { after, test } from 'node:test';

import {
  checkSnaps,
  DESCRIPTION_LENGTH,
  descriptionBlocks,
  LANGUAGES,
  main,
  metainfo,
  metainfoTexts,
  plainText,
  plan,
  type Revision,
  type Run,
  release,
  revisionsOf,
  SUMMARY_LENGTH,
  snapArch,
  snapInfo,
  snapName,
  snapProblems,
  summaryOf,
  type Texts,
  textProblems,
} from '../snap-store.ts';

/** No log lines in the tests. */
const quiet = (_line: string) => undefined;

const dirs: string[] = [];
after(() => {
  for (const dir of dirs) fs.rmSync(dir, { recursive: true, force: true });
});
const tempDir = () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'hashi-snap-'));
  dirs.push(dir);
  return dir;
};

const snapYaml = (o: { name?: string; version?: string; arch?: string; extra?: string } = {}) =>
  [
    `name: ${o.name ?? 'hashi'}`,
    "version: '1.2.0'".replace('1.2.0', o.version ?? '1.2.0'),
    'summary: Daily Bridges Puzzle',
    'description: |-',
    '  Hashi is Hashiwokakero.',
    '  name: not a field',
    'architectures:',
    `- ${o.arch ?? 'amd64'}`,
    'base: core24',
    'assumes:',
    '- command-chain',
    'apps:',
    '  hashi:',
    '    command: usr/bin/hashi',
    'confinement: strict',
    'grade: stable',
    o.extra ?? '',
  ].join('\n');

const TABLE = `Rev.    Uploaded              Arches         Version    Channels
7       2026-11-12T10:00:00Z  arm64          1.2.0      -
6       2026-11-12T09:59:00Z  amd64          1.2.0      latest/edge*
5       2026-10-25T10:00:00Z  amd64, arm64   1.1.0      latest/stable*, latest/candidate*
`;

/** A fake of the programs: answers by the first arguments, and records every call. */
function fake(answers: Record<string, string | { status: number; stderr: string }>) {
  const calls: string[] = [];
  const run: Run = (program, args) => {
    const call = [program, ...args].join(' ');
    calls.push(call);
    const key = Object.keys(answers).find((k) => call.startsWith(k));
    if (key === undefined) return { status: 1, stdout: '', stderr: `unexpected: ${call}` };
    const answer = answers[key];
    if (typeof answer === 'string') return { status: 0, stdout: answer, stderr: '' };
    return { status: answer?.status ?? 1, stdout: '', stderr: answer?.stderr ?? '' };
  };
  return { run, calls };
}

// --- MetaInfo ------------------------------------------------------------------------------

test('the description becomes paragraphs and lists', () => {
  const blocks = descriptionBlocks(
    'Hashi is a puzzle.\n\nHOW TO PLAY\nConnect the islands.\n\nTHE DAILY\n• One a day\n•   Rankings\n\nFree.',
  );
  assert.deepEqual(blocks, [
    { p: 'Hashi is a puzzle.' },
    { p: 'HOW TO PLAY' },
    { p: 'Connect the islands.' },
    { p: 'THE DAILY' },
    { ul: ['One a day', 'Rankings'] },
    { p: 'Free.' },
  ]);
  assert.equal(
    plainText(blocks),
    'Hashi is a puzzle.\n\nHOW TO PLAY\n\nConnect the islands.\n\nTHE DAILY\n\n- One a day\n- Rankings\n\nFree.',
  );
  // A paragraph between points ends the list
  assert.deepEqual(descriptionBlocks('• a\nb\n• c'), [{ ul: ['a'] }, { p: 'b' }, { ul: ['c'] }]);
});

test('the summary is the title after the name', () => {
  assert.equal(summaryOf('Hashi: Daily Bridges Puzzle'), 'Daily Bridges Puzzle');
  assert.equal(summaryOf('Hashi：毎日のパズル。'), '毎日のパズル');
  assert.throws(() => summaryOf('Daily Bridges Puzzle'), /isn't "Hashi: <summary>"/);
});

test("every language's store texts fit the snap and the MetaInfo", () => {
  const texts = metainfoTexts();
  assert.equal(texts.length, LANGUAGES.length);
  assert.equal(texts[0]?.lang, null);
  assert.equal(texts[0]?.summary, 'Daily Bridges Puzzle');
  for (const t of texts) {
    assert.deepEqual(textProblems(t), [], t.play);
    assert.ok(
      t.blocks.some((b) => 'ul' in b),
      `${t.play} has lists`,
    );
  }
  // The gettext names, each once
  const langs = texts.map((t) => t.lang).filter((l) => l !== null);
  assert.equal(new Set(langs).size, langs.length);
  assert.ok(langs.includes('pt_BR') && langs.includes('zh_TW') && langs.includes('de'));
});

test('texts over the limits or naming other stores are problems', () => {
  const t: Texts = {
    lang: null,
    play: 'en-US',
    summary: 'x'.repeat(SUMMARY_LENGTH + 1),
    blocks: [{ p: 'y'.repeat(DESCRIPTION_LENGTH) }, { p: 'Get it on Google Play' }],
  };
  const problems = textProblems(t);
  assert.equal(problems.length, 3, problems.join('\n'));
  assert.match(problems[0] ?? '', /summary has 79 characters/);
  assert.match(problems[1] ?? '', /description has 4119 characters/);
  assert.match(problems[2] ?? '', /names Google Play/);
  assert.match(textProblems({ ...t, summary: 'ok', blocks: [] })[0] ?? '', /empty/);
});

test('the MetaInfo has the texts of every language, the release and the rating', () => {
  const texts: Texts[] = [
    { lang: 'de', play: 'de-DE', summary: 'Brücken & Inseln', blocks: [{ ul: ['Eins'] }] },
    {
      lang: null,
      play: 'en-US',
      summary: 'Daily <Bridges>',
      blocks: [{ p: 'Hashi "is" fun' }, { ul: ['One', 'Two'] }],
    },
  ];
  const xml = metainfo(texts, { version: '1.2.0', date: '2026-11-12', notes: 'Faster & calmer.' });
  assert.match(xml, /^<\?xml version="1.0" encoding="UTF-8"\?>\n/);
  assert.match(xml, /<id>rs\.hashi<\/id>/);
  assert.match(xml, /<launchable type="desktop-id">Hashi\.desktop<\/launchable>/);
  // English first, untranslated; escaped
  assert.match(
    xml,
    / {2}<summary>Daily &lt;Bridges&gt;<\/summary>\n {2}<summary xml:lang="de">Brücken &amp; Inseln<\/summary>/,
  );
  assert.match(xml, /<p>Hashi &quot;is&quot; fun<\/p>/);
  assert.match(xml, / {4}<ul>\n {6}<li>One<\/li>\n {6}<li>Two<\/li>\n {4}<\/ul>/);
  // Only the points are translated, never the list (appstreamcli refuses that)
  assert.match(xml, / {4}<ul>\n {6}<li xml:lang="de">Eins<\/li>\n {4}<\/ul>/);
  assert.doesNotMatch(xml, /<ul xml:lang/);
  assert.match(xml, /<content_attribute id="social-chat">mild<\/content_attribute>/);
  assert.match(xml, /<color type="primary" scheme_preference="dark">#101827<\/color>/);
  assert.match(
    xml,
    /<release version="1\.2\.0" date="2026-11-12">\n {6}<description>\n {8}<p>Faster &amp; calmer\.<\/p>/,
  );
  assert.match(xml, /releases\/tag\/app-v1\.2\.0<\/url>/);
  // Notes that name another store stay out, as no notes do
  for (const notes of ['Now in Google Play Games.', '']) {
    const plain = metainfo(texts, { version: '1.2.0', date: '2026-11-12', notes });
    assert.match(plain, /<release version="1\.2\.0" date="2026-11-12">\n {6}<url /);
  }
  assert.throws(
    () => metainfo(texts, { version: '1.2', date: '2026-11-12', notes: '' }),
    /x\.y\.z/,
  );
  assert.throws(
    () => metainfo(texts, { version: '1.2.0', date: '12.11.2026', notes: '' }),
    /YYYY-MM-DD/,
  );
  assert.throws(
    () => metainfo(texts.slice(0, 1), { version: '1.2.0', date: '2026-11-12', notes: '' }),
    /English/,
  );
});

test('metainfo writes the file from the store texts', () => {
  const out = path.join(tempDir(), 'linux', 'rs.hashi.metainfo.xml');
  const notes = path.join(tempDir(), 'whatsnew');
  fs.writeFileSync(notes, 'Hashi 1.2: calmer.\n');
  const summary = main(
    ['metainfo', '--version', '1.2.0', '--date', '2026-11-12', '--whatsnew', notes, '--out', out],
    {},
  );
  assert.match(summary[0] ?? '', /1\.2\.0 \(2026-11-12\) in 21 languages/);
  const xml = fs.readFileSync(out, 'utf8');
  assert.match(xml, /<summary>Daily Bridges Puzzle<\/summary>/);
  assert.match(xml, /<summary xml:lang="ja">/);
  assert.match(xml, /<p>Hashi 1\.2: calmer\.<\/p>/);
});

// --- Snaps ---------------------------------------------------------------------------------

test("a snap's meta/snap.yaml is read", () => {
  assert.deepEqual(snapInfo(snapYaml()), {
    name: 'hashi',
    version: '1.2.0',
    architectures: ['amd64'],
    base: 'core24',
    confinement: 'strict',
    grade: 'stable',
  });
  // Inline, quoted
  const inline = snapInfo('name: "hashi"\nversion: 1.2.0\narchitectures: [arm64]\nbase: core24\n');
  assert.deepEqual(inline.architectures, ['arm64']);
  assert.equal(inline.name, 'hashi');
  assert.equal(inline.confinement, '');
});

test("a snap's problems", () => {
  const expected = { name: 'hashi', version: '1.2.0' };
  assert.deepEqual(snapProblems(snapInfo(snapYaml()), expected), []);
  assert.deepEqual(snapProblems(snapInfo(snapYaml({ arch: 'arm64' })), expected), []);
  const wrong = snapInfo(
    snapYaml({ name: 'hashi-puzzle', version: '1.1.0', arch: 'armhf' })
      .replace('core24', 'core22')
      .replace('strict', 'devmode')
      .replace('grade: stable', 'grade: devel'),
  );
  const problems = snapProblems(wrong, expected);
  assert.equal(problems.length, 6, problems.join('\n'));
  assert.match(problems.join('\n'), /"hashi-puzzle", not "hashi"/);
  assert.match(problems.join('\n'), /version is 1\.1\.0/);
  assert.match(problems.join('\n'), /for armhf/);
  assert.match(problems.join('\n'), /core22/);
  assert.match(problems.join('\n'), /devmode/);
  assert.match(problems.join('\n'), /devel/);
  // Two architectures in one snap
  const both = { ...snapInfo(snapYaml()), architectures: ['amd64', 'arm64'] };
  assert.match(snapProblems(both, expected)[0] ?? '', /amd64, arm64/);
  assert.equal(snapArch('x64'), 'amd64');
  assert.equal(snapArch('arm64'), 'arm64');
});

test("the store's revisions are read by the table's columns", () => {
  assert.deepEqual(revisionsOf(TABLE), [
    { revision: 7, arches: ['arm64'], version: '1.2.0', channels: [] },
    { revision: 6, arches: ['amd64'], version: '1.2.0', channels: ['edge*'] },
    {
      revision: 5,
      arches: ['amd64', 'arm64'],
      version: '1.1.0',
      channels: ['stable*', 'candidate*'],
    },
  ]);
  assert.deepEqual(revisionsOf('No revisions found for hashi.\n'), []);
  assert.deepEqual(revisionsOf('Rev.  Uploaded  Arches  Version  Channels\n'), []);
  assert.throws(() => revisionsOf('Something else'), /no table/);
});

test('a release uploads, releases what the store has, or does nothing', () => {
  const revisions = revisionsOf(TABLE);
  const at = (arch: string, channel = 'stable', version = '1.2.0') =>
    plan(revisions, { version, arch, channel });
  // Neither uploaded yet: 1.2.0 for amd64 is revision 6, for arm64 7; stable has 1.1.0
  assert.deepEqual(at('amd64'), { action: 'release', revision: 6 });
  assert.deepEqual(at('arm64'), { action: 'release', revision: 7 });
  assert.deepEqual(at('amd64', 'edge'), { action: 'nothing', revision: 6 });
  assert.deepEqual(at('amd64', 'stable', '1.3.0'), { action: 'upload' });
  assert.deepEqual(at('amd64', 'stable', '1.1.0'), { action: 'nothing', revision: 5 });
  assert.throws(() => at('arm64', 'stable', '1.0.9'), /stable has 1\.1\.0 for arm64 .*newer/);
  assert.deepEqual(plan([], { version: '1.2.0', arch: 'amd64', channel: 'stable' }), {
    action: 'upload',
  });
  // The newest revision of the version, when the store has two
  const twice: Revision[] = [
    { revision: 8, arches: ['amd64'], version: '1.2.0', channels: [] },
    ...revisions,
  ];
  assert.deepEqual(plan(twice, { version: '1.2.0', arch: 'amd64', channel: 'beta' }), {
    action: 'release',
    revision: 8,
  });
});

test('the snaps are checked through unsquashfs, one per architecture', () => {
  const amd = { 'unsquashfs -cat a.snap meta/snap.yaml': snapYaml() };
  const arm = { 'unsquashfs -cat b.snap meta/snap.yaml': snapYaml({ arch: 'arm64' }) };
  const { run } = fake({ ...amd, ...arm });
  const snaps = checkSnaps(run, ['a.snap', 'b.snap'], { name: 'hashi', version: '1.2.0' });
  assert.deepEqual(
    snaps.map((s) => [s.file, s.arch]),
    [
      ['a.snap', 'amd64'],
      ['b.snap', 'arm64'],
    ],
  );
  assert.throws(
    () => checkSnaps(fake({ ...amd }).run, ['a.snap'], { name: 'hashi', version: '1.3.0' }),
    /a\.snap: its version is 1\.2\.0, not 1\.3\.0/,
  );
  const same = fake({ 'unsquashfs -cat': snapYaml() });
  assert.throws(
    () => checkSnaps(same.run, ['a.snap', 'c.snap'], { name: 'hashi', version: '1.2.0' }),
    /two snaps for amd64/,
  );
  const broken = fake({ unsquashfs: { status: 1, stderr: 'not a squashfs' } });
  assert.throws(
    () => checkSnaps(broken.run, ['a.snap'], { name: 'hashi', version: '1.2.0' }),
    /unsquashfs -cat a\.snap meta\/snap\.yaml failed \(1\): not a squashfs/,
  );
});

test('a release runs snapcraft for each snap', () => {
  const dir = tempDir();
  const files = ['hashi-1.3.0-linux-x64.snap', 'hashi-1.3.0-linux-arm64.snap'].map((f) => {
    fs.writeFileSync(path.join(dir, f), 'snap');
    return path.join(dir, f);
  });
  const table = `${TABLE.trimEnd()}\n9       2026-11-13T10:00:00Z  arm64          1.3.0      -\n`;
  const { run, calls } = fake({
    'snapcraft list-revisions hashi': table,
    'snapcraft upload': "Revision 10 created for 'hashi' and released to 'stable'\n",
    'snapcraft release hashi 9 stable': '',
  });
  const snaps = [
    { file: files[0] ?? '', info: snapInfo(snapYaml()), arch: 'amd64' },
    { file: files[1] ?? '', info: snapInfo(snapYaml({ arch: 'arm64' })), arch: 'arm64' },
  ];
  const summary = release(
    run,
    snaps,
    { name: 'hashi', version: '1.3.0', channel: 'stable' },
    quiet,
  );
  assert.deepEqual(calls, [
    'snapcraft list-revisions hashi',
    `snapcraft upload --release=stable ${files[0]}`,
    'snapcraft release hashi 9 stable',
  ]);
  assert.deepEqual(summary, [
    "Snap Store: 1.3.0 for amd64 uploaded as revision 10, released to stable once the store's review passes",
    'Snap Store: revision 9 (1.3.0, arm64) released to stable',
  ]);
  // Already there: nothing
  const again = fake({ 'snapcraft list-revisions hashi': TABLE });
  assert.deepEqual(
    release(
      again.run,
      snaps.slice(0, 1),
      { name: 'hashi', version: '1.2.0', channel: 'edge' },
      quiet,
    ),
    ['Snap Store: 1.2.0 for amd64 is in edge already (revision 6), nothing to do'],
  );
  assert.deepEqual(again.calls, ['snapcraft list-revisions hashi']);
  // snapcraft's failure is the release's
  const refused = fake({
    'snapcraft list-revisions': TABLE,
    'snapcraft upload': { status: 2, stderr: 'Snap name not registered' },
  });
  assert.throws(
    () =>
      release(
        refused.run,
        snaps.slice(0, 1),
        { name: 'hashi', version: '1.3.0', channel: 'stable' },
        quiet,
      ),
    /snapcraft upload --release=stable .* failed \(2\): Snap name not registered/,
  );
});

test('the name comes from SNAP_NAME, hashi without it', () => {
  assert.equal(snapName({}), 'hashi');
  assert.equal(snapName({ SNAP_NAME: ' ' }), 'hashi');
  assert.equal(snapName({ SNAP_NAME: 'hashi-puzzle' }), 'hashi-puzzle');
  for (const wrong of ['Hashi', 'hashi_puzzle', '-hashi', 'ha--shi', '2026', 'x'.repeat(41)])
    assert.throws(() => snapName({ SNAP_NAME: wrong }), /isn't a snap name/, wrong);
});

test('release checks the snaps, and uploads only with the login and not in a dry run', () => {
  const answers = {
    'unsquashfs -cat a.snap meta/snap.yaml': snapYaml({ version: '1.3.0' }),
    'snapcraft list-revisions hashi': TABLE,
    'snapcraft upload': 'Revision 10 created\n',
  };
  const dry = fake(answers);
  assert.deepEqual(main(['release', '--version', '1.3.0', '--dry-run', 'a.snap'], {}, dry.run), [
    'Snap Store, dry run: a.snap (amd64) checked (hashi 1.3.0, core24, strict), would go to stable',
  ]);
  assert.deepEqual(dry.calls, ['unsquashfs -cat a.snap meta/snap.yaml']);
  assert.throws(
    () => main(['release', '--version', '1.3.0', 'a.snap'], {}, fake(answers).run),
    /SNAPCRAFT_STORE_CREDENTIALS not set/,
  );
  assert.throws(
    () =>
      main(
        ['release', '--version', '1.3.0', '--channel', 'nightly', 'a.snap'],
        {},
        fake(answers).run,
      ),
    /channel nightly isn't one of stable, candidate, beta, edge/,
  );
  assert.throws(
    () =>
      main(
        ['release', '--version', '1.3.0', '--dry-run', 'a.snap'],
        { SNAP_NAME: 'hashi-puzzle' },
        fake(answers).run,
      ),
    /"hashi", not "hashi-puzzle"/,
  );
});
