// The store screenshots' database: made-up players with two months of dailies, a group of
// friends, rivals and today's leaderboard, written into a fresh local SQLite database (never
// Turso). Deterministic like e2e/seed.ts: the same random sequence every run, relative to today.
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { DatabaseSync } from 'node:sqlite';

import type { Puzzle } from '../../e2e/board';

interface Fixed {
  puzzle: Puzzle;
  solution: number[];
}
type Difficulty = 'easy' | 'medium' | 'hard' | 'expert' | 'oni';
/**
 * The fixed puzzles of the screenshots, from the generator: a daily for every difficulty and a
 * practice board. Solutions in the order of the board's edges (e2e/board.ts `edges`).
 */
const FIXED = JSON.parse(readFileSync(new URL('./puzzles.json', import.meta.url), 'utf8')) as {
  daily: Record<Difficulty, Fixed>;
  practice: Fixed & { source: unknown };
};

const MS_PER_DAY = 86_400_000;
const today = () => Math.floor(Date.now() / MS_PER_DAY);
/** The weekday's difficulty, as model::daily_difficulty: the page names it next to the board. */
const WEEK: Difficulty[] = ['easy', 'medium', 'medium', 'hard', 'hard', 'expert', 'oni'];
const difficulty = WEEK[(today() + 3) % 7];
/** How long the day's puzzle takes against a medium one. */
const SCALE: Record<Difficulty, number> = { easy: 0.55, medium: 1, hard: 1.7, expert: 2.8, oni: 4.6 };

/**
 * Today's daily, the fixed one of the weekday's difficulty, and the practice board: the same
 * screenshots every run of a weekday.
 */
export const PUZZLES = {
  daily: { difficulty, ...FIXED.daily[difficulty], timeMs: Math.round(124_370 * SCALE[difficulty]) },
  practice: FIXED.practice,
};
export const DAILY = PUZZLES.daily;
/** Days of dailies before today. */
const HISTORY = 75;

interface Player {
  id: number;
  /** `null` for guests, who have a generated name. */
  username: string | null;
  name: string;
  /** Times this much slower than the quickest. */
  pace: number;
  /** How often they play. */
  keen: number;
}

/** The player the screenshots are taken as: in the group, with rivals and a long streak. */
export const ME = 'mika';
/**
 * Mika again, a second account under the same name, in the middle of today's daily: the first
 * screenshot. The first one has finished it, for the leaderboard.
 */
export const PLAYING = 'mika_play';
/** The value of a player's session cookie (by username, or name for guests). */
export const sessionOf = (user: string) => sha256(`hashi-store:${user}`);

const players: [string | null, string, number, number][] = [
  [ME, ME, 1.0, 1],
  [PLAYING, ME, 1.0, 0],
  ['aiko', 'aiko', 0.82, 0.95],
  ['lukas', 'lukas', 1.08, 0.9],
  ['sofia', 'sofia', 0.95, 0.85],
  ['chen_wei', 'chen_wei', 0.88, 0.9],
  ['fatima', 'fatima', 1.12, 0.8],
  ['emre', 'emre', 1.2, 0.7],
  ['mateo', 'mateo', 1.3, 0.6],
  ['ingrid', 'ingrid', 1.15, 0.75],
  ['kofi', 'kofi', 1.25, 0.6],
  ['yuki', 'yuki', 0.78, 0.7],
  ['ravi', 'ravi', 1.4, 0.65],
  ['olga', 'olga', 1.0, 0.7],
  ['noor', 'noor', 1.1, 0.6],
  ['pieter', 'pieter', 1.5, 0.55],
  ['hana', 'hana', 0.92, 0.6],
  ['tomas', 'tomas', 1.35, 0.5],
  ['jules', 'jules', 1.6, 0.5],
  [null, 'Sleepy Heron 12', 1.45, 0.4],
  [null, 'Brave Koi 7', 1.7, 0.4],
];
export const PLAYERS: Player[] = players.map(([username, name, pace, keen], i) => ({
  id: i + 1,
  username,
  name,
  pace,
  keen,
}));

export const id = (username: string) => {
  const p = PLAYERS.find((p) => (p.username ?? p.name) === username);
  if (!p) throw new Error(`no player ${username}`);
  return p.id;
};

/** The group of friends: mika made it, the others joined. */
export const GROUP = { id: 1, name: 'Bridge Club', invite: 'storeclub1', members: [ME, 'aiko', 'lukas', 'sofia', 'chen_wei', 'fatima'] };
export const RIVALS = ['aiko', 'lukas', 'sofia'];

/** Today's times of the others on a medium, in seconds: the leaderboard mika's solve lands in. */
const TODAY = [
  ['yuki', 101.4],
  ['aiko', 118.9],
  ['chen_wei', 127.3],
  ['sofia', 146.0],
  ['hana', 151.6],
  ['lukas', 163.2],
  ['olga', 170.8],
  ['ingrid', 184.5],
  ['fatima', 196.1],
  ['emre', 214.7],
  ['noor', 233.0],
  ['Sleepy Heron 12', 262.4],
  ['mateo', 281.9],
  ['ravi', 305.5],
] as const;

export function seed(dbPath: string) {
  const db = new DatabaseSync(dbPath);
  const now = Date.now();
  const day0 = today();
  const random = mulberry32(11);
  db.exec('BEGIN');
  for (const p of PLAYERS) {
    db.prepare('INSERT INTO users (id, username, name, created_at) VALUES (?, ?, ?, ?)').run(
      p.id,
      p.username,
      p.name,
      (day0 - HISTORY - 30) * MS_PER_DAY,
    );
  }
  // A guest with a session isn't cleaned up; every player gets one so none of them is
  for (const p of PLAYERS) {
    db.prepare('INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES (?, ?, ?, ?)').run(
      sha256(sessionOf(p.username ?? p.name)),
      p.id,
      now,
      now + 365 * MS_PER_DAY,
    );
  }

  const attempt = db.prepare(
    'INSERT INTO attempts (user_id, day, started_at, finished_at, hints, checks, time_ms) VALUES (?, ?, ?, ?, ?, ?, ?)',
  );
  for (let day = day0 - HISTORY; day < day0; day++) {
    // Sunday's Oni takes everyone five times as long as Monday's easy one
    const weekday = (((day + 3) % 7) + 7) % 7;
    const scale = [0.6, 0.9, 1.2, 1.6, 2.2, 3.2, 5][weekday];
    for (const p of PLAYERS) {
      // Not on the leaderboards: the second mika only plays today's
      if (p.username === PLAYING) continue;
      if (p.username !== ME && random() >= p.keen) continue;
      const time = Math.round((70_000 + random() * 90_000) * p.pace * scale);
      // In the afternoon, UTC
      const finished = day * MS_PER_DAY + 43_200_000 + Math.floor(random() * 30_000_000);
      const hints = random() < 0.15 ? 1 : 0;
      const checks = random() < 0.25 ? 1 : 0;
      attempt.run(p.id, day, finished - time, finished, hints, checks, time + hints * 30_000 + checks * 10_000);
    }
  }

  db.prepare('INSERT INTO dailies (day, difficulty, puzzle, solution, created_at) VALUES (?, ?, ?, ?, ?)').run(
    day0,
    DAILY.difficulty,
    JSON.stringify(DAILY.puzzle),
    JSON.stringify(DAILY.solution),
    day0 * MS_PER_DAY,
  );
  const scale = SCALE[DAILY.difficulty];
  TODAY.forEach(([name, seconds], i) => {
    const time = Math.round(seconds * 1000 * scale);
    const finished = now - (TODAY.length - i) * 600_000;
    attempt.run(id(name), day0, finished - time, finished, 0, i % 4 === 3 ? 1 : 0, time);
  });

  // Mika's solve of today, without hints or checks: the board stays solved on the page
  const solved = now - 300_000;
  // Third place, between aiko and chen_wei
  const mine = DAILY.timeMs;
  db.prepare(
    `INSERT INTO attempts (user_id, day, started_at, finished_at, hints, checks, time_ms, moves, bridges)
     VALUES (?, ?, ?, ?, 0, 0, ?, ?, ?)`,
  ).run(
    id(ME),
    day0,
    solved - mine,
    solved,
    mine,
    DAILY.solution.reduce((a, b) => a + b, 0),
    JSON.stringify(DAILY.solution),
  );
  // The second one started a minute and a half ago (scenes.ts moves it to just before each
  // screenshot)
  attempt.run(id(PLAYING), day0, now - 97_000, null, 0, 0, null);

  db.prepare('INSERT INTO groups (id, name, invite, owner_id, created_at) VALUES (?, ?, ?, ?, ?)').run(
    GROUP.id,
    GROUP.name,
    GROUP.invite,
    id(ME),
    (day0 - HISTORY) * MS_PER_DAY,
  );
  GROUP.members.forEach((name, i) => {
    db.prepare('INSERT INTO group_members (group_id, user_id, joined_at, colour) VALUES (?, ?, ?, ?)').run(
      GROUP.id,
      id(name),
      (day0 - HISTORY) * MS_PER_DAY + i * 3_600_000,
      i,
    );
  });
  // The group's river: the bridges of the members' recent dailies, two thirds of the way across
  // its first level (900 for six), so that no visit completes it and celebrates
  const runs = db
    .prepare(
      `SELECT user_id, day FROM attempts WHERE day >= ? AND day < ? AND finished_at IS NOT NULL
       AND user_id IN (${GROUP.members.map(() => '?').join(',')}) ORDER BY day, user_id`,
    )
    .all(day0 - 10, day0, ...GROUP.members.map(id)) as { user_id: number; day: number }[];
  const planks = 610;
  runs.forEach((r, i) => {
    const bridges = Math.floor(planks / runs.length) + (i < planks % runs.length ? 1 : 0);
    const run = { bridges, islands: 0, steady: false, dithers: 0, cleared: false, burst: 0 };
    db.prepare('UPDATE attempts SET run = ? WHERE user_id = ? AND day = ?').run(JSON.stringify(run), r.user_id, r.day);
  });
  for (const name of RIVALS) {
    db.prepare('INSERT INTO rivals (user_id, rival_id, created_at) VALUES (?, ?, ?)').run(
      id(ME),
      id(name),
      (day0 - 30) * MS_PER_DAY,
    );
  }
  db.exec('COMMIT');
  db.close();
}

function sha256(text: string) {
  return createHash('sha256').update(text).digest('hex');
}

/** A small seeded random number generator, uniform in [0, 1). */
function mulberry32(seed: number) {
  let a = seed;
  return () => {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4_294_967_296;
  };
}
