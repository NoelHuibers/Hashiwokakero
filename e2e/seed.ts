// The test data: players with the dailies of the last weeks, signed in sessions and a Dojo
// entry. Written into the schema the server created, before the server that the tests use
// starts, so its startup jobs (the stamp sync) already see it. Deterministic: the same random
// sequence every run, relative to today.
import { createHash } from 'node:crypto';
import { DatabaseSync } from 'node:sqlite';

const MS_PER_DAY = 86_400_000;

export interface Player {
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
  { name: 'aiko', username: 'aiko', pace: 1.0, session: sessionFor('aiko') },
  { name: 'bram', username: 'bram', pace: 1.3 },
  { name: 'chen', username: 'chen', pace: 0.9 },
  { name: 'Drowsy Otter 21', username: null, pace: 1.6, session: sessionFor('otter') },
  { name: 'emre', username: 'emre', pace: 1.2 },
  { name: 'fatima', username: 'fatima', pace: 1.1 },
  { name: 'Quiet Crane 5', username: null, pace: 2.0 },
  // In HASHI_ADMINS (server.ts)
  { name: 'admin', username: 'admin', pace: 1.4, session: sessionFor('admin') },
];

export const player = (name: string) => {
  const p = PLAYERS.find((p) => p.name === name);
  if (!p) throw new Error(`no seeded player ${name}`);
  return p;
};

/** A finished house entry of the SAT Dojo, for /dojo/:id. */
export const DOJO_SUBMISSION = 1;

/** Days of dailies before today. */
const HISTORY = 40;

export function seed(dbPath: string) {
  const db = new DatabaseSync(dbPath);
  const today = Math.floor(Date.now() / MS_PER_DAY);
  const random = mulberry32(7);
  db.exec('BEGIN');
  const ids = PLAYERS.map((p) => {
    const row = db
      .prepare('INSERT INTO users (username, name, created_at) VALUES (?, ?, ?) RETURNING id')
      .get(p.username, p.name, (today - HISTORY - 20) * MS_PER_DAY) as { id: number };
    if (p.session) {
      db.prepare('INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES (?, ?, ?, ?)').run(
        sha256(p.session),
        row.id,
        Date.now(),
        Date.now() + 365 * MS_PER_DAY,
      );
    }
    return row.id;
  });

  // The first three play nearly every day (streaks, medals), the others now and then
  const attempt = db.prepare(
    'INSERT INTO attempts (user_id, day, started_at, finished_at, hints, checks, time_ms) VALUES (?, ?, ?, ?, ?, ?, ?)',
  );
  for (let day = today - HISTORY; day < today; day++) {
    PLAYERS.forEach((p, i) => {
      if (random() >= (i < 3 ? 0.9 : 0.55)) return;
      const time = Math.round((60_000 + random() * 140_000) * p.pace);
      const finished = day * MS_PER_DAY + Math.floor(random() * 80_000_000);
      const hints = random() < 0.25 ? 1 : 0;
      const checks = random() < 0.33 ? 1 : 0;
      attempt.run(ids[i], day, finished - time, finished, hints, checks, time);
    });
  }
  // Today's board isn't empty, but the signed in players haven't played yet
  attempt.run(ids[1], today, Date.now() - 120_000, Date.now() - 1_000, 0, 0, 119_000);
  attempt.run(ids[2], today, Date.now() - 100_000, Date.now() - 2_000, 0, 1, 98_000);

  const families = [
    { family: 'Hashi', solved: 8, total: 8, par2: 0.4 },
    { family: 'HashiParity', solved: 4, total: 6, par2: 7.1 },
    { family: 'Random3Sat', solved: 7, total: 8, par2: 3.2 },
    { family: 'Pigeonhole', solved: 4, total: 6, par2: 7.5 },
    { family: 'Queens', solved: 6, total: 6, par2: 0.9 },
  ];
  const score = { par2: 3.6, solved: 29, total: 34, wrong: 0, budget: 10_000_000_000, fuel: 41.2, families };
  const created = (today - 10) * MS_PER_DAY;
  db.prepare(
    `INSERT INTO dojo_submissions (id, user_id, house, name, description, wasm, sha256, size, created_at, updated_at,
       state, season, progress, total, score, par2, solved, wrong, code_hash)
     VALUES (?, NULL, 1, 'Plain DPLL', 'Unit propagation and nothing else.', ?, ?, 1, ?, ?, 'done', 1, 34, 34, ?, ?, ?, 0, ?)`,
  ).run(
    DOJO_SUBMISSION,
    new Uint8Array([0]),
    sha256('plain-dpll'),
    created,
    created,
    JSON.stringify(score),
    score.par2,
    score.solved,
    sha256('plain-dpll-code'),
  );
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
