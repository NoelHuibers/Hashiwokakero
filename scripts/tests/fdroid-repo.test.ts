// node --test scripts/tests/: the working directory of our F-Droid repository
// (scripts/fdroid-repo.ts): the listings in every locale, the images from the capture or carried
// over from the published repository, nothing written outside it.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { PLAY } from '../../store/locales.ts';
import { APP_ID, assembleFdroid, carriedImages } from '../fdroid-repo.ts';

const temp = () => fs.mkdtempSync(path.join(os.tmpdir(), 'fdroid-repo-'));
const read = (...parts: string[]) => fs.readFileSync(path.join(...parts), 'utf8');

/** An index-v2.json with images in two locales, as fdroidserver writes it. */
const index = {
  repo: {},
  packages: {
    [APP_ID]: {
      metadata: {
        featureGraphic: {
          'en-US': {
            name: '/rs.hashi/en-US/featureGraphic_4UHr-QI70gw3ub2oT=.png',
            sha256: 'a',
            size: 1,
          },
          de: { name: '/rs.hashi/en-US/featureGraphic.png', sha256: 'a', size: 1 },
        },
        screenshots: {
          phone: {
            'en-US': [
              { name: '/rs.hashi/en-US/phoneScreenshots/1-daily.png', sha256: 'b', size: 1 },
              { name: '/rs.hashi/en-US/phoneScreenshots/2-ranks.png', sha256: 'c', size: 1 },
            ],
            'de-DE': [
              { name: '/rs.hashi/de-DE/phoneScreenshots/1-daily.png', sha256: 'd', size: 1 },
            ],
            '../evil': [{ name: '/rs.hashi/x/phoneScreenshots/1.png', sha256: 'e', size: 1 }],
            ja: [{ name: '/rs.hashi/ja/phoneScreenshots/../../../x.png', sha256: 'f', size: 1 }],
          },
        },
      },
    },
  },
};

test('reads the feature graphics and phone screenshots of an index, only plain names', () => {
  assert.deepEqual(carriedImages(index), [
    {
      locale: 'en-US',
      file: 'featureGraphic.png',
      name: '/rs.hashi/en-US/featureGraphic_4UHr-QI70gw3ub2oT=.png',
    },
    {
      locale: 'en-US',
      file: 'phoneScreenshots/1-daily.png',
      name: '/rs.hashi/en-US/phoneScreenshots/1-daily.png',
    },
    {
      locale: 'en-US',
      file: 'phoneScreenshots/2-ranks.png',
      name: '/rs.hashi/en-US/phoneScreenshots/2-ranks.png',
    },
    {
      locale: 'de-DE',
      file: 'phoneScreenshots/1-daily.png',
      name: '/rs.hashi/de-DE/phoneScreenshots/1-daily.png',
    },
  ]);
  assert.deepEqual(carriedImages(null), []);
  assert.deepEqual(carriedImages({ packages: {} }), []);
});

test('every locale of Google Play with its texts, and the icons', async () => {
  const out = temp();
  const summary = await assembleFdroid({ out });
  assert.equal(summary.locales, PLAY.length);
  assert.deepEqual(summary.screenshots, { images: 0, carried: 0 });
  assert.ok(fs.statSync(path.join(out, 'config.yml')).mode % 0o1000 === 0o600);
  assert.match(read(out, 'config.yml'), /^repo_url: https:\/\/.+\/repo$/m);
  assert.match(read(out, 'metadata', `${APP_ID}.yml`), /^License: MIT$/m);
  assert.ok(fs.existsSync(path.join(out, 'icon.png')));
  const app = path.join(out, 'metadata', APP_ID);
  for (const { locale } of PLAY) {
    for (const file of ['title.txt', 'short_description.txt', 'full_description.txt'])
      assert.ok(read(app, locale, file).trim().length > 0, `${locale}/${file}`);
  }
  // fr-CA is a copy of fr-FR
  assert.equal(
    read(app, 'fr-CA', 'full_description.txt'),
    read(app, 'fr-FR', 'full_description.txt'),
  );
  assert.ok(fs.existsSync(path.join(app, 'en-US', 'images', 'icon.png')));
});

test('the screenshots of the capture, else those of the published repository', async () => {
  const images = temp();
  fs.mkdirSync(path.join(images, 'en', 'play-phone'), { recursive: true });
  for (const f of ['2-ranks.png', '1-daily.png', '10-night.png'])
    fs.writeFileSync(path.join(images, 'en', 'play-phone', f), `capture ${f}`);
  fs.writeFileSync(path.join(images, 'en', 'feature.png'), 'capture feature');

  const fetched: string[] = [];
  const out = temp();
  const summary = await assembleFdroid({
    out,
    images,
    carry: 'https://example.org/fdroid/repo/',
    fetchFile: async (url) => {
      fetched.push(url);
      return Buffer.from(url.endsWith('/index-v2.json') ? JSON.stringify(index) : `live ${url}`);
    },
  });
  const app = path.join(out, 'metadata', APP_ID);
  // en-US from the capture (in its order), de-DE from the live repository
  assert.deepEqual(fs.readdirSync(path.join(app, 'en-US', 'images', 'phoneScreenshots')), [
    '1-daily.png',
    '10-night.png',
    '2-ranks.png',
  ]);
  assert.equal(read(app, 'en-US', 'images', 'featureGraphic.png'), 'capture feature');
  assert.equal(
    read(app, 'de-DE', 'images', 'phoneScreenshots', '1-daily.png'),
    'live https://example.org/fdroid/repo/rs.hashi/de-DE/phoneScreenshots/1-daily.png',
  );
  assert.deepEqual(summary.screenshots, { images: 1, carried: 1 });
  // The capture's locale isn't fetched again, the unsafe names never
  assert.deepEqual(fetched, [
    'https://example.org/fdroid/repo/index-v2.json',
    'https://example.org/fdroid/repo/rs.hashi/de-DE/phoneScreenshots/1-daily.png',
  ]);
  assert.ok(!fs.existsSync(path.join(out, 'metadata', 'x.png')));
});

test('nothing published yet: no images, no failure', async () => {
  const out = temp();
  const summary = await assembleFdroid({
    out,
    carry: 'https://example.org/fdroid/repo',
    fetchFile: async () => {
      throw new Error('404');
    },
  });
  assert.deepEqual(summary.screenshots, { images: 0, carried: 0 });
});
