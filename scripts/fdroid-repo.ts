// Puts together the working directory of our F-Droid repository for fdroidserver (app/README.md,
// F-Droid repository), everything but the APKs, which the release job adds to <dir>/repo:
//
//   <dir>/config.yml                    app/fdroid/config.yml (the keys come from the environment)
//   <dir>/metadata/rs.hashi.yml         app/fdroid/rs.hashi.yml
//   <dir>/metadata/rs.hashi/<locale>/   Google Play's listing in each of its locales
//                                       (store/locales.ts, with the copies of `same`): the texts,
//                                       and in images/ the icon (en-US), the feature graphic and
//                                       the phone screenshots. No changelog: fdroidserver only
//                                       shows those of versions it built itself (the metadata's
//                                       Changelog links to the GitHub releases)
//   <dir>/icon.png                      the repository's icon, the app's (fdroidserver copies it
//                                       into repo/icons/)
//
// The images are made in CI (store/capture, the "Store listing" workflow), not committed: they
// come from a folder like target/store/images (--images, `pnpm run store:images`), else from the
// repository as it is published now (--carry <repo URL>, its index-v2.json), so a release without
// fresh images keeps the screenshots it had.
//
//   node scripts/fdroid-repo.ts <dir> [--images <dir>] [--carry <repo URL>]
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { PLAY, PLAY_METADATA } from '../store/locales.ts';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
export const APP_ID = 'rs.hashi';
const FDROID = path.join(ROOT, 'app/fdroid');
const ICON = path.join(ROOT, 'public/icon-512.png');
/** Play's texts, by F-Droid's names for them (title, summary, description). */
const TEXTS = ['title.txt', 'short_description.txt', 'full_description.txt'];

/** An image of the published repository: its locale, where it goes, and its path in the repo. */
export interface Carried {
  locale: string;
  /** Relative to the locale's images/ folder. */
  file: string;
  /** Relative to the repository's URL, e.g. `/rs.hashi/en-US/phoneScreenshots/1-daily.png`. */
  name: string;
}

interface FileEntry {
  name: string;
}

/** A locale's map of the index, or nothing when it isn't one. */
function locales<T>(value: unknown): [string, T][] {
  return value !== null && typeof value === 'object'
    ? Object.entries(value as Record<string, T>)
    : [];
}

/** The feature graphics and phone screenshots of the app in an index-v2.json. */
export function carriedImages(index: unknown): Carried[] {
  const metadata = (
    index as { packages?: Record<string, { metadata?: Record<string, unknown> }> } | null
  )?.packages?.[APP_ID]?.metadata;
  if (metadata === undefined) return [];
  const images: Carried[] = [];
  for (const [locale, entry] of locales<FileEntry>(metadata.featureGraphic)) {
    if (typeof entry?.name === 'string')
      images.push({ locale, file: 'featureGraphic.png', name: entry.name });
  }
  const screenshots = metadata.screenshots as { phone?: unknown } | undefined;
  for (const [locale, entries] of locales<FileEntry[]>(screenshots?.phone)) {
    if (!Array.isArray(entries)) continue;
    for (const entry of entries) {
      if (typeof entry?.name !== 'string') continue;
      images.push({
        locale,
        file: path.posix.join('phoneScreenshots', path.posix.basename(entry.name)),
        name: entry.name,
      });
    }
  }
  // Only plain names, in the app's folder of their locale: nothing of the index may write outside
  // the working directory or fetch from elsewhere. fdroidserver names a feature graphic by its
  // hash (featureGraphic_<base64url>.png), a screenshot as it was.
  return images.filter((i) => {
    if (!/^[A-Za-z0-9-]+$/.test(i.locale)) return false;
    const folder = `/${APP_ID}/${i.locale}/`;
    if (!i.name.startsWith(folder)) return false;
    const rest = i.name.slice(folder.length);
    return i.file === 'featureGraphic.png'
      ? /^featureGraphic(_[\w=-]+)?\.png$/.test(rest)
      : /^phoneScreenshots\/\w[\w-]*(\.\w+)*\.png$/.test(rest) && rest === i.file;
  });
}

/** The screenshots of a folder of frames, in their order (`1-daily.png`, `2-ranks.png`, ...). */
function shots(dir: string): string[] {
  if (!fs.existsSync(dir)) return [];
  return fs
    .readdirSync(dir)
    .filter((f) => f.endsWith('.png'))
    .sort((a, b) => Number.parseInt(a, 10) - Number.parseInt(b, 10));
}

export interface Summary {
  locales: number;
  /** Locales with phone screenshots, and where they came from. */
  screenshots: { images: number; carried: number };
}

export async function assembleFdroid({
  out,
  images,
  carry,
  fetchFile = async (url) => {
    const response = await fetch(url);
    if (!response.ok) throw new Error(`${url}: ${response.status}`);
    return Buffer.from(await response.arrayBuffer());
  },
}: {
  out: string;
  images?: string;
  carry?: string;
  /** The body of a URL; tests replace it. */
  fetchFile?: (url: string) => Promise<Buffer>;
}): Promise<Summary> {
  const metadata = path.join(out, 'metadata');
  const app = path.join(metadata, APP_ID);
  fs.mkdirSync(app, { recursive: true });
  fs.mkdirSync(path.join(out, 'repo'), { recursive: true });
  fs.copyFileSync(path.join(FDROID, 'config.yml'), path.join(out, 'config.yml'));
  // fdroidserver refuses a config others can read: it holds the keys' settings
  fs.chmodSync(path.join(out, 'config.yml'), 0o600);
  fs.copyFileSync(path.join(FDROID, `${APP_ID}.yml`), path.join(metadata, `${APP_ID}.yml`));
  fs.copyFileSync(ICON, path.join(out, 'icon.png'));

  const summary: Summary = { locales: 0, screenshots: { images: 0, carried: 0 } };
  const withShots = new Set<string>();
  for (const { locale, site, same } of PLAY) {
    const dir = path.join(app, locale);
    fs.mkdirSync(dir, { recursive: true });
    const source = path.join(ROOT, PLAY_METADATA, same ?? locale);
    for (const file of TEXTS) {
      fs.writeFileSync(
        path.join(dir, file),
        `${fs.readFileSync(path.join(source, file), 'utf8').trim()}\n`,
      );
    }
    summary.locales++;
    const img = path.join(dir, 'images');
    fs.mkdirSync(img, { recursive: true });
    if (locale === 'en-US') fs.copyFileSync(ICON, path.join(img, 'icon.png'));
    if (images === undefined) continue;
    const feature = path.join(images, site, 'feature.png');
    if (fs.existsSync(feature)) fs.copyFileSync(feature, path.join(img, 'featureGraphic.png'));
    const phone = shots(path.join(images, site, 'play-phone'));
    if (phone.length === 0) continue;
    fs.mkdirSync(path.join(img, 'phoneScreenshots'), { recursive: true });
    for (const f of phone)
      fs.copyFileSync(
        path.join(images, site, 'play-phone', f),
        path.join(img, 'phoneScreenshots', f),
      );
    withShots.add(locale);
    summary.screenshots.images++;
  }

  if (carry !== undefined) {
    const base = carry.replace(/\/+$/, '');
    let index: unknown = null;
    try {
      index = JSON.parse((await fetchFile(`${base}/index-v2.json`)).toString('utf8'));
    } catch (e) {
      // The first release: nothing published yet
      console.warn(`no images carried over from ${base}: ${e instanceof Error ? e.message : e}`);
    }
    const known = new Set(PLAY.map((l) => l.locale));
    const carried = new Set<string>();
    for (const image of carriedImages(index)) {
      if (!known.has(image.locale) || withShots.has(image.locale)) continue;
      const to = path.join(app, image.locale, 'images', image.file);
      fs.mkdirSync(path.dirname(to), { recursive: true });
      fs.writeFileSync(to, await fetchFile(`${base}${image.name}`));
      if (image.file.startsWith('phoneScreenshots/')) carried.add(image.locale);
    }
    summary.screenshots.carried = carried.size;
  }
  return summary;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  const option = (name: string) => {
    const i = args.indexOf(name);
    if (i === -1) return undefined;
    const value = args[i + 1];
    if (value === undefined) throw new Error(`${name} needs a value`);
    args.splice(i, 2);
    return value;
  };
  try {
    const images = option('--images');
    const carry = option('--carry');
    const [out] = args;
    if (out === undefined || args.length > 1)
      throw new Error(
        'usage: node scripts/fdroid-repo.ts <dir> [--images <dir>] [--carry <repo URL>]',
      );
    const summary = await assembleFdroid({
      out: path.resolve(out),
      ...(images === undefined ? {} : { images: path.resolve(images) }),
      ...(carry === undefined ? {} : { carry }),
    });
    console.log(
      `${out}: ${summary.locales} locales, phone screenshots in ${summary.screenshots.images} locales from the images and ${summary.screenshots.carried} carried over`,
    );
  } catch (e) {
    console.error(e instanceof Error ? e.message : e);
    process.exit(1);
  }
}
