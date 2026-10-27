// Checks the store listings in fastlane/metadata against the stores' limits and rules
// (app/README.md, Store listings). `node store/validate.mjs` prints every problem and fails when
// there is one; CI runs it, and scripts/tests/store-listing.test.mjs tests the rules.
//
// Google Play: title ≤ 30, short description ≤ 80, full description ≤ 4000 characters.
// App Store: name and subtitle ≤ 30 characters, keywords ≤ 100 bytes (UTF-8; App Store Connect
// says characters, bytes are the safe reading for Chinese, Japanese and Korean), promotional text
// ≤ 170 and description ≤ 4000 characters.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { APPLE, APPLE_KNOWN, METADATA, PLAY, PLAY_KNOWN, PLAY_METADATA, SITE_LANGUAGES, siteUrl } from './locales.mjs';

export const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

export const PLAY_FIELDS = { title: 30, short_description: 80, full_description: 4000 };
export const APPLE_FIELDS = {
  name: 30,
  subtitle: 30,
  keywords: 100,
  promotional_text: 170,
  description: 4000,
  support_url: 255,
  marketing_url: 255,
  privacy_url: 255,
};
/** Captions of the screenshots, store/captions.json: short, they sit above the app. */
export const CAPTION_LIMIT = 34;
export const SUBCAPTION_LIMIT = 44;
export const SCENES = ['daily', 'ranking', 'practice', 'friends', 'stamps', 'night'];

/**
 * Names of other games and puzzle brands, of the stores and their platforms: not in keywords
 * (App Store Review Guideline 2.3.7, Play's metadata policy), nor in titles. Lower case.
 */
export const FORBIDDEN_TERMS = [
  'sudoku', 'nikoli', 'kakuro', 'kenken', 'picross', 'nonogram', 'wordle', 'tetris', 'flow free',
  'two dots', 'candy crush', 'nyt', 'new york times', 'apple', 'iphone', 'ipad', 'android',
  'google', 'app store', 'play store',
];
/** Ranking, price and calls to action that the stores refuse in names and short texts. */
export const PROMO_WORDS = [
  /#\s*1\b/, /\bno\.?\s*1\b/i, /\bbest\b/i, /\btop\b/i, /\bfree\b/i, /\bsale\b/i, /\bdiscount\b/i,
  /\bnew\b/i, /\bdownload now\b/i, /\binstall now\b/i, /\bplay now\b/i, /\btry now\b/i,
];
/** Words for "free"/"best"/"#1" in the other languages, for names and titles. */
const PROMO_LOCAL = [
  'gratis', 'kostenlos', 'gratuit', 'gratuito', 'grátis', 'darmo', 'ücretsiz', 'бесплатн',
  'безкоштовн', 'مجان', 'मुफ़्त', 'miễn phí', 'ฟรี', '無料', '무료', '免费', '免費', 'beste', 'meilleur',
  'migliore', 'mejor', 'melhor', 'najlepsz', 'en iyi', 'лучш', 'найкращ',
];

const EMOJI = /\p{Extended_Pictographic}/u;
/** Scripts written without spaces between words: keywords are compared as substrings. */
const UNSPACED = /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Thai}]/u;

const read = (file) => (fs.existsSync(file) ? fs.readFileSync(file, 'utf8').trim() : undefined);
const chars = (text) => [...text].length;
const bytes = (text) => Buffer.byteLength(text, 'utf8');

/** Reads every listing of fastlane/metadata: `{ play: { locale: fields }, apple: {...}, dirs }`. */
export function loadListings(root = ROOT) {
  const listDirs = (dir) =>
    fs.existsSync(dir)
      ? fs.readdirSync(dir, { withFileTypes: true }).filter((d) => d.isDirectory()).map((d) => d.name)
      : [];
  const play = {};
  for (const locale of listDirs(path.join(root, PLAY_METADATA))) {
    play[locale] = Object.fromEntries(
      Object.keys(PLAY_FIELDS).map((f) => [f, read(path.join(root, PLAY_METADATA, locale, `${f}.txt`))]),
    );
  }
  const apple = {};
  for (const locale of listDirs(path.join(root, METADATA)).filter((d) => d !== 'android')) {
    apple[locale] = Object.fromEntries(
      Object.keys(APPLE_FIELDS).map((f) => [f, read(path.join(root, METADATA, locale, `${f}.txt`))]),
    );
  }
  const captionsFile = path.join(root, 'store/captions.json');
  const captions = fs.existsSync(captionsFile) ? JSON.parse(fs.readFileSync(captionsFile, 'utf8')) : {};
  return { play, apple, copyright: read(path.join(root, METADATA, 'copyright.txt')), captions };
}

/** Every problem of the listings, as messages; empty when everything is fine. */
export function validate(listings, { play = PLAY, apple = APPLE } = {}) {
  const errors = [];
  const fail = (where, message) => errors.push(`${where}: ${message}`);

  checkLocales('Google Play', play, PLAY_KNOWN, listings.play, PLAY_FIELDS, fail);
  checkLocales('App Store', apple, APPLE_KNOWN, listings.apple, APPLE_FIELDS, fail);

  for (const { locale, same } of play) {
    const listing = listings.play[locale];
    if (same || !listing) continue;
    const where = `Google Play ${locale}`;
    checkLimits(where, listing, PLAY_FIELDS, fail);
    checkName(`${where} title`, listing.title, fail);
    const short = listing.short_description ?? '';
    if (/\n/.test(short)) fail(where, 'the short description is one line');
    if (EMOJI.test(short)) fail(where, 'no emoji in the short description');
    for (const p of PROMO_WORDS) if (p.test(short)) fail(where, `no promotional wording in the short description (${p})`);
    checkMentions(where, `${listing.title}\n${short}\n${listing.full_description}`, ['iphone', 'ipad', 'app store', 'ios'], fail);
  }

  for (const { locale, site, same } of apple) {
    const listing = listings.apple[locale];
    if (same || !listing) continue;
    const where = `App Store ${locale}`;
    checkLimits(where, listing, APPLE_FIELDS, fail);
    checkName(`${where} name`, listing.name, fail);
    checkName(`${where} subtitle`, listing.subtitle, fail);
    if (listing.keywords !== undefined) checkKeywords(where, listing, fail);
    checkMentions(where, `${listing.name}\n${listing.subtitle}\n${listing.description}\n${listing.promotional_text}`, ['android', 'google play', 'play store'], fail);
    const urls = {
      support_url: siteUrl(site, '/kontakt'),
      marketing_url: siteUrl(site, '/'),
      privacy_url: siteUrl(site, '/datenschutz'),
    };
    for (const [field, expected] of Object.entries(urls)) {
      if (listing[field] !== undefined && listing[field] !== expected) {
        fail(where, `${field} is ${listing[field]}, expected ${expected}`);
      }
    }
  }

  if (!listings.copyright || !/^\d{4} \S/.test(listings.copyright)) {
    fail('App Store', 'copyright.txt is "<year> <owner>"');
  }

  checkCaptions(listings.captions ?? {}, fail);
  return errors;
}

function checkLocales(store, table, known, present, fields, fail) {
  const byLocale = new Map(table.map((l) => [l.locale, l]));
  for (const { locale, site, same } of table) {
    if (!known.includes(locale)) fail(`${store} ${locale}`, 'not a locale the store (fastlane) knows');
    if (!SITE_LANGUAGES.includes(site)) fail(`${store} ${locale}`, `no site language ${site}`);
    if (same) {
      if (present[locale]) fail(`${store} ${locale}`, `takes the texts of ${same}, remove its folder`);
      const source = byLocale.get(same);
      if (!source || source.same) fail(`${store} ${locale}`, `${same} is not a locale with texts of its own`);
      continue;
    }
    if (!present[locale]) {
      fail(`${store} ${locale}`, 'missing');
      continue;
    }
    for (const field of Object.keys(fields)) {
      if (!present[locale][field]) fail(`${store} ${locale}`, `${field}.txt is missing or empty`);
    }
  }
  for (const locale of Object.keys(present)) {
    if (!byLocale.has(locale)) fail(`${store} ${locale}`, 'a folder for a locale that store/locales.mjs does not list');
  }
  for (const site of SITE_LANGUAGES) {
    if (!table.some((l) => l.site === site)) fail(store, `no listing in the site's language ${site}`);
  }
}

function checkLimits(where, listing, limits, fail) {
  for (const [field, limit] of Object.entries(limits)) {
    const value = listing[field];
    if (value === undefined) continue;
    const length = field === 'keywords' ? bytes(value) : chars(value);
    const unit = field === 'keywords' ? 'bytes' : 'characters';
    if (length > limit) fail(where, `${field} has ${length} ${unit}, at most ${limit}`);
  }
}

/** Titles, names and subtitles: no emoji, promotion, other brands, shouting or decoration. */
function checkName(where, value, fail) {
  if (!value) return;
  if (/\n/.test(value)) fail(where, 'one line');
  if (EMOJI.test(value)) fail(where, 'no emoji');
  if (/([!?.*~_-])\1/.test(value)) fail(where, 'no repeated punctuation');
  for (const p of PROMO_WORDS) if (p.test(value)) fail(where, `no promotional wording (${p})`);
  const lower = value.toLowerCase();
  for (const w of PROMO_LOCAL) if (lower.includes(w)) fail(where, `no promotional wording (${w})`);
  for (const t of FORBIDDEN_TERMS) if (lower.includes(t)) fail(where, `no other brands (${t})`);
  for (const word of value.split(/[\s:·–-]+/)) {
    if (word.length > 3 && /\p{Lu}/u.test(word) && word === word.toUpperCase() && !/\p{Ll}/u.test(word)) {
      fail(where, `no words in capitals (${word})`);
    }
  }
}

/**
 * The keywords: comma separated without spaces around the commas, no empty or repeated entries,
 * nothing the name or the subtitle already has (they count for search on their own), no other
 * brands.
 */
function checkKeywords(where, listing, fail) {
  const keywords = listing.keywords;
  if (/\n/.test(keywords)) fail(where, 'keywords are one line');
  if (/\s,|,\s/.test(keywords)) fail(where, 'no spaces around the commas of the keywords');
  const terms = keywords.split(',');
  if (terms.some((t) => t.trim() === '')) fail(where, 'an empty keyword (two commas, or one at an end)');
  const seen = new Set();
  const nameWords = words(`${listing.name ?? ''} ${listing.subtitle ?? ''}`);
  const nameText = `${listing.name ?? ''} ${listing.subtitle ?? ''}`.toLowerCase();
  for (const term of terms.map((t) => t.trim().toLowerCase()).filter(Boolean)) {
    if (seen.has(term)) fail(where, `keyword "${term}" twice`);
    seen.add(term);
    for (const t of FORBIDDEN_TERMS) if (term.includes(t)) fail(where, `keyword "${term}": no other brands`);
    if (UNSPACED.test(term)) {
      if (nameText.includes(term)) fail(where, `keyword "${term}" is in the name or subtitle already`);
    } else {
      for (const w of words(term)) {
        if (nameWords.has(w)) fail(where, `keyword "${term}": "${w}" is in the name or subtitle already`);
      }
    }
  }
  if (/\b(app|game|games)\b/i.test(keywords)) fail(where, 'keywords: "app" and category names count anyway');
}

const words = (text) =>
  new Set(
    text
      .toLowerCase()
      .split(/[^\p{L}\p{M}\p{N}]+/u)
      .filter((w) => w.length > 0),
  );

function checkMentions(where, text, terms, fail) {
  const lower = text.toLowerCase();
  for (const t of terms) {
    if (new RegExp(`(^|[^\\p{L}])${t}([^\\p{L}]|$)`, 'u').test(lower)) fail(where, `mentions another platform (${t})`);
  }
}

function checkCaptions(captions, fail) {
  for (const site of SITE_LANGUAGES) {
    const lang = captions[site];
    if (!lang) {
      fail('store/captions.json', `no captions in ${site}`);
      continue;
    }
    for (const scene of [...SCENES, 'feature']) {
      const c = lang[scene];
      if (!c?.title) {
        fail(`store/captions.json ${site}`, `no caption for ${scene}`);
        continue;
      }
      if (chars(c.title.replace(/\*/g, '')) > CAPTION_LIMIT) fail(`store/captions.json ${site}`, `${scene}: caption over ${CAPTION_LIMIT} characters`);
      if (c.text && chars(c.text) > SUBCAPTION_LIMIT) {
        fail(`store/captions.json ${site}`, `${scene}: text over ${SUBCAPTION_LIMIT} characters`);
      }
      for (const p of PROMO_WORDS.slice(0, 2)) if (p.test(c.title)) fail(`store/captions.json ${site}`, `${scene}: no ranking claims`);
      if (EMOJI.test(`${c.title}${c.text ?? ''}`)) fail(`store/captions.json ${site}`, `${scene}: no emoji`);
    }
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const errors = validate(loadListings());
  if (errors.length > 0) {
    for (const e of errors) console.error(`✗ ${e}`);
    console.error(`\n${errors.length} problem(s) in the store listings`);
    process.exit(1);
  }
  const listings = loadListings();
  console.log(
    `Store listings OK: Google Play ${Object.keys(listings.play).length} + ${PLAY.filter((l) => l.same).length} copied locales, ` +
      `App Store ${Object.keys(listings.apple).length} + ${APPLE.filter((l) => l.same).length} copied locales`,
  );
}
