// The fonts of the social preview images (src/server/og), cut down to the characters they draw.
//
//   pnpm run og-fonts
//
// The server draws the images with resvg, which reads TrueType/OpenType but not the woff2 files
// the site serves. This takes the same @fontsource packages as scripts/fonts.ts, keeps only the
// glyphs of the texts on the images in every language, and writes one TrueType collection per
// font to src/server/og/fonts (committed, a few hundred KB together; the server embeds them, so
// it needs no node_modules to build or test). The packages split every font into slices by
// unicode-range; a collection holds the subset of each slice that is used.
//
// Run it again when a text on the images changes: the og tests (`every_text_has_its_glyphs`)
// fail when a language draws a character its fonts don't have.

import fs from 'node:fs';
import path from 'node:path';
import subsetFont from 'subset-font';

const root = path.resolve(import.meta.dirname, '..');
const modules = path.join(root, 'node_modules');
const out = path.join(root, 'src/server/og/fonts');

// The stylesheets of each font, as in scripts/fonts.ts. The keys are the file names, the server
// lists the same ones (FONTS in src/server/og/mod.rs).
const FONTS: Record<string, string[]> = {
  // The pixel font: the name, island numbers, kanji, sizes
  dotgothic16: ['@fontsource/dotgothic16/400.css'],
  // Titles, where the site uses it for headings (see --display-font in style/tailwind.css)
  'kaisei-decol': ['@fontsource/kaisei-decol/700.css'],
  // Body text, and every language's fallback for Latin and digits
  murecho: ['@fontsource-variable/murecho/wght.css'],
  'be-vietnam-pro': [600, 700, 800].map((w) => `@fontsource/be-vietnam-pro/${w}.css`),
  'noto-sans-kr': ['@fontsource-variable/noto-sans-kr/wght.css'],
  'noto-sans-sc': ['@fontsource-variable/noto-sans-sc/wght.css'],
  'noto-sans-tc': ['@fontsource-variable/noto-sans-tc/wght.css'],
  'noto-sans-arabic': ['@fontsource-variable/noto-sans-arabic/wght.css'],
  'noto-sans-devanagari': ['@fontsource-variable/noto-sans-devanagari/wght.css'],
  'noto-sans-thai': ['@fontsource-variable/noto-sans-thai/wght.css'],
};

// The text font of the languages Murecho doesn't cover, as `script_font` in src/lang/mod.rs
const TEXT_FONT: Partial<Record<string, string>> = {
  ko: 'noto-sans-kr',
  zh: 'noto-sans-sc',
  'zh-Hant': 'noto-sans-tc',
  ar: 'noto-sans-arabic',
  hi: 'noto-sans-devanagari',
  th: 'noto-sans-thai',
  vi: 'be-vietnam-pro',
};

const LOCALES = fs.readdirSync(path.join(root, 'locales')).sort();
const DIGITS = '0123456789';

// What the pixel font draws in every language (src/server/og/card.rs and model/calendar.rs)
const PIXEL = `Hashi 橋をかけろ · HASHIWOKAKERO hashi.rs #×${DIGITS}月火水木金土日初級中級上級達人鬼共島`;

function strings(lang: string, namespace: string): unknown {
  const file = path.join(root, 'locales', lang, `${namespace}.json`);
  return JSON.parse(fs.readFileSync(file, 'utf8'));
}

const isObject = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null;

// A key of a language, English where it has none (as leptos_i18n does)
function lookup(lang: string, namespace: string, key: string): unknown {
  const get = (json: unknown) =>
    key.split('.').reduce<unknown>((o, k) => (isObject(o) ? o[k] : undefined), json);
  const value = get(strings(lang, namespace)) ?? get(strings('en', namespace));
  if (value === undefined) throw new Error(`no ${namespace}.${key}`);
  return value;
}

/** A text of a language: a string, or an object of them (the weekdays, the months). */
function text(lang: string, namespace: string, key: string): string {
  const value = lookup(lang, namespace, key);
  if (typeof value === 'string') return value;
  if (isObject(value)) {
    return Object.values(value)
      .map((v) => {
        if (typeof v !== 'string') throw new Error(`${namespace}.${key} of ${lang}: not texts`);
        return v;
      })
      .join(' ');
  }
  throw new Error(`${namespace}.${key} of ${lang}: not a text`);
}

// The translated texts on the images of a language, placeholders removed: what the text font
// draws, and the daily's title, which is in the heading font
function texts(lang: string): { text: string; title: string } {
  const t = (key: string) => text(lang, 'common', key);
  const all = [
    t('og.tagline'),
    t('og.free'),
    t('og.daily'),
    t('og.unique'),
    t('date_long'),
    t('weekday'),
    t('month'),
    t('difficulty'),
    // A shared puzzle's card (`shared_card` in src/server/og/mod.rs)
    text(lang, 'shared', 'og.kicker'),
    DIGITS,
  ].join(' ');
  const title = [
    text(lang, 'daily', 'title'),
    text(lang, 'shared', 'og.beat'),
    text(lang, 'shared', 'og.solve'),
    `${DIGITS}:`,
  ].join(' ');
  const strip = (s: string) => s.replace(/\{\{[^}]*\}\}/g, '');
  return { text: strip(all), title: strip(title) };
}

// The characters each font has to have: everything a language draws in it, also as a fallback
const needed = new Map(Object.keys(FONTS).map((f) => [f, new Set<string>()]));
const add = (font: string, s: string) => {
  const chars = needed.get(font);
  if (!chars) throw new Error(`no font ${font} in FONTS`);
  for (const c of s) chars.add(c);
};
add('dotgothic16', PIXEL);
for (const lang of LOCALES) {
  const { text, title } = texts(lang);
  const font = TEXT_FONT[lang] ?? 'murecho';
  add(font, text + title);
  // Murecho backs up every language, as in the site's font stacks
  add('murecho', text + title);
  // The titles' font where the site's headings use it, but not for Cyrillic, Polish and Turkish
  // (see `families` in src/server/og/mod.rs)
  // Its ° is as wide as a kanji, `n° 4` looks torn apart: Murecho draws it
  if (font === 'murecho' && !['ru', 'uk', 'pl', 'tr'].includes(lang))
    add('kaisei-decol', title.replace('°', ''));
}

// unicode-range: U+0000-00FF,U+0131 -> a test for code points
function inRange(ranges: string): (c: string) => boolean {
  const parts = ranges.split(',').map((r): [number, number] => {
    const [a = '', b] = r.trim().replace(/^U\+/i, '').split('-');
    const lo = Number.parseInt(a.replace(/\?/g, '0'), 16);
    const hi = b ? Number.parseInt(b, 16) : Number.parseInt(a.replace(/\?/g, 'f'), 16);
    return [lo, hi];
  });
  return (c) => {
    const cp = c.codePointAt(0) ?? -1;
    return parts.some(([lo, hi]) => cp >= lo && cp <= hi);
  };
}

// The slices of a stylesheet: file and unicode-range
function slices(source: string): { file: string; covers: (c: string) => boolean }[] {
  const file = path.join(modules, source);
  const css = fs.readFileSync(file, 'utf8');
  return [...css.matchAll(/@font-face\s*{([^}]*)}/g)].map(([, body = '']) => {
    const url = body.match(/url\(\.\/files\/([^)]+\.woff2)\)/)?.[1];
    if (!url) throw new Error(`${source}: a @font-face without a woff2 file`);
    const range = body.match(/unicode-range:\s*([^;]+);/)?.[1] ?? 'U+0-10FFFF';
    return { file: path.join(path.dirname(file), 'files', url), covers: inRange(range) };
  });
}

// Fonts in one TrueType collection: each font's tables copied as they are, their offsets moved
function collection(fonts: Buffer[]): Buffer {
  const header = 12 + 4 * fonts.length;
  const align = (n: number) => (n + 3) & ~3;
  let offset = align(header);
  const offsets = fonts.map((f) => {
    const at = offset;
    offset = align(offset + f.length);
    return at;
  });
  const ttc = Buffer.alloc(offset);
  ttc.write('ttcf', 0, 'latin1');
  ttc.writeUInt32BE(0x00010000, 4);
  ttc.writeUInt32BE(fonts.length, 8);
  fonts.forEach((font, i) => {
    const at = offsets[i] ?? 0;
    ttc.writeUInt32BE(at, 12 + 4 * i);
    font.copy(ttc, at);
    const tables = font.readUInt16BE(4);
    for (let t = 0; t < tables; t++) {
      const record = at + 12 + 16 * t;
      ttc.writeUInt32BE(ttc.readUInt32BE(record + 8) + at, record + 8);
    }
  });
  return ttc;
}

fs.rmSync(out, { recursive: true, force: true });
fs.mkdirSync(out, { recursive: true });
let total = 0;
for (const [name, sources] of Object.entries(FONTS)) {
  // Every character but controls (the bidi marks have no glyphs)
  const chars = [...(needed.get(name) ?? [])].filter((c) => !/[\p{Cc}\p{Cf}]/u.test(c));
  const subsets: Buffer[] = [];
  for (const source of sources) {
    for (const slice of slices(source)) {
      const text = chars.filter(slice.covers).join('');
      if (!text) continue;
      const bytes = fs.readFileSync(slice.file);
      // Space too: resvg lays out the gaps between words with it
      subsets.push(await subsetFont(bytes, ` ${text}`, { targetFormat: 'sfnt' }));
    }
  }
  const file = path.join(out, `${name}.ttc`);
  fs.writeFileSync(file, collection(subsets));
  const size = fs.statSync(file).size;
  total += size;
  console.log(
    `${name}: ${chars.length} characters, ${subsets.length} slices, ${(size / 1024).toFixed(0)} KB`,
  );
}
console.log(`og fonts: ${(total / 1024).toFixed(0)} KB in src/server/og/fonts`);
