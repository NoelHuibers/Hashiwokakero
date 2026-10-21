// What the tests cover, read from the code so new pages and languages are tested without
// touching the tests: the routes of src/app.rs and the languages of i18n/build.rs.
import { readFileSync } from 'node:fs';
import path from 'node:path';

import { ROOT } from './env';
import { DOJO_SUBMISSION, player, SEEDED_GROUP } from './seed';

const source = (file: string) => readFileSync(path.join(ROOT, file), 'utf8');

/** BCP 47 tags, the first is the default and has no URL prefix. */
export const LOCALES: string[] = (() => {
  const list = source('i18n/build.rs').match(/LOCALES: \[&str; \d+\] = \[([^\]]*)\]/);
  if (!list) throw new Error('LOCALES not found in i18n/build.rs');
  return [...list[1].matchAll(/"([^"]+)"/g)].map((m) => m[1]);
})();
export const DEFAULT_LOCALE = LOCALES[0];

/** A seeded value for the parameters of every route that has some. */
const PARAMS: Record<string, Record<string, number | string>> = {
  '/dojo/:id': { id: DOJO_SUBMISSION },
  '/player/:id': { id: player('aiko').id },
  '/groups/:id': { id: SEEDED_GROUP.id },
  '/g/:code': { code: SEEDED_GROUP.invite },
};

/** Every page of the router, parameters filled in: `/`, `/practice`, `/dojo/1`, ... */
export const ROUTES: string[] = [...source('src/app.rs').matchAll(/<Route path=path!\("([^"]*)"\)/g)].map((m) => {
  const route = '/' + m[1].replace(/^\//, '');
  return route.replace(/:(\w+)/g, (_, name: string) => {
    const value = PARAMS[route]?.[name];
    if (value === undefined) throw new Error(`no seeded value for :${name} of ${route} (e2e/site.ts)`);
    return String(value);
  });
});
if (ROUTES.length < 5) throw new Error('the routes in src/app.rs were not found');

/** `path` in a language: `/rules` -> `/de/rules`, `/` -> `/de`. */
export function localized(locale: string, path: string) {
  if (locale === DEFAULT_LOCALE) return path;
  return path === '/' ? `/${locale}` : `/${locale}${path}`;
}

export const RTL_LOCALES = new Set(['ar']);
