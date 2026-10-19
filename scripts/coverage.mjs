// Line coverage of the tests CI runs, held against a baseline so it can't quietly drop.
//
//   pnpm run coverage             run the tests with coverage, report, compare to the baseline
//   pnpm run coverage --update    the same, then store the new numbers as the baseline
//
// Needs cargo-llvm-cov (`cargo binstall cargo-llvm-cov`), it installs the llvm-tools-preview
// component on first use. Reports land in target/llvm-cov/: html/index.html, lcov.info and
// summary.md (also appended to the job summary in CI). CARGO_BUILD_JOBS limits the build jobs.
//
// Fails when the total or a crate drops more than TOLERANCE percentage points below the
// baseline (scripts/coverage-baseline.json). The tolerance absorbs small noise, a real drop is
// usually a new file or feature without tests.

import { execFileSync, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

const TOLERANCE = 1;
// Files below this many lines don't make the "least covered" list, it is meant to show where
// tests would pay off most
const SIGNIFICANT_LINES = 100;

const root = path.resolve(import.meta.dirname, '..');
const baselineFile = path.join(root, 'scripts/coverage-baseline.json');
const update = process.argv.includes('--update');

// The tests CI runs, it runs them only here (the coverage job in .github/workflows/ci.yml, the
// test job only lints and runs the doctests, which cargo-llvm-cov covers on nightly only). All in
// one (debug) profile: cargo-llvm-cov reports one profile at a time, and the dojo's tests are
// fast in debug too, its sandbox counts fuel, not time.
const RUNS = [
  ['-p', 'common', '-p', 'backend', '-p', 'sat_solver'],
  ['-p', 'dojo', '--features', 'worker'],
  ['-p', 'hashi', '--lib', '--no-default-features', '--features', 'ssr'],
];

// Crates by the directory of their sources, the most specific first
const CRATES = [
  ['dojo/house/', 'house'],
  ['backend/', 'backend'],
  ['common/', 'common'],
  ['dojo/', 'dojo'],
  ['sat_solver/', 'sat_solver'],
  ['src/', 'hashi'],
];

// Installs llvm-tools-preview without asking, also in CI
const env = { ...process.env, CARGO_LLVM_COV_SETUP: 'yes' };

function cargo(...args) {
  const result = spawnSync('cargo', args, { cwd: root, env, stdio: 'inherit' });
  if (result.status !== 0) {
    console.error(`cargo ${args.join(' ')} failed`);
    process.exit(result.status ?? 1);
  }
}

const metadata = JSON.parse(
  execFileSync('cargo', ['metadata', '--format-version', '1', '--no-deps'], {
    cwd: root,
    maxBuffer: 64 * 1024 * 1024,
  }),
);
const out = path.join(metadata.target_directory, 'llvm-cov');

// Removes old profiles and the workspace's own build artifacts, which would otherwise count too.
// The runs below keep what the others left (--no-report), the report merges them. It needs
// --workspace, by default it only covers the root package.
cargo('llvm-cov', 'clean', '--workspace');
for (const run of RUNS) {
  // --locked: the versions in Cargo.lock, like every build in CI
  cargo('llvm-cov', '--no-report', '--locked', ...run);
}
fs.mkdirSync(out, { recursive: true });
const summaryJson = path.join(out, 'summary.json');
const report = (...args) => cargo('llvm-cov', 'report', '--workspace', ...args);
report('--json', '--summary-only', '--output-path', summaryJson);
report('--lcov', '--output-path', path.join(out, 'lcov.info'));
report('--html', '--output-dir', out);

// Lines per file and crate
const summary = JSON.parse(fs.readFileSync(summaryJson, 'utf8'));
const files = summary.data[0].files.map(f => {
  const file = path.relative(root, f.filename).split(path.sep).join('/');
  const crate = CRATES.find(([dir]) => file.startsWith(dir))?.[1] ?? file.split('/')[0];
  return { file, crate, lines: f.summary.lines.count, covered: f.summary.lines.covered };
});
const sum = list => ({
  lines: list.reduce((n, f) => n + f.lines, 0),
  covered: list.reduce((n, f) => n + f.covered, 0),
});
const percent = ({ lines, covered }) => (lines ? Math.round((covered / lines) * 1e4) / 100 : 100);
const names = [...new Set(files.map(f => f.crate))].sort();
const crates = Object.fromEntries(
  names.map(name => [name, sum(files.filter(f => f.crate === name))]),
);
const total = sum(files);
const current = {
  total: percent(total),
  crates: Object.fromEntries(names.map(name => [name, percent(crates[name])])),
};
const baseline = fs.existsSync(baselineFile)
  ? JSON.parse(fs.readFileSync(baselineFile, 'utf8'))
  : { crates: {} };

// Compare with the baseline: a drop beyond the tolerance fails, a rise asks for a new baseline
const fmt = n => (n === undefined ? '-' : n.toFixed(2));
const dropped = [];
let rose = false;
const rows = [
  ...names.map(name => [name, crates[name], current.crates[name], baseline.crates[name]]),
  ['total', total, current.total, baseline.total],
].map(([name, lines, now, before]) => {
  let change = 'new';
  if (before === undefined) {
    rose = true;
  } else {
    const d = Math.round((now - before) * 100) / 100;
    if (d < -TOLERANCE) dropped.push(`${name} ${fmt(now)}% (baseline ${fmt(before)}%)`);
    // Smaller changes are noise
    if (d >= 0.5) rose = true;
    change = d > 0 ? `+${fmt(d)}` : fmt(d);
  }
  return { name, ...lines, now, before, change };
});
for (const name of Object.keys(baseline.crates)) {
  if (!(name in crates)) dropped.push(`${name} has no coverage data`);
}

const UPDATE = '`pnpm run coverage --update` and commit scripts/coverage-baseline.json';
let verdict;
if (update) {
  verdict = 'Stored these numbers as the new baseline, commit scripts/coverage-baseline.json.';
} else if (dropped.length) {
  verdict =
    `Coverage dropped more than ${TOLERANCE} percentage point below the baseline: ` +
    `${dropped.join(', ')}. Add tests for the new code. If the drop is intended (tested code ` +
    `was removed or moved), lower the baseline with ${UPDATE}.`;
} else if (rose) {
  verdict = `Coverage rose, raise the baseline so it stays there: ${UPDATE}.`;
} else {
  verdict = `Coverage is within ${TOLERANCE} percentage point of the baseline.`;
}

// The job summary in CI, also written to target/llvm-cov/summary.md
const table = (head, body) =>
  [head, head.map((_, i) => (i ? '--:' : '---')), ...body]
    .map(cells => `| ${cells.join(' | ')} |\n`)
    .join('');
const fileRow = f => [f.file, f.lines, f.covered, fmt(percent(f))];
const byCoverage = (a, b) => a.covered / a.lines - b.covered / b.lines || b.lines - a.lines;
const least = files.filter(f => f.lines >= SIGNIFICANT_LINES).sort(byCoverage).slice(0, 10);
const md = [
  '## Line coverage\n',
  `${verdict}\n`,
  table(
    ['Crate', 'Lines', 'Covered', '%', 'Baseline', 'Change'],
    rows.map(r => [r.name, r.lines, r.covered, fmt(r.now), fmt(r.before), r.change]),
  ),
  `### Least covered files (${SIGNIFICANT_LINES}+ lines)\n`,
  table(['File', 'Lines', 'Covered', '%'], least.map(fileRow)),
  '<details><summary>All files</summary>\n',
  table(['File', 'Lines', 'Covered', '%'], files.map(fileRow)),
  '</details>\n',
].join('\n');
fs.writeFileSync(path.join(out, 'summary.md'), md);
if (process.env.GITHUB_STEP_SUMMARY) fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY, md);

const line = cells =>
  cells.map((c, i) => (i ? String(c).padStart(10) : String(c).padEnd(12))).join('');
console.log(`\n${line(['crate', 'lines', '%', 'baseline', 'change'])}`);
for (const r of rows) console.log(line([r.name, r.lines, fmt(r.now), fmt(r.before), r.change]));
console.log(`\nReports: ${path.join(out, 'html/index.html')}, ${path.join(out, 'lcov.info')}\n`);

if (update) {
  fs.writeFileSync(baselineFile, `${JSON.stringify(current, null, 2)}\n`);
} else if (dropped.length) {
  console.error(verdict);
  process.exit(1);
}
console.log(verdict);
