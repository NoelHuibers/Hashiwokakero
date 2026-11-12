// The Linux app in the Snap Store (docs/stores.md, Snap Store), and the AppStream MetaInfo of the
// Linux app:
//
//   node scripts/snap-store.ts metainfo --version <x.y.z> --date <YYYY-MM-DD> [--whatsnew <file>]
//                                       --out <file>
//   node scripts/snap-store.ts release --version <x.y.z> [--channel <channel>] [--dry-run]
//                                      <snap>...
//
// metainfo: rs.hashi.metainfo.xml, what software centres (GNOME Software, KDE Discover) show of
// an app, from Google Play's listing (fastlane/metadata/android: the title's part after "Hashi: "
// as the summary, the description, in each of the app's languages), the release's version, date
// and notes, the content rating, the links and the brand colours. The release puts it into the
// .deb and the .rpm (/usr/share/metainfo), and the snap takes its summary and description from
// there (snapcraft's adopt-info). One source of truth: the store texts.
//
// release: each snap (one per architecture, from app/linux/snap/snapcraft.yaml) is checked (its
// meta/snap.yaml: the name, the version, one architecture, core24, strict, stable) and uploaded
// with snapcraft into the channel (default stable), which releases it to it once the Snap Store
// has reviewed it. A rerun uploads nothing twice: a revision of this version and architecture
// the store has already is released into the channel instead, and nothing happens when the
// channel has it. It refuses when the channel has a newer version.
//
// --dry-run: the snaps' checks and the plan, without snapcraft.
//
// SNAP_NAME is the snap's registered name (default hashi). snapcraft logs in with
// SNAPCRAFT_STORE_CREDENTIALS (`snapcraft export-login`, docs/stores.md).
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { PLAY_METADATA } from '../store/locales.ts';
import { otherStores } from './galaxy-store.ts';
import { compareVersions } from './microsoft-store.ts';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
export const APP_ID = 'rs.hashi';
/** The desktop entry Tauri's .deb and .rpm install (after productName). */
export const DESKTOP_ID = 'Hashi.desktop';
export const REPOSITORY = 'https://github.com/NoelHuibers/Hashiwokakero';
export const DEFAULT_NAME = 'hashi';
export const CHANNELS = ['stable', 'candidate', 'beta', 'edge'] as const;
/** The snap's summary: at most 78 characters. */
export const SUMMARY_LENGTH = 78;
/** The snap's description: at most 4096 characters. */
export const DESCRIPTION_LENGTH = 4096;

// --- MetaInfo ------------------------------------------------------------------------------

/**
 * The MetaInfo's languages: Google Play's locale whose texts it gets, and its `xml:lang` (the
 * gettext names software centres look up). English is the untranslated text; Latin American
 * Spanish (es-419) has no language of its own there, Spain's is `es`.
 */
export const LANGUAGES: readonly { lang: string | null; play: string }[] = [
  { lang: null, play: 'en-US' },
  { lang: 'de', play: 'de-DE' },
  { lang: 'fr', play: 'fr-FR' },
  { lang: 'it', play: 'it-IT' },
  { lang: 'es', play: 'es-ES' },
  { lang: 'pt_BR', play: 'pt-BR' },
  { lang: 'pt_PT', play: 'pt-PT' },
  { lang: 'nl', play: 'nl-NL' },
  { lang: 'pl', play: 'pl-PL' },
  { lang: 'tr', play: 'tr-TR' },
  { lang: 'ru', play: 'ru-RU' },
  { lang: 'uk', play: 'uk' },
  { lang: 'ar', play: 'ar' },
  { lang: 'hi', play: 'hi-IN' },
  { lang: 'id', play: 'id' },
  { lang: 'vi', play: 'vi' },
  { lang: 'th', play: 'th' },
  { lang: 'ja', play: 'ja-JP' },
  { lang: 'ko', play: 'ko-KR' },
  { lang: 'zh_CN', play: 'zh-CN' },
  { lang: 'zh_TW', play: 'zh-TW' },
];

export const escapeXml = (text: string) =>
  text
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');

/** A block of a description: a paragraph, or a list of points. */
export type Block = { p: string } | { ul: string[] };

/**
 * Google Play's description as AppStream has descriptions: paragraphs and lists. Lines that start
 * with "•" are a list's points, the other lines of a block (a heading, a paragraph) a paragraph
 * each; blank lines separate blocks.
 */
export function descriptionBlocks(text: string): Block[] {
  const blocks: Block[] = [];
  for (const chunk of text.trim().split(/\n\s*\n/)) {
    let list: string[] | null = null;
    for (const raw of chunk.split('\n')) {
      const line = raw.trim();
      if (line === '') continue;
      if (line.startsWith('•')) {
        if (list === null) {
          list = [];
          blocks.push({ ul: list });
        }
        list.push(line.replace(/^•\s*/, ''));
      } else {
        list = null;
        blocks.push({ p: line });
      }
    }
  }
  return blocks;
}

/** The blocks as plain text, as the Snap Store shows a description, for its length. */
export function plainText(blocks: Block[]): string {
  return blocks
    .map((b) => ('p' in b ? b.p : b.ul.map((point) => `- ${point}`).join('\n')))
    .join('\n\n');
}

/** The summary of a store title "Hashi: Daily Bridges Puzzle": what follows the app's name. */
export function summaryOf(title: string): string {
  const match = /^Hashi\s*[:：]\s*(.+)$/u.exec(title.trim());
  if (match?.[1] === undefined) throw new Error(`the title "${title}" isn't "Hashi: <summary>"`);
  return match[1].trim().replace(/[.。]$/u, '');
}

export interface Texts {
  lang: string | null;
  play: string;
  summary: string;
  blocks: Block[];
}

/** The texts of each language, from Google Play's listing. */
export function metainfoTexts(metadata = path.join(ROOT, PLAY_METADATA)): Texts[] {
  const read = (locale: string, file: string) =>
    fs.readFileSync(path.join(metadata, locale, file), 'utf8').trim();
  return LANGUAGES.map(({ lang, play }) => ({
    lang,
    play,
    summary: summaryOf(read(play, 'title.txt')),
    blocks: descriptionBlocks(read(play, 'full_description.txt')),
  }));
}

/** The problems of a language's texts with the Snap Store's limits (and other stores' names). */
export function textProblems(t: Texts): string[] {
  const problems: string[] = [];
  const where = t.play;
  if ([...t.summary].length > SUMMARY_LENGTH)
    problems.push(`${where}: the summary has ${[...t.summary].length} characters, at most 78`);
  const plain = plainText(t.blocks);
  if ([...plain].length > DESCRIPTION_LENGTH)
    problems.push(`${where}: the description has ${[...plain].length} characters, at most 4096`);
  if (t.blocks.length === 0) problems.push(`${where}: the description is empty`);
  const stores = otherStores(`${t.summary}\n${plain}`);
  if (stores.length > 0) problems.push(`${where}: names ${stores.join(', ')}`);
  return problems;
}

export interface Release {
  version: string;
  /** YYYY-MM-DD */
  date: string;
  /** The release notes, or none. */
  notes: string;
}

/**
 * The MetaInfo of the Linux app: the texts of every language, the release (its notes only when
 * they name no other store: the 1.1 notes are about Google Play Games).
 */
export function metainfo(texts: Texts[], release: Release): string {
  if (!/^\d+\.\d+\.\d+$/.test(release.version))
    throw new Error(`the version ${release.version} isn't x.y.z`);
  if (!/^\d{4}-\d{2}-\d{2}$/.test(release.date))
    throw new Error(`the date ${release.date} isn't YYYY-MM-DD`);
  const english = texts.find((t) => t.lang === null);
  if (english === undefined) throw new Error('no English texts');
  const ordered = [english, ...texts.filter((t) => t.lang !== null)];
  const attr = (t: Texts) => (t.lang === null ? '' : ` xml:lang="${t.lang}"`);
  const summaries = ordered.map((t) => `  <summary${attr(t)}>${escapeXml(t.summary)}</summary>`);
  const description = ordered.flatMap((t) =>
    t.blocks.flatMap((b) =>
      'p' in b
        ? [`    <p${attr(t)}>${escapeXml(b.p)}</p>`]
        : [
            // The list itself is never translated, only its points (appstreamcli)
            '    <ul>',
            ...b.ul.map((point) => `      <li${attr(t)}>${escapeXml(point)}</li>`),
            '    </ul>',
          ],
    ),
  );
  const notes = release.notes.trim();
  const releaseNotes =
    notes === '' || otherStores(notes).length > 0
      ? []
      : ['      <description>', `        <p>${escapeXml(notes)}</p>`, '      </description>'];
  return `${[
    '<?xml version="1.0" encoding="UTF-8"?>',
    '<!-- Made by scripts/snap-store.ts metainfo from the store texts (fastlane/metadata/android) -->',
    '<component type="desktop-application">',
    `  <id>${APP_ID}</id>`,
    '  <metadata_license>MIT</metadata_license>',
    '  <project_license>MIT</project_license>',
    '  <name>Hashi</name>',
    ...summaries,
    `  <developer id="${APP_ID}">`,
    '    <name>Noel Huibers</name>',
    '  </developer>',
    '  <description>',
    ...description,
    '  </description>',
    `  <launchable type="desktop-id">${DESKTOP_ID}</launchable>`,
    '  <url type="homepage">https://hashi.rs</url>',
    `  <url type="bugtracker">${REPOSITORY}/issues</url>`,
    '  <url type="contact">https://hashi.rs/kontakt</url>',
    `  <url type="vcs-browser">${REPOSITORY}</url>`,
    '  <categories>',
    '    <category>Game</category>',
    '    <category>LogicGame</category>',
    '  </categories>',
    '  <keywords>',
    ...['Hashiwokakero', 'Bridges', 'Puzzle', 'Logic', 'Daily'].map(
      (k) => `    <keyword>${k}</keyword>`,
    ),
    '  </keywords>',
    '  <provides>',
    '    <binary>hashi</binary>',
    '  </provides>',
    '  <requires>',
    '    <display_length compare="ge">360</display_length>',
    '  </requires>',
    '  <supports>',
    '    <control>pointing</control>',
    '    <control>keyboard</control>',
    '    <control>touch</control>',
    '  </supports>',
    '  <branding>',
    '    <color type="primary" scheme_preference="light">#f4ecdd</color>',
    '    <color type="primary" scheme_preference="dark">#101827</color>',
    '  </branding>',
    // Rankings, groups and rivals show the players' names and times, without a chat
    '  <content_rating type="oars-1.1">',
    '    <content_attribute id="social-chat">mild</content_attribute>',
    '    <content_attribute id="social-info">mild</content_attribute>',
    '  </content_rating>',
    '  <releases>',
    `    <release version="${release.version}" date="${release.date}">`,
    ...releaseNotes,
    `      <url type="details">${REPOSITORY}/releases/tag/app-v${release.version}</url>`,
    '    </release>',
    '  </releases>',
    '</component>',
  ].join('\n')}\n`;
}

// --- Snaps ---------------------------------------------------------------------------------

/** What a snap's meta/snap.yaml says. */
export interface SnapInfo {
  name: string;
  version: string;
  architectures: string[];
  base: string;
  confinement: string;
  grade: string;
}

const unquote = (v: string) => v.trim().replace(/^(['"])(.*)\1$/, '$2');

/**
 * The fields of meta/snap.yaml that the checks need: top-level scalars, and the list of
 * architectures (`- amd64` lines below it, or `[amd64]`). snapcraft writes it, so this subset
 * of YAML is enough.
 */
export function snapInfo(yaml: string): SnapInfo {
  const scalars = new Map<string, string>();
  const architectures: string[] = [];
  let inArchitectures = false;
  for (const line of yaml.split('\n')) {
    const top = /^([a-z][a-z0-9-]*):(.*)$/.exec(line);
    if (top?.[1] !== undefined) {
      const value = (top[2] ?? '').trim();
      inArchitectures = top[1] === 'architectures' && value === '';
      if (top[1] === 'architectures' && value.startsWith('['))
        architectures.push(
          ...value
            .slice(1, value.lastIndexOf(']'))
            .split(',')
            .map(unquote)
            .filter((a) => a !== ''),
        );
      else if (value !== '') scalars.set(top[1], unquote(value));
      continue;
    }
    const item = /^\s*-\s*(.+)$/.exec(line);
    if (inArchitectures && item?.[1] !== undefined) architectures.push(unquote(item[1]));
    else if (!/^\s/.test(line) && line.trim() !== '') inArchitectures = false;
  }
  const field = (name: string) => scalars.get(name) ?? '';
  return {
    name: field('name'),
    version: field('version'),
    architectures,
    base: field('base'),
    confinement: field('confinement'),
    grade: field('grade'),
  };
}

/** The snap's architecture of a release file's: x64 is amd64. */
export const snapArch = (arch: string) => (arch === 'x64' ? 'amd64' : arch);

/** What is wrong with a snap for the release of `version` under `name`. */
export function snapProblems(info: SnapInfo, expected: { name: string; version: string }) {
  const problems: string[] = [];
  if (info.name !== expected.name)
    problems.push(`the snap is called "${info.name}", not "${expected.name}" (SNAP_NAME)`);
  if (info.version !== expected.version)
    problems.push(`its version is ${info.version || 'missing'}, not ${expected.version}`);
  if (info.architectures.length !== 1 || !['amd64', 'arm64'].includes(info.architectures[0] ?? ''))
    problems.push(
      `it is for ${info.architectures.join(', ') || 'no architecture'}, not one of amd64 and arm64`,
    );
  if (info.base !== 'core24') problems.push(`its base is ${info.base || 'missing'}, not core24`);
  if (info.confinement !== 'strict')
    problems.push(`its confinement is ${info.confinement || 'missing'}, not strict`);
  if (info.grade !== 'stable')
    problems.push(`its grade is ${info.grade || 'missing'}, not stable (devel can't go to stable)`);
  return problems;
}

/** A revision of the snap in the store, from `snapcraft list-revisions`. */
export interface Revision {
  revision: number;
  arches: string[];
  version: string;
  /** Its channels without the track (`stable`), `*` marking the ones it is current in. */
  channels: string[];
}

/**
 * The table of `snapcraft list-revisions <name>`:
 *
 *     Rev.    Uploaded              Arches       Version    Channels
 *     3       2026-11-12T10:00:00Z  arm64        1.2.0      latest/stable*, latest/edge
 *
 * Read by the header's columns (Arches and Channels may hold spaces after commas).
 */
export function revisionsOf(table: string): Revision[] {
  const lines = table.split('\n').filter((l) => l.trim() !== '');
  const headerAt = lines.findIndex((l) => /^\s*Rev\.?\s/.test(l));
  if (headerAt === -1) {
    if (lines.some((l) => /no revisions/i.test(l))) return [];
    throw new Error(`snapcraft list-revisions printed no table:\n${table.trim()}`);
  }
  const header = lines[headerAt] ?? '';
  const column = (name: string) => {
    const at = header.indexOf(name);
    if (at === -1) throw new Error(`snapcraft list-revisions has no column ${name}: ${header}`);
    return at;
  };
  const starts = {
    rev: 0,
    uploaded: column('Uploaded'),
    arches: column('Arches'),
    version: column('Version'),
    channels: column('Channels'),
  };
  const list = (v: string) =>
    v
      .split(',')
      .map((s) => s.trim())
      .filter((s) => s !== '' && s !== '-');
  return lines.slice(headerAt + 1).map((line) => {
    const cell = (from: number, to?: number) => line.slice(from, to).trim();
    const revision = Number(cell(starts.rev, starts.uploaded));
    if (!Number.isInteger(revision)) throw new Error(`not a revision: ${line}`);
    return {
      revision,
      arches: list(cell(starts.arches, starts.version)),
      version: cell(starts.version, starts.channels),
      channels: list(cell(starts.channels)).map((c) => c.replace(/^latest\//, '')),
    };
  });
}

export type Step =
  | { action: 'upload' }
  | { action: 'release'; revision: number }
  | { action: 'nothing'; revision: number };

/**
 * What to do with the snap of `version` for `arch` in `channel`, given the store's revisions:
 * nothing when the channel has it already, release the revision the store has of it (an upload
 * of a run before), else upload. Refuses when the channel has a newer version.
 */
export function plan(
  revisions: Revision[],
  o: { version: string; arch: string; channel: string },
): Step {
  const ofArch = revisions.filter((r) => r.arches.includes(o.arch));
  const current = ofArch.find((r) => r.channels.includes(`${o.channel}*`));
  if (current !== undefined) {
    if (current.version === o.version) return { action: 'nothing', revision: current.revision };
    if (compareVersions(current.version, o.version) > 0)
      throw new Error(
        `the channel ${o.channel} has ${current.version} for ${o.arch} (revision ${current.revision}), newer than ${o.version}`,
      );
  }
  const uploaded = ofArch
    .filter((r) => r.version === o.version)
    .sort((a, b) => b.revision - a.revision)[0];
  if (uploaded !== undefined) return { action: 'release', revision: uploaded.revision };
  return { action: 'upload' };
}

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

/** Runs it and fails with its output when it fails. Its standard output. */
function must(run: Run, program: string, args: readonly string[]): string {
  const r = run(program, args);
  if (r.status !== 0)
    throw new Error(
      `${program} ${args.join(' ')} failed (${r.status}): ${`${r.stderr}\n${r.stdout}`.trim()}`,
    );
  return r.stdout;
}

/** A snap file, checked. */
export interface Snap {
  file: string;
  info: SnapInfo;
  arch: string;
}

/** Reads and checks each snap (its meta/snap.yaml, through unsquashfs). */
export function checkSnaps(
  run: Run,
  files: string[],
  expected: { name: string; version: string },
): Snap[] {
  const snaps = files.map((file) => {
    const info = snapInfo(must(run, 'unsquashfs', ['-cat', file, 'meta/snap.yaml']));
    const problems = snapProblems(info, expected);
    if (problems.length > 0) throw new Error(`${path.basename(file)}: ${problems.join('; ')}`);
    return { file, info, arch: info.architectures[0] ?? '' };
  });
  const arches = snaps.map((s) => s.arch);
  const twice = arches.filter((a, i) => arches.indexOf(a) !== i);
  if (twice.length > 0) throw new Error(`two snaps for ${twice.join(', ')}`);
  return snaps;
}

export interface ReleaseOptions {
  name: string;
  version: string;
  channel: string;
}

/** Uploads or releases each snap into the channel (see the top). Its summary. */
export function release(run: Run, snaps: Snap[], o: ReleaseOptions, log = console.log): string[] {
  const revisions = revisionsOf(must(run, 'snapcraft', ['list-revisions', o.name]));
  const summary: string[] = [];
  for (const snap of snaps) {
    const step = plan(revisions, { version: o.version, arch: snap.arch, channel: o.channel });
    const file = path.basename(snap.file);
    switch (step.action) {
      case 'nothing':
        summary.push(
          `Snap Store: ${o.version} for ${snap.arch} is in ${o.channel} already (revision ${step.revision}), nothing to do`,
        );
        break;
      case 'release':
        log(`${file}: the store has ${o.version} for ${snap.arch} as revision ${step.revision}`);
        must(run, 'snapcraft', ['release', o.name, String(step.revision), o.channel]);
        summary.push(
          `Snap Store: revision ${step.revision} (${o.version}, ${snap.arch}) released to ${o.channel}`,
        );
        break;
      case 'upload': {
        log(`${file}: uploading (${(fs.statSync(snap.file).size / 1048576).toFixed(1)} MB)`);
        const out = must(run, 'snapcraft', ['upload', `--release=${o.channel}`, snap.file]);
        log(out.trim());
        const revision = /Revision (\d+)/i.exec(out)?.[1];
        summary.push(
          `Snap Store: ${o.version} for ${snap.arch} uploaded${revision === undefined ? '' : ` as revision ${revision}`}, released to ${o.channel} once the store's review passes`,
        );
        break;
      }
      default: {
        const never: never = step;
        throw new Error(`unknown step ${JSON.stringify(never)}`);
      }
    }
  }
  return summary;
}

/** The snap's name from the environment (SNAP_NAME), checked as the store checks names. */
export function snapName(env: NodeJS.ProcessEnv): string {
  const name = env.SNAP_NAME?.trim() || DEFAULT_NAME;
  const ok = name.length <= 40 && /^[a-z0-9]+(-[a-z0-9]+)*$/.test(name) && /[a-z]/.test(name);
  if (!ok)
    throw new Error(
      `SNAP_NAME "${name}" isn't a snap name (lower-case letters, digits and single hyphens, at most 40)`,
    );
  return name;
}

// --- Command line --------------------------------------------------------------------------

const USAGE = `usage: node scripts/snap-store.ts metainfo --version <x.y.z> --date <YYYY-MM-DD> [--whatsnew <file>] --out <file>
       node scripts/snap-store.ts release --version <x.y.z> [--channel <channel>] [--dry-run] <snap>...`;

function usage(): never {
  console.error(USAGE);
  process.exit(2);
}

export function main(args: string[], env: NodeJS.ProcessEnv, run: Run = spawn): string[] {
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

  if (command === 'metainfo') {
    const version = option('--version');
    const date = option('--date');
    const whatsnew = option('--whatsnew');
    const out = option('--out');
    if (version === undefined || date === undefined || out === undefined || args.length > 0)
      usage();
    const texts = metainfoTexts();
    const problems = texts.flatMap(textProblems);
    if (problems.length > 0) throw new Error(`the store texts don't fit:\n${problems.join('\n')}`);
    const notes = whatsnew === undefined ? '' : fs.readFileSync(whatsnew, 'utf8').trim();
    fs.mkdirSync(path.dirname(out), { recursive: true });
    fs.writeFileSync(out, metainfo(texts, { version, date, notes }));
    return [`${out}: the MetaInfo of ${version} (${date}) in ${texts.length} languages`];
  }

  if (command === 'release') {
    const dryRun = flag('--dry-run');
    const version = option('--version');
    const channel = option('--channel') ?? 'stable';
    if (version === undefined || args.length === 0) usage();
    if (!(CHANNELS as readonly string[]).includes(channel))
      throw new Error(`the channel ${channel} isn't one of ${CHANNELS.join(', ')}`);
    const name = snapName(env);
    const snaps = checkSnaps(run, args, { name, version });
    const checked = snaps.map((s) => `${path.basename(s.file)} (${s.arch})`).join(', ');
    if (dryRun)
      return [
        `Snap Store, dry run: ${checked} checked (${name} ${version}, core24, strict), would go to ${channel}`,
      ];
    if (!env.SNAPCRAFT_STORE_CREDENTIALS) throw new Error('SNAPCRAFT_STORE_CREDENTIALS not set');
    return release(run, snaps, { name, version, channel });
  }
  usage();
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    const summary = main(process.argv.slice(2), process.env);
    for (const line of summary) console.log(line);
    if (process.env.GITHUB_STEP_SUMMARY)
      fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${summary.join('\n\n')}\n`);
  } catch (e) {
    console.log(`::error::${e instanceof Error ? e.message : e}`);
    process.exit(1);
  }
}
