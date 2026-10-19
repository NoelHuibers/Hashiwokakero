// Self-hosted web fonts, so no visitor's IP address ever reaches Google Fonts (GDPR).
//
// Copies the woff2 files of the @fontsource packages below to public/fonts (served at /fonts/)
// and writes their @font-face rules next to them: fonts.css for every page, one stylesheet per
// script that Murecho and Kaisei Decol don't cover, linked by ScriptFont in app.rs.
// The files are split by unicode-range, a browser only downloads the slices a page uses.
//
// Runs after `pnpm install` (postinstall) and in the Dockerfile. public/fonts is not committed.

import { createHash } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';

const SHEETS = {
  // Body text, headings (only the bold cut, headings are all bold) and the pixel accents
  fonts: [
    '@fontsource-variable/murecho/index.css',
    '@fontsource/kaisei-decol/700.css',
    '@fontsource/dotgothic16/400.css',
  ],
  'noto-sans-kr': ['@fontsource-variable/noto-sans-kr/index.css'],
  'noto-sans-sc': ['@fontsource-variable/noto-sans-sc/index.css'],
  'noto-sans-tc': ['@fontsource-variable/noto-sans-tc/index.css'],
  'noto-sans-arabic': ['@fontsource-variable/noto-sans-arabic/index.css'],
  'noto-sans-devanagari': ['@fontsource-variable/noto-sans-devanagari/index.css'],
  'noto-sans-thai': ['@fontsource-variable/noto-sans-thai/index.css'],
  'be-vietnam-pro': [400, 500, 700, 900].map(w => `@fontsource/be-vietnam-pro/${w}.css`),
};

const root = path.resolve(import.meta.dirname, '..');
const modules = path.join(root, 'node_modules');
const out = path.join(root, 'public/fonts');

fs.rmSync(out, { recursive: true, force: true });
fs.mkdirSync(out, { recursive: true });

let files = 0;
for (const [sheet, sources] of Object.entries(SHEETS)) {
  const css = sources.map(source => {
    const file = path.join(modules, source);
    return fs
      .readFileSync(file, 'utf8')
      .replace(/\/\*[^]*?\*\//g, '')
      // Only woff2 (every current browser), with a content hash in the name so the files can be
      // cached forever (see cache_control in main.rs)
      .replace(/src: url\(\.\/files\/([^)]+)\.woff2\) (format\('[^']+'\))[^;]*;/g, (_, name, format) => {
        const bytes = fs.readFileSync(path.join(path.dirname(file), 'files', `${name}.woff2`));
        const hash = createHash('sha256').update(bytes).digest('hex').slice(0, 10);
        fs.writeFileSync(path.join(out, `${name}.${hash}.woff2`), bytes);
        files++;
        return `src: url(/fonts/${name}.${hash}.woff2) ${format};`;
      });
  });
  fs.writeFileSync(path.join(out, `${sheet}.css`), css.join('').replace(/\n\s*/g, '').trim() + '\n');
}
console.log(`fonts: ${files} files, ${Object.keys(SHEETS).length} stylesheets in public/fonts`);
