// The test data: players with the dailies of the last weeks, signed in sessions, a Dojo entry
// and contact messages. Written into the schema the server created, before the server that the
// tests use starts, so its startup jobs (the stamp sync) already see it. Deterministic: the same
// random sequence every run, relative to today.
import { createHash } from 'node:crypto';
import { DatabaseSync } from 'node:sqlite';

const MS_PER_DAY = 86_400_000;

export interface Player {
  /** The user id, for /player/:id. */
  id: number;
  name: string;
  /** `null` for guests. */
  username: string | null;
  /** Their times are this much slower than the fastest. */
  pace: number;
  /** A signed in session, the value of the `hashi_session` cookie. */
  session?: string;
}

/** A session cookie value: 64 hex digits, the database only keeps its SHA-256. */
const sessionFor = (name: string) => sha256(`hashi-e2e:${name}`);

export const PLAYERS: Player[] = [
  { id: 1, name: 'aiko', username: 'aiko', pace: 1.0, session: sessionFor('aiko') },
  { id: 2, name: 'bram', username: 'bram', pace: 1.3 },
  { id: 3, name: 'chen', username: 'chen', pace: 0.9 },
  { id: 4, name: 'Drowsy Otter 21', username: null, pace: 1.6, session: sessionFor('otter') },
  { id: 5, name: 'emre', username: 'emre', pace: 1.2 },
  { id: 6, name: 'fatima', username: 'fatima', pace: 1.1 },
  // With a session, or the startup's cleanup of abandoned guests would delete it, some time
  // after the tests started: a test seeing it or not depended on when
  { id: 7, name: 'Quiet Crane 5', username: null, pace: 2.0, session: sessionFor('crane') },
  // In HASHI_ADMINS (server.ts)
  { id: 8, name: 'admin', username: 'admin', pace: 1.4, session: sessionFor('admin') },
  // Uploads to the Dojo (dojo.spec.ts), a test of its own: what it changes doesn't reach the
  // others. Not in the dailies.
  { id: 9, name: 'kenji', username: 'kenji', pace: 0, session: sessionFor('kenji') },
];

/** Players without dailies. */
const NOT_PLAYING = new Set(['kenji']);

/** In the admin dashboard's messages. */
export const CONTACT_MESSAGES = [
  { name: 'Mirai', email: 'mirai@example.com', message: 'Could the daily have a 5×5 for beginners?' },
  { name: '', email: '', message: 'The koto music is lovely, thank you for it.' },
];

export const player = (name: string) => {
  const p = PLAYERS.find((p) => p.name === name);
  if (!p) throw new Error(`no seeded player ${name}`);
  return p;
};

/** A finished house entry of the SAT Dojo, for /dojo/:id. A second one, `Plain CDCL`, ranks
 * behind it. */
export const DOJO_SUBMISSION = 1;

/** The Dojo's current season (`DOJO_SEASON`), the one before is archived. */
export const DOJO_SEASON = 2;

/** The revealed seed of the archived season, and the commitment published at its start. */
export const ARCHIVED_SEED = 'e2e-archived-seed';
export const ARCHIVED_COMMITMENT = sha256(ARCHIVED_SEED);
/** The family of the seeded community benchmark. */
export const COMMUNITY_FAMILY = 'crypto-sha1';

/** A group of the players who play the most, made by aiko (the others joined after her). */
export const SEEDED_GROUP = {
  id: 1,
  name: 'Bridge Club',
  invite: 'seededgrp2',
  members: ['aiko', 'bram', 'chen', 'Drowsy Otter 21', 'emre', 'fatima'],
};

/** Two who played today (seed.ts) and nobody else: a test joins it as a guest and finishes the
 * daily, to see the open board with times, seals and duels (group-rows.spec.ts). */
export const TEA_GROUP = {
  id: 3,
  name: 'Tea House',
  invite: 'seededtea2',
  members: ['chen', 'bram'],
};

/** The day before today: the seeded group's members who played it have move logs, for the
 * replay theatre (replay.spec.ts). Every log moves on the first edges, which every daily has. */
export const REPLAY_DAY_BACK = 1;

/**
 * A pair whose river 川 is across its stream: Quiet Crane 5 (signed in, no other test uses them)
 * made it with bram. Only the crane's dailies in it have bridges (their runs), enough for the
 * stream and half of the river after it, a level that nobody has looked at yet: the crane's first
 * visit completes it and celebrates.
 */
export const RIVER_GROUP = {
  id: 2,
  name: 'Paper Cranes',
  invite: 'papercrane',
  members: ['Quiet Crane 5', 'bram'],
  /** Days before today it was made. */
  age: 30,
  /** A pair's stream takes 400 bridges, its river 850 (model::level_target). */
  bridges: 400 + 425,
};

/** Days of dailies before today. */
const HISTORY = 40;

export function seed(dbPath: string) {
  const db = new DatabaseSync(dbPath);
  const today = Math.floor(Date.now() / MS_PER_DAY);
  const random = mulberry32(7);
  db.exec('BEGIN');
  for (const p of PLAYERS) {
    db.prepare('INSERT INTO users (id, username, name, created_at) VALUES (?, ?, ?, ?)').run(
      p.id,
      p.username,
      p.name,
      (today - HISTORY - 20) * MS_PER_DAY,
    );
    if (p.session) {
      db.prepare('INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES (?, ?, ?, ?)').run(
        sha256(p.session),
        p.id,
        Date.now(),
        Date.now() + 365 * MS_PER_DAY,
      );
    }
  }

  // The first three play nearly every day (streaks, medals), the others now and then
  const attempt = db.prepare(
    'INSERT INTO attempts (user_id, day, started_at, finished_at, hints, checks, time_ms) VALUES (?, ?, ?, ?, ?, ?, ?)',
  );
  for (let day = today - HISTORY; day < today; day++) {
    PLAYERS.forEach((p, i) => {
      if (NOT_PLAYING.has(p.name)) return;
      if (random() >= (i < 3 ? 0.9 : 0.55)) return;
      const time = Math.round((60_000 + random() * 140_000) * p.pace);
      const finished = day * MS_PER_DAY + Math.floor(random() * 80_000_000);
      const hints = random() < 0.25 ? 1 : 0;
      const checks = random() < 0.33 ? 1 : 0;
      attempt.run(p.id, day, finished - time, finished, hints, checks, time);
    });
  }
  // Today's board isn't empty, but the signed in players haven't played yet
  attempt.run(player('bram').id, today, Date.now() - 120_000, Date.now() - 1_000, 0, 0, 119_000);
  attempt.run(player('chen').id, today, Date.now() - 100_000, Date.now() - 2_000, 0, 1, 98_000);

  // Move logs of yesterday for the group's members who played it, as the daily stores them:
  // a few bridges each, quicker or slower by their pace, and a hint for the second
  const logged = db.prepare(
    `UPDATE attempts SET log = ?1, paid_actions = CASE WHEN ?2 > 0 THEN json_array(json_object(
       'id', 'seeded-hint-0001', 'action', 'Hint', 'bridges', json_array(), 'outcome', json_object('Hint', json_array(1, 1)),
       'at', started_at + ?2)) ELSE '[]' END
     WHERE user_id = ?3 AND day = ?4 AND finished_at IS NOT NULL`,
  );
  SEEDED_GROUP.members.forEach((name, i) => {
    const p = player(name);
    const step = Math.round(9_000 * p.pace);
    const moves = [
      [0, 1],
      [1, 1],
      [0, 2],
      [2, 1],
      [3, 2],
      [1, 2],
      [4, 1],
      [5, 1],
    ];
    const log = moves.map(([edge, count], k) => ({ t: step * (k + 1) + i * 700, edge, count, tap: true }));
    // The hint, bought between the third and fourth move
    const hintAt = i === 1 ? step * 3 + i * 700 + 1_000 : 0;
    logged.run(JSON.stringify(log), hintAt, p.id, today - REPLAY_DAY_BACK);
  });

  // The group, a member colour each in the order they joined
  db.prepare('INSERT INTO groups (id, name, invite, owner_id, created_at) VALUES (?, ?, ?, ?, ?)').run(
    SEEDED_GROUP.id,
    SEEDED_GROUP.name,
    SEEDED_GROUP.invite,
    player('aiko').id,
    (today - HISTORY) * MS_PER_DAY,
  );
  SEEDED_GROUP.members.forEach((name, i) => {
    db.prepare('INSERT INTO group_members (group_id, user_id, joined_at, colour) VALUES (?, ?, ?, ?)').run(
      SEEDED_GROUP.id,
      player(name).id,
      (today - HISTORY) * MS_PER_DAY + i * 3_600_000,
      i,
    );
  });

  // The pair's river: the crane's dailies since it was made share its bridges, double bridges
  // and all, as their runs (server/daily/stamps.rs) have them
  const made = (today - RIVER_GROUP.age) * MS_PER_DAY;
  db.prepare('INSERT INTO groups (id, name, invite, owner_id, created_at) VALUES (?, ?, ?, ?, ?)').run(
    RIVER_GROUP.id,
    RIVER_GROUP.name,
    RIVER_GROUP.invite,
    player(RIVER_GROUP.members[0]).id,
    made,
  );
  RIVER_GROUP.members.forEach((name, i) => {
    db.prepare('INSERT INTO group_members (group_id, user_id, joined_at, colour) VALUES (?, ?, ?, ?)').run(
      RIVER_GROUP.id,
      player(name).id,
      made,
      i,
    );
  });
  const crane = player(RIVER_GROUP.members[0]).id;
  const days = db
    .prepare('SELECT day FROM attempts WHERE user_id = ? AND finished_at >= ? ORDER BY day')
    .all(crane, made)
    .map((row) => Number(row.day));
  days.forEach((day, i) => {
    // Shared out evenly, the first ones get what doesn't divide
    const bridges = Math.floor(RIVER_GROUP.bridges / days.length) + (i < RIVER_GROUP.bridges % days.length ? 1 : 0);
    const run = { bridges, islands: 0, steady: false, dithers: 0, cleared: false, burst: 0 };
    db.prepare('UPDATE attempts SET run = ? WHERE user_id = ? AND day = ?').run(JSON.stringify(run), crane, day);
  });

  db.prepare('INSERT INTO groups (id, name, invite, owner_id, created_at) VALUES (?, ?, ?, ?, ?)').run(
    TEA_GROUP.id,
    TEA_GROUP.name,
    TEA_GROUP.invite,
    player('chen').id,
    (today - 3) * MS_PER_DAY,
  );
  TEA_GROUP.members.forEach((name, i) => {
    db.prepare('INSERT INTO group_members (group_id, user_id, joined_at, colour) VALUES (?, ?, ?, ?)').run(
      TEA_GROUP.id,
      player(name).id,
      (today - 3) * MS_PER_DAY + i * 3_600_000,
      i + 2,
    );
  });

  // A hidden set: the dev families at larger sizes too, and the held-out one as the mystery family
  const families = [
    { family: 'Hashi', solved: 5, total: 5, par2: 0.4 },
    { family: 'HashiParity', solved: 4, total: 5, par2: 4.1 },
    { family: 'Random3Sat', solved: 4, total: 5, par2: 4.2 },
    { family: 'Pigeonhole', solved: 2, total: 5, par2: 12.5 },
    { family: 'Queens', solved: 3, total: 4, par2: 5.9 },
    { family: 'Factoring', solved: 5, total: 7, par2: 6.3 },
    { family: 'Miter', solved: 4, total: 6, par2: 7.4 },
    { family: 'Coloring', solved: 5, total: 7, par2: 6.0 },
    { family: 'Mystery', solved: 3, total: 4, par2: 5.2 },
  ];
  const score = { par2: 5.9, solved: 35, total: 48, wrong: 0, budget: 10_000_000_000, fuel: 61.2, families };
  const created = (today - 10) * MS_PER_DAY;
  // Per formula, in set order, for the metrics beyond PAR-2 (private while the season runs). The
  // held-out family is stored under its own name, Tseitin, and shown as the mystery's.
  const labels: Record<string, string[]> = {
    Hashi: ['Sat'],
    HashiParity: ['Unsat'],
    Random3Sat: ['Sat', 'Unsat'],
    Pigeonhole: ['Unsat', 'Unsat', 'Sat'],
    Queens: ['Sat'],
    Factoring: ['Sat', 'Unsat'],
    Miter: ['Unsat', 'Sat'],
    Coloring: ['Sat', 'Unsat'],
    Mystery: ['Unsat', 'Unsat', 'Sat', 'Unsat'],
  };
  const hiddenResults = (solved: (family: string, i: number) => number | null) =>
    families.flatMap(({ family, total }) =>
      Array.from({ length: total }, (_, i) => {
        const expected = labels[family][i % labels[family].length];
        const fuel = solved(family, i);
        return {
          name: `f-${family}-${i}`,
          family: family === 'Mystery' ? 'Tseitin' : family,
          expected,
          outcome: fuel === null ? 'Timeout' : { Solved: expected },
          fuel: fuel ?? 7_000_000_000,
        };
      }),
    );
  // Plain DPLL solves the first ones of every family, the CDCL below some others too
  const dpllResults = hiddenResults((family, i) => {
    const f = families.find((x) => x.family === family)!;
    return i < f.solved ? 2_000_000 * (i + 1) * (family.length + 3) : null;
  });
  const cdclResults = hiddenResults((family, i) => {
    const f = families.find((x) => x.family === family)!;
    if (i === f.total - 1) return 900_000_000 + 50_000_000 * family.length;
    return i < f.solved - 1 ? 9_000_000 * (i + 2) * family.length : null;
  });
  const cdclScore = { ...score, par2: 6.4, solved: 34, fuel: 70.5 };
  // rules_version is dojo::RULES_VERSION: with an older one the site evaluates it again
  const house = db.prepare(
    `INSERT INTO dojo_submissions (id, user_id, house, name, description, wasm, sha256, size, created_at, updated_at,
       state, season, progress, total, score, par2, solved, wrong, code_hash, rules_version, results)
     VALUES (?, NULL, 1, ?, ?, ?, ?, 1, ?, ?, 'done', ?, 48, 48, ?, ?, ?, 0, ?, 6, ?)`,
  );
  house.run(
    DOJO_SUBMISSION,
    'Plain DPLL',
    'Unit propagation and nothing else.',
    new Uint8Array([0]),
    sha256('plain-dpll'),
    created,
    created,
    DOJO_SEASON,
    JSON.stringify(score),
    score.par2,
    score.solved,
    sha256('plain-dpll-code'),
    JSON.stringify(dpllResults),
  );
  house.run(
    DOJO_SUBMISSION + 1,
    'Plain CDCL',
    'Clause learning, no restarts.',
    new Uint8Array([1]),
    sha256('plain-cdcl'),
    created,
    created,
    DOJO_SEASON,
    JSON.stringify(cdclScore),
    cdclScore.par2,
    cdclScore.solved,
    sha256('plain-cdcl-code'),
    JSON.stringify(cdclResults),
  );
  // The season before, archived: its revealed seed (which reveals its mystery family), its set and
  // the house entry's final result
  const archived = DOJO_SEASON - 1;
  db.prepare(
    `INSERT INTO dojo_seasons (season, started_at, ended_at, commitment, seed, generator_version, rules_version)
     VALUES (?, ?, ?, ?, ?, 2, 3)`,
  ).run(archived, (today - 40) * MS_PER_DAY, (today - 12) * MS_PER_DAY, ARCHIVED_COMMITMENT, ARCHIVED_SEED);
  db.prepare(
    'INSERT INTO dojo_season_sets (season, generator_version, archive, sha256, created_at) VALUES (?, 2, ?, ?, ?)',
  ).run(archived, new Uint8Array([0x1f, 0x8b, 8, 0]), sha256('set'), (today - 12) * MS_PER_DAY);
  const results = [
    { name: 'hashi-01', family: 'Hashi', expected: 'Sat', outcome: { Solved: 'Sat' }, fuel: 1_200_000 },
    { name: 'pigeonhole-01', family: 'Pigeonhole', expected: 'Unsat', outcome: 'Timeout', fuel: 10_000_000_000 },
  ];
  db.prepare(
    `INSERT INTO dojo_archive (season, submission_id, user_id, house, name, created_at, par2, solved, wrong, score, results,
       rules_version, rank)
     VALUES (?, ?, NULL, 1, 'Plain DPLL', ?, ?, ?, 0, ?, ?, 3, 1)`,
  ).run(
    archived,
    DOJO_SUBMISSION,
    created - 30 * MS_PER_DAY,
    score.par2,
    score.solved,
    JSON.stringify(score),
    JSON.stringify(results),
  );
  // A community benchmark by aiko, in the pool and drawn for the archived season (the running one
  // draws it too when the server starts)
  const benchmark = 'p cnf 1 2\n1 0\n-1 0\n';
  db.prepare(
    `INSERT INTO dojo_benchmarks (id, user_id, family, description, cnf, hash, shape, vars, clauses, state, answer,
       license, created_at, updated_at, accepted_at)
     VALUES (1, ?, ?, 'A tiny contradiction.', ?, ?, 'e2e-shape', 1, 2, 'accepted', 'UNSAT', 'CC-BY-4.0', ?, ?, ?)`,
  ).run(
    player('aiko').id,
    COMMUNITY_FAMILY,
    new TextEncoder().encode(benchmark),
    createHash('md5').update('1 0 -1 0').digest('hex'),
    created - 40 * MS_PER_DAY,
    created - 40 * MS_PER_DAY,
    created - 40 * MS_PER_DAY,
  );
  db.prepare('INSERT INTO dojo_season_pool (season, position, benchmark_id) VALUES (?, 0, 1)').run(archived);
  db.prepare("UPDATE dojo_seasons SET pool_digest = 'e2e' WHERE season = ?").run(archived);
  CONTACT_MESSAGES.forEach((m, i) => {
    db.prepare('INSERT INTO contact_messages (name, email, message, created_at) VALUES (?, ?, ?, ?)').run(
      m.name,
      m.email,
      m.message,
      Date.now() - (i + 1) * 3_600_000,
    );
  });
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
