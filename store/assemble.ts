// Puts the upload tree together for fastlane (app/README.md, Store listings):
//
//   target/store/fastlane/android/<locale>/       supply's metadata_path: the texts, and with
//                                                  images images/ (icon, feature graphic, screenshots)
//   target/store/fastlane/apple/metadata/<locale>/ deliver's metadata_path, with release_notes.txt
//                                                  (What's New) from app/store/whatsnew
//   target/store/fastlane/apple/screenshots/<locale>/ deliver's screenshots_path, with images
//
// from fastlane/metadata (the texts; locales with `same` in store/locales.ts get a copy of
// another's) and target/store/images (`pnpm run store:images`). Checks every image's size and
// that the stores' screenshots have no alpha channel.
//
//   node store/assemble.ts            texts and images
//   node store/assemble.ts --text     texts only (no images needed)
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { APPLE, METADATA, PLAY, PLAY_METADATA, type StoreLocale } from './locales.ts';
import { APPLE_OTHER_PLATFORMS, loadListings, mentions, ROOT, validate } from './validate.ts';

export const OUT = path.join(ROOT, 'target/store/fastlane');
export const IMAGES = path.join(ROOT, 'target/store/images');
/** The app icon of the website and the app (app/src-tauri/app-icon.json), Play's hi-res icon. */
export const ICON = path.join(ROOT, 'public/icon-512.png');

/** Every image of a locale: [folder of the store tree, source frame folder, width, height]. */
type Shots = readonly (readonly [folder: string, frame: string, width: number, height: number])[];
export const PLAY_SHOTS: Shots = [
  ['phoneScreenshots', 'play-phone', 1080, 1920],
  ['sevenInchScreenshots', 'play-tablet-7', 1080, 1920],
  ['tenInchScreenshots', 'play-tablet-10', 1440, 2560],
];
export const APPLE_SHOTS: Shots = [
  ['iphone', 'iphone-69', 1320, 2868],
  ['ipad', 'ipad-13', 2064, 2752],
];

export interface PngInfo {
  width: number;
  height: number;
  bitDepth: number;
  colorType: number;
}

/** Width, height and colour type of a PNG (2 RGB, 6 RGBA, ...), from its header. */
export function pngInfo(file: string): PngInfo {
  const head = Buffer.alloc(26);
  const fd = fs.openSync(file, 'r');
  try {
    fs.readSync(fd, head, 0, 26, 0);
  } finally {
    fs.closeSync(fd);
  }
  if (head.toString('latin1', 1, 4) !== 'PNG' || head.toString('latin1', 12, 16) !== 'IHDR') {
    throw new Error(`${file} is not a PNG`);
  }
  return {
    width: head.readUInt32BE(16),
    height: head.readUInt32BE(20),
    bitDepth: head.readUInt8(24),
    colorType: head.readUInt8(25),
  };
}

/** Problems of an image: its size, and an alpha channel where the store refuses one. */
export function checkImage(
  file: string,
  {
    width,
    height,
    alpha = false,
  }: {
    width: number;
    height: number;
    /** Whether it may have an alpha channel; 'required': it must. */
    alpha?: boolean | 'required';
  },
): string[] {
  if (!fs.existsSync(file)) return [`${path.relative(ROOT, file)} is missing`];
  const info = pngInfo(file);
  const problems: string[] = [];
  if (info.width !== width || info.height !== height) {
    problems.push(
      `${path.relative(ROOT, file)} is ${info.width}×${info.height}, expected ${width}×${height}`,
    );
  }
  // Colour types 4 and 6 have an alpha channel
  const hasAlpha = info.colorType === 4 || info.colorType === 6;
  if (hasAlpha && !alpha)
    problems.push(`${path.relative(ROOT, file)} has an alpha channel, the store refuses it`);
  if (alpha === 'required' && !hasAlpha)
    problems.push(`${path.relative(ROOT, file)} should be a 32-bit PNG`);
  return problems;
}

/** The release notes of the version, `whatsnew-<locale>` in Google Play's locale codes. */
export const WHATSNEW = path.join(ROOT, 'app/store/whatsnew');
/** App Store Connect's limit of "What's New in This Version", in characters. */
export const RELEASE_NOTES_LIMIT = 4000;

/**
 * The release notes of an App Store locale in `dir`: its own `whatsnew-<locale>`, else the one of
 * the Google Play locale with texts in the same language when there is only one (whatsnew-it-IT
 * for it, whatsnew-ja-JP for ja), else whatsnew-en-US. Undefined when there is none.
 */
export function releaseNotesFile(
  dir: string,
  { locale, site }: StoreLocale,
  play: readonly StoreLocale[] = PLAY,
): string | undefined {
  const sameLanguage = play.filter((l) => l.site === site && !l.same);
  const candidates = [
    locale,
    ...(sameLanguage.length === 1 ? sameLanguage.map((l) => l.locale) : []),
    'en-US',
  ];
  return candidates.map((l) => path.join(dir, `whatsnew-${l}`)).find((file) => fs.existsSync(file));
}

/**
 * Writes each App Store locale's release_notes.txt (deliver's "What's New") into `metadata`, from
 * the release notes in `whatsnew`. Notes that name another platform (Google Play, Play Games,
 * Android: Apple turns those away) are left out, so deliver leaves App Store Connect's text as it
 * is; notes over Apple's limit are refused. Returns how many it wrote, and a note for each file
 * left out.
 */
export function writeReleaseNotes(
  metadata: string,
  {
    whatsnew = WHATSNEW,
    apple = APPLE,
    play = PLAY,
  }: { whatsnew?: string; apple?: readonly StoreLocale[]; play?: readonly StoreLocale[] } = {},
): { written: number; notes: string[] } {
  const skipped = new Map<string, { names: string[]; locales: string[] }>();
  let written = 0;
  for (const store of apple) {
    const file = releaseNotesFile(whatsnew, store, play);
    if (file === undefined) continue;
    const text = fs.readFileSync(file, 'utf8').trim();
    if (text === '') continue;
    const length = [...text].length;
    if (length > RELEASE_NOTES_LIMIT)
      throw new Error(
        `${path.relative(ROOT, file)} has ${length} characters, the App Store takes ${RELEASE_NOTES_LIMIT}`,
      );
    const names = mentions(text, APPLE_OTHER_PLATFORMS);
    if (names.length > 0) {
      const entry = skipped.get(file) ?? { names, locales: [] };
      entry.locales.push(store.locale);
      skipped.set(file, entry);
      continue;
    }
    fs.writeFileSync(path.join(metadata, store.locale, 'release_notes.txt'), text);
    written++;
  }
  const notes = [...skipped].map(
    ([file, { names, locales }]) =>
      `${path.relative(ROOT, file)} names ${names.join(', ')}: the App Store's What's New stays as it is in ${locales.length === apple.length ? 'every locale' : locales.join(', ')}`,
  );
  return { written, notes };
}

const copyText = (from: string, to: string) =>
  fs.writeFileSync(to, fs.readFileSync(from, 'utf8').trim());

export function assemble({ images = true }: { images?: boolean } = {}): {
  play: number;
  apple: number;
  /** The App Store locales that get release notes, and why others don't. */
  releaseNotes: { written: number; notes: string[] };
} {
  const errors = validate(loadListings());
  if (errors.length > 0)
    throw new Error(
      `the listings have problems, see node store/validate.ts:\n${errors.join('\n')}`,
    );
  fs.rmSync(OUT, { recursive: true, force: true });
  const problems: string[] = [];
  const shots = (dir: string) =>
    fs.existsSync(dir)
      ? fs
          .readdirSync(dir)
          .filter((f) => f.endsWith('.png'))
          .sort((a, b) => Number.parseInt(a, 10) - Number.parseInt(b, 10))
      : [];

  for (const { locale, site, same } of PLAY) {
    const dir = path.join(OUT, 'android', locale);
    fs.mkdirSync(dir, { recursive: true });
    const source = path.join(ROOT, PLAY_METADATA, same ?? locale);
    // supply sends the files as they are: no trailing newline in a title
    for (const file of fs.readdirSync(source))
      copyText(path.join(source, file), path.join(dir, file));
    if (!images) continue;
    const img = path.join(dir, 'images');
    fs.mkdirSync(img, { recursive: true });
    fs.copyFileSync(ICON, path.join(img, 'icon.png'));
    problems.push(
      ...checkImage(path.join(img, 'icon.png'), { width: 512, height: 512, alpha: 'required' }),
    );
    const feature = path.join(IMAGES, site, 'feature.png');
    problems.push(...checkImage(feature, { width: 1024, height: 500 }));
    if (fs.existsSync(feature)) fs.copyFileSync(feature, path.join(img, 'featureGraphic.png'));
    for (const [folder, frame, width, height] of PLAY_SHOTS) {
      const files = shots(path.join(IMAGES, site, frame));
      // At most 8 per kind, at least 2 for phones (4 for tablets to be shown to them)
      if (files.length < 4 || files.length > 8)
        problems.push(
          `Google Play ${locale} ${folder}: ${files.length} screenshots, 4 to 8 expected`,
        );
      fs.mkdirSync(path.join(img, folder), { recursive: true });
      for (const f of files) {
        const from = path.join(IMAGES, site, frame, f);
        problems.push(...checkImage(from, { width, height }));
        fs.copyFileSync(from, path.join(img, folder, f));
      }
    }
  }

  const appleMeta = path.join(OUT, 'apple', 'metadata');
  fs.mkdirSync(appleMeta, { recursive: true });
  copyText(path.join(ROOT, METADATA, 'copyright.txt'), path.join(appleMeta, 'copyright.txt'));
  for (const { locale, site, same } of APPLE) {
    const dir = path.join(appleMeta, locale);
    fs.mkdirSync(dir, { recursive: true });
    const source = path.join(ROOT, METADATA, same ?? locale);
    for (const file of fs.readdirSync(source))
      copyText(path.join(source, file), path.join(dir, file));
    if (!images) continue;
    const out = path.join(OUT, 'apple', 'screenshots', locale);
    fs.mkdirSync(out, { recursive: true });
    for (const [device, frame, width, height] of APPLE_SHOTS) {
      const files = shots(path.join(IMAGES, site, frame));
      if (files.length < 3 || files.length > 10)
        problems.push(
          `App Store ${locale} ${device}: ${files.length} screenshots, 3 to 10 expected`,
        );
      for (const f of files) {
        const from = path.join(IMAGES, site, frame, f);
        problems.push(...checkImage(from, { width, height }));
        // deliver tells the devices apart by size and keeps the order of the names
        fs.copyFileSync(from, path.join(out, `${device}-${f}`));
      }
    }
  }
  const releaseNotes = writeReleaseNotes(appleMeta);
  if (problems.length > 0) throw new Error(`the images have problems:\n${problems.join('\n')}`);
  return { play: PLAY.length, apple: APPLE.length, releaseNotes };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const text = process.argv.includes('--text');
  try {
    const { play, apple, releaseNotes } = assemble({ images: !text });
    console.log(
      `${path.relative(ROOT, OUT)}: Google Play ${play} locales, App Store ${apple} locales (${releaseNotes.written} with release notes), ${text ? 'texts only' : 'texts and images'}`,
    );
    for (const note of releaseNotes.notes) console.log(note);
  } catch (e) {
    console.error(e instanceof Error ? e.message : e);
    process.exit(1);
  }
}
