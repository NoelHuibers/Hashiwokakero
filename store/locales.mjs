// The store languages of the listings (app/README.md, Store listings): which locales each store
// gets, and which of the site's languages (locales/) their screenshots are taken in.
//
// A locale with `text` has its own texts in fastlane/metadata. One with `same` takes the texts of
// another locale of the same store: `node store/assemble.mjs` copies them into the upload tree,
// so the repository keeps one copy (fr-CA is fr-FR's French, en-GB the en-US English).
//
// Codes: Google Play's are fastlane supply's (Supply::Languages, `-` instead of `_`), Apple's
// are App Store Connect's (FastlaneCore::Languages::ALL_LANGUAGES, what deliver accepts).

/** The site's languages, the folders of locales/. */
export const SITE_LANGUAGES = [
  'en', 'de', 'fr', 'it', 'es', 'pt', 'nl', 'pl', 'tr', 'ru',
  'uk', 'ar', 'hi', 'id', 'vi', 'th', 'ja', 'ko', 'zh', 'zh-Hant',
];

/** Google Play. The default language of the app is en-US: other locales fall back to it. */
export const PLAY = [
  { locale: 'en-US', site: 'en' },
  { locale: 'de-DE', site: 'de' },
  { locale: 'fr-FR', site: 'fr' },
  { locale: 'fr-CA', site: 'fr', same: 'fr-FR' },
  { locale: 'it-IT', site: 'it' },
  { locale: 'es-ES', site: 'es' },
  { locale: 'es-419', site: 'es' },
  { locale: 'es-US', site: 'es', same: 'es-419' },
  { locale: 'pt-BR', site: 'pt' },
  { locale: 'pt-PT', site: 'pt' },
  { locale: 'nl-NL', site: 'nl' },
  { locale: 'pl-PL', site: 'pl' },
  { locale: 'tr-TR', site: 'tr' },
  { locale: 'ru-RU', site: 'ru' },
  { locale: 'uk', site: 'uk' },
  { locale: 'ar', site: 'ar' },
  { locale: 'hi-IN', site: 'hi' },
  { locale: 'id', site: 'id' },
  { locale: 'vi', site: 'vi' },
  { locale: 'th', site: 'th' },
  { locale: 'ja-JP', site: 'ja' },
  { locale: 'ko-KR', site: 'ko' },
  { locale: 'zh-CN', site: 'zh' },
  { locale: 'zh-TW', site: 'zh-Hant' },
  { locale: 'zh-HK', site: 'zh-Hant', same: 'zh-TW' },
];

/** The App Store. The primary language of the app record is en-US. */
export const APPLE = [
  { locale: 'en-US', site: 'en' },
  { locale: 'en-GB', site: 'en', same: 'en-US' },
  { locale: 'en-AU', site: 'en', same: 'en-US' },
  { locale: 'en-CA', site: 'en', same: 'en-US' },
  { locale: 'de-DE', site: 'de' },
  { locale: 'fr-FR', site: 'fr' },
  { locale: 'fr-CA', site: 'fr', same: 'fr-FR' },
  { locale: 'it', site: 'it' },
  { locale: 'es-ES', site: 'es' },
  { locale: 'es-MX', site: 'es' },
  { locale: 'pt-BR', site: 'pt' },
  { locale: 'pt-PT', site: 'pt' },
  { locale: 'nl-NL', site: 'nl' },
  { locale: 'pl', site: 'pl' },
  { locale: 'tr', site: 'tr' },
  { locale: 'ru', site: 'ru' },
  { locale: 'uk', site: 'uk' },
  { locale: 'ar-SA', site: 'ar' },
  { locale: 'hi', site: 'hi' },
  { locale: 'id', site: 'id' },
  { locale: 'vi', site: 'vi' },
  { locale: 'th', site: 'th' },
  { locale: 'ja', site: 'ja' },
  { locale: 'ko', site: 'ko' },
  { locale: 'zh-Hans', site: 'zh' },
  { locale: 'zh-Hant', site: 'zh-Hant' },
];

/** Every locale fastlane supply 2.241 knows (Supply::Languages::ALL_LANGUAGES). */
export const PLAY_KNOWN = `af am ar az-AZ be bg bn-BD ca cs-CZ da-DK de-DE el-GR en-AU en-CA en-GB en-IN en-SG
  en-US en-ZA es-419 es-ES es-US et eu-ES fa fi-FI fil fr-CA fr-FR gl-ES hi-IN hr hu-HU hy-AM id
  is-IS it-IT iw-IL ja-JP ka-GE km-KH kn-IN ko-KR ky-KG lo-LA lt lv mk-MK ml-IN mn-MN mr-IN ms
  ms-MY my-MM ne-NP nl-NL no-NO pl-PL pt-BR pt-PT rm ro ru-RU si-LK sk sl sr sv-SE sw ta-IN te-IN
  th tr-TR uk vi zh-CN zh-HK zh-TW zu`.split(/\s+/);

/** Every locale App Store Connect takes (FastlaneCore::Languages::ALL_LANGUAGES, 2.241). */
export const APPLE_KNOWN = `ar-SA bn-BD ca cs da de-DE el en-AU en-CA en-GB en-US es-ES es-MX fi fr-CA fr-FR
  gu-IN he hi hr hu id it ja kn-IN ko ml-IN mr-IN ms nl-NL no or-IN pa-IN pl pt-BR pt-PT ro ru sk
  sl-SI sv ta-IN te-IN th tr uk ur-PK vi zh-Hans zh-Hant`.split(/\s+/);

/** The site's URL of a page in a language: `/` in English, `/de/kontakt` in German. */
export function siteUrl(site, path = '/') {
  const base = 'https://hashi.rs';
  if (site === 'en') return `${base}${path}`;
  return path === '/' ? `${base}/${site}` : `${base}/${site}${path}`;
}

/** The fastlane metadata, relative to the repository root. */
export const METADATA = 'fastlane/metadata';
export const PLAY_METADATA = `${METADATA}/android`;
