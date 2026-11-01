// The size of what the browser downloads for the site (the WASM, JS and CSS of a release build),
// and how long its pages take to hydrate, held against a baseline so neither grows unnoticed.
//
//   pnpm run bundle-size                  report the sizes, compare to the baseline
//   pnpm run bundle-size --update         the same, then store the new numbers as the baseline
//   pnpm run bundle-size --site dist/site the site of another build (CI's artifact)
//
// Needs a release build (`cargo leptos build --release`). The hydration times are those of
// `pnpm run perf:hydration` (e2e/perf/), read from its results when they are there. The report
// lands in target/bundle/: summary.md (also appended to the job summary in CI) and bundle.json,
// the current numbers in the baseline's format.
//
// Fails when the WASM and JS together grow more than the baseline's tolerance, gzipped (what
// most browsers get). The hydration times are only reported: a CI runner's speed varies too much
// to fail a build on them. See docs/performance/bundle-baseline.md.

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';

const root = path.resolve(import.meta.dirname, '..');
const baselineFile = path.join(root, 'docs/performance/bundle-baseline.json');
const out = path.join(root, 'target/bundle');
const args = process.argv.slice(2);
const update = args.includes('--update');
const option = (name: string, fallback: string): string => {
  const i = args.indexOf(name);
  const value = i >= 0 ? args[i + 1] : undefined;
  return value ? path.resolve(value) : fallback;
};
const pkg = path.join(option('--site', path.join(root, 'target/site')), 'pkg');
const hydrationFile = option(
  '--hydration',
  path.join(root, 'e2e/test-results-perf/hydration.json'),
);

/** The raw, gzipped and brotli-compressed size of an asset or a sum of them, in bytes. */
interface Sizes {
  raw: number;
  gzip: number;
  brotli: number;
}

/** e2e/perf/hydration.spec.ts's results, in ms per page. */
interface Hydration {
  runs: number;
  cpuThrottling: number;
  pages: Record<string, { median: number; min: number; max: number; runs: number[] }>;
}

/** The numbers of a build: bundle.json, and the baseline. */
interface Report {
  commit: string | null;
  date: string;
  tolerancePercent: number;
  assets: Record<string, Sizes>;
  code: Sizes;
  total: Sizes;
  hydration: Hydration | null;
}

// Both files are written by this repository's own scripts (this one and e2e/perf/)
const baseline: Report | null = fs.existsSync(baselineFile)
  ? JSON.parse(fs.readFileSync(baselineFile, 'utf8'))
  : null;
// In percent of the baseline's gzipped WASM and JS. BUNDLE_TOLERANCE overrides it for one run.
const tolerance = Number(process.env.BUNDLE_TOLERANCE ?? baseline?.tolerancePercent ?? 5);

// Every asset by its name without the content hash: hashi.<hash>.wasm -> hashi.wasm. The
// TypeScript declarations next to them aren't loaded.
if (!fs.existsSync(pkg)) {
  console.error(`no site at ${pkg}: run \`cargo leptos build --release\` or pass --site`);
  process.exit(1);
}
const assets = fs
  .readdirSync(pkg)
  .filter((file) => /\.(wasm|js|css)$/.test(file))
  .sort()
  .map((file) => {
    const data = fs.readFileSync(path.join(pkg, file));
    return {
      name: file.replace(/\.[\w-]{22}\.(\w+)$/, '.$1'),
      raw: data.length,
      // As in "Build app (web)": the best each compression does, the server's own may be less
      gzip: zlib.gzipSync(data, { level: 9 }).length,
      brotli: zlib.brotliCompressSync(data).length,
    };
  });
if (!(assets.some((a) => a.name.endsWith('.wasm')) && assets.some((a) => a.name.endsWith('.js')))) {
  console.error(`no WASM or JS in ${pkg}`);
  process.exit(1);
}
const sum = (list: Sizes[]): Sizes => ({
  raw: list.reduce((n, a) => n + a.raw, 0),
  gzip: list.reduce((n, a) => n + a.gzip, 0),
  brotli: list.reduce((n, a) => n + a.brotli, 0),
});
const code = sum(assets.filter((a) => !a.name.endsWith('.css')));
const total = sum(assets);

const hydration: Hydration | null = fs.existsSync(hydrationFile)
  ? JSON.parse(fs.readFileSync(hydrationFile, 'utf8'))
  : null;

// In CI the commit it ran on (a pull request's merge commit), git may not trust the checkout of a
// container
let commit = process.env.GITHUB_SHA?.slice(0, 7) ?? null;
try {
  commit ??= execFileSync('git', ['rev-parse', '--short', 'HEAD'], {
    cwd: root,
    encoding: 'utf8',
  }).trim();
} catch {
  // Not a checkout
}
const current: Report = {
  commit,
  date: new Date().toISOString().slice(0, 10),
  tolerancePercent: baseline?.tolerancePercent ?? 5,
  assets: Object.fromEntries(assets.map(({ name, ...sizes }) => [name, sizes])),
  code,
  total,
  hydration: hydration ?? baseline?.hydration ?? null,
};

// The gate: gzipped WASM and JS against the baseline
const growth = baseline ? ((code.gzip - baseline.code.gzip) / baseline.code.gzip) * 100 : 0;
const grew = baseline && growth > tolerance;

const kb = (n: number | undefined) => (n === undefined ? '-' : `${(n / 1024).toFixed(1)} KB`);
const change = (now: number, before: number | undefined) => {
  if (before === undefined) return 'new';
  const d = ((now - before) / before) * 100;
  return `${d > 0 ? '+' : ''}${d.toFixed(1)} %`;
};
const table = (head: string[], body: string[][]) =>
  [head, head.map((_, i) => (i ? '--:' : '---')), ...body]
    .map((cells) => `| ${cells.join(' | ')} |\n`)
    .join('');
const sizeRow = (name: string, now: Sizes, before: Sizes | undefined) => [
  name,
  kb(now.raw),
  kb(now.gzip),
  kb(now.brotli),
  kb(before?.gzip),
  change(now.gzip, before?.gzip),
];

const UPDATE =
  'run `pnpm run bundle-size --update` on a release build (or take bundle.json from the "Bundle size" job\'s artifact) and commit docs/performance/bundle-baseline.json';
let verdict: string;
if (update) {
  verdict =
    'Stored these numbers as the new baseline, commit docs/performance/bundle-baseline.json.';
} else if (!baseline) {
  verdict = `No baseline yet: ${UPDATE}.`;
} else if (grew) {
  verdict =
    `The gzipped WASM and JS grew ${growth.toFixed(1)} %, more than the ${tolerance} % allowed ` +
    `(${kb(baseline.code.gzip)} -> ${kb(code.gzip)}). Make it smaller, or if the growth is ` +
    `worth it, raise the baseline: ${UPDATE}.`;
} else if (growth < -tolerance) {
  verdict = `The gzipped WASM and JS shrank ${(-growth).toFixed(1)} %, lower the baseline so it stays there: ${UPDATE}.`;
} else {
  verdict = `The gzipped WASM and JS are within ${tolerance} % of the baseline (${change(code.gzip, baseline.code.gzip)}).`;
}

const ms = (n: number | undefined) => (n === undefined ? '-' : `${Math.round(n)} ms`);
const hydrationRows = hydration
  ? Object.entries(hydration.pages).map(([page, t]) => {
      const before = baseline?.hydration?.pages?.[page]?.median;
      return [
        `\`${page}\``,
        ms(t.median),
        `${ms(t.min)} – ${ms(t.max)}`,
        ms(before),
        before ? change(t.median, before) : '-',
      ];
    })
  : [];
const md = [
  '## Bundle size\n',
  `${verdict}\n`,
  table(
    ['Asset', 'Raw', 'gzip', 'brotli', 'Baseline gzip', 'Change'],
    [
      ...assets.map((a) => sizeRow(a.name, a, baseline?.assets?.[a.name])),
      sizeRow('**WASM + JS**', code, baseline?.code),
      sizeRow('**total**', total, baseline?.total),
    ],
  ),
  '## Hydration\n',
  hydration
    ? 'From navigation to the hydrated page (the `data-hydrated` mark of src/lib.rs), the median of ' +
      `${hydration.runs} cold loads with the CPU slowed down ${hydration.cpuThrottling}× (e2e/perf/). Only ` +
      `reported, the runner's speed varies too much to fail on it.\n`
    : 'No hydration times, run `pnpm run perf:hydration` first.\n',
  hydration ? table(['Page', 'Median', 'Range', 'Baseline', 'Change'], hydrationRows) : '',
].join('\n');
fs.mkdirSync(out, { recursive: true });
fs.writeFileSync(path.join(out, 'summary.md'), md);
fs.writeFileSync(path.join(out, 'bundle.json'), `${JSON.stringify(current, null, 2)}\n`);
if (process.env.GITHUB_STEP_SUMMARY) fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY, md);

const line = (cells: string[]) =>
  cells.map((c, i) => (i ? String(c).padStart(i === 2 ? 18 : 12) : String(c).padEnd(14))).join('');
console.log(`\n${line(['asset', 'raw', 'gzip', 'brotli', 'baseline', 'change'])}`);
const rows: [string, Sizes, Sizes | undefined][] = [
  ...assets.map((a): [string, Sizes, Sizes | undefined] => [a.name, a, baseline?.assets?.[a.name]]),
  ['WASM + JS', code, baseline?.code],
  ['total', total, baseline?.total],
];
for (const [name, now, before] of rows) {
  console.log(line(sizeRow(name, now, before)));
}
if (hydration) {
  console.log(`\nhydration (median of ${hydration.runs}, CPU ${hydration.cpuThrottling}× slower)`);
  for (const row of hydrationRows) console.log(line(row.map((c) => String(c).replaceAll('`', ''))));
}
console.log(`\nReport: ${path.join(out, 'summary.md')}\n`);

if (update) {
  fs.writeFileSync(baselineFile, `${JSON.stringify(current, null, 2)}\n`);
} else if (grew) {
  console.error(verdict);
  process.exit(1);
}
console.log(verdict);
