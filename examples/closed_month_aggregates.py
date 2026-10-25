"""Closed-month standings: counting every read (before) against stored standings (after).

Run with Python 3.9+ from the Hashiwokakero checkout root. No third-party packages,
application database, server or Rust build are used. Same synthetic history as
examples/profile_sql_baseline.py (1,000 players x 180 days through 2026-10-21, one
ranked attempt each). The month SQL, the stored-standings read and migration 21
(tables and triggers) are read from the current source, so record the revision.
Prints JSON; see docs/performance/closed-month-aggregates.md.
"""
from pathlib import Path
import sqlite3, time, statistics, json, datetime, calendar

REPEATS = 5
fame = Path('src/server/daily/fame.rs').read_text()
db_rs = Path('src/server/db.rs').read_text()
placed = 'SELECT a.user_id, a.day, a.hints' + fame.split('"SELECT a.user_id, a.day, a.hints')[1].split('"\n    )')[0]
month_sql = placed.format(filter='a.day BETWEEN ?1 AND ?2')
stored_sql = 'SELECT s.user_id, s.points' + fame.split('"SELECT s.user_id, s.points')[1].split('",\n            params![month, limit')[0]
migration = '--' + db_rs.split('-- The points race of months')[1].split('",\n];')[0]

def points(place, finishers):
    # model::points
    if finishers <= 1:
        return 20
    beaten = max(0, min(finishers - 1, finishers - place))
    return 10 + (10 * beaten * 2 + finishers - 1) // ((finishers - 1) * 2)

def database(triggers):
    conn = sqlite3.connect(':memory:', isolation_level=None)
    conn.executescript('''CREATE TABLE attempts(user_id INTEGER,day INTEGER,started_at INTEGER,finished_at INTEGER,hints INTEGER DEFAULT 0,checks INTEGER DEFAULT 0,revealed INTEGER DEFAULT 0,time_ms INTEGER,flag TEXT,run TEXT,daypart TEXT,PRIMARY KEY(user_id,day)); CREATE INDEX attempts_ranking ON attempts(day,time_ms); CREATE INDEX attempts_user_ranked ON attempts(user_id,day,time_ms) WHERE finished_at IS NOT NULL AND revealed=0 AND flag IS NULL; CREATE INDEX attempts_day_ranked ON attempts(day,time_ms,finished_at) WHERE finished_at IS NOT NULL AND revealed=0 AND flag IS NULL;''')
    conn.executemany('INSERT INTO attempts(user_id,day,started_at,finished_at,time_ms) VALUES(?,?,?,?,?)', ((u, d, d*86400000, d*86400000+((u*173+d*97)%1000000)+60000, ((u*173+d*97)%1000000)+60000) for d in range(last-days+1, last+1) for u in range(1, players+1)))
    if triggers:
        conn.executescript(migration)
    return conn

epoch = datetime.date(1970, 1, 1)
today = datetime.date(2026, 10, 21)
last = (today - epoch).days
players = 1000; days = 180
month_keys = sorted({(epoch + datetime.timedelta(days=d)).replace(day=1) for d in range(last-days+1, last+1)})
month_args = [((m-epoch).days, (m.replace(day=calendar.monthrange(m.year, m.month)[1])-epoch).days) for m in month_keys]
closed = [(m.year*12 + m.month - 1, args) for m, args in zip(month_keys[:-1], month_args[:-1])]

def count(conn, args):
    # fame::count_points: the month query and the aggregation in Rust
    standings = {}
    rows = conn.execute(month_sql, args).fetchall()
    for user, _, _, _, finished_at, place, finishers, _, _, _ in rows:
        p = standings.setdefault(user, [0, 0, 1 << 62])
        p[0] += points(place, finishers); p[1] += 1; p[2] = min(p[2], finished_at)
    ranked = sorted(standings.items(), key=lambda e: (-e[1][0], e[1][2], e[0]))
    return [(u, p, s, f) for u, (p, s, f) in ranked], len(rows)

def store(conn, month, args):
    # fame::closed_month on a miss: count and store under the write lock
    conn.execute('BEGIN IMMEDIATE')
    standings, rows = count(conn, args)
    conn.execute('INSERT INTO closed_months (month, first_day, last_day) VALUES (?, ?, ?)', (month, *args))
    values = [f'({month}, {place}, {u}, {p}, {s}, {f})' for place, (u, p, s, f) in enumerate(standings, 1)]
    for i in range(0, len(values), 500):
        conn.execute('INSERT INTO month_standings (month, place, user_id, points, solved, first_finish) VALUES ' + ','.join(values[i:i+500]))
    conn.execute('COMMIT')
    return standings, rows

def timed(f):
    runs = []
    for _ in range(REPEATS):
        st = time.perf_counter(); out = f(); runs.append((time.perf_counter() - st) * 1000)
    return out, {'median_ms': round(statistics.median(runs), 2), 'runs_ms': [round(x, 2) for x in runs]}

conn = database(triggers=True)
result = {'sqlite': sqlite3.sqlite_version, 'repeats': REPEATS, 'shape': {'players': players, 'days': days, 'attempts': players*days, 'from': str(epoch + datetime.timedelta(days=last-days+1)), 'through': str(today), 'closed_months': len(closed)}}

# Before: every cold profile counted every closed month (champion = first of the full count)
def champions_before():
    counted = [count(conn, a) for _, a in closed]
    return [standings[0][0] for standings, _ in counted], sum(rows for _, rows in counted)
(winners_before, rows), t = timed(champions_before)
result['before_six_champions_sql_only'] = {**timed(lambda: sum(len(conn.execute(month_sql, a).fetchall()) for _, a in closed))[1], 'rows': rows}
result['before_six_champions_with_aggregation'] = {**t, 'rows': rows}

# Once per month (and after a change drops it): count and store
fills = []
for month, args in closed:
    st = time.perf_counter(); standings, rows = store(conn, month, args); ms = (time.perf_counter() - st) * 1000
    fills.append({'month': month, 'rows_read': rows, 'standings_stored': len(standings), 'ms': round(ms, 2)})
result['fill_once_per_month'] = {'total_ms': round(sum(f['ms'] for f in fills), 2), 'months': fills}

# After: what a cold profile reads, the stored winner of each month
def champions_after():
    read = [conn.execute(stored_sql, (m, 1)).fetchall() for m, _ in closed]
    return [r[0][0] for r in read], sum(len(r) for r in read)
(winners_after, rows), t = timed(champions_after)
assert winners_after == winners_before, (winners_after, winners_before)
result['after_six_champions'] = {**t, 'rows': rows}

# A closed month's board (all standings), before and after
m, a = closed[-1]
(full, _), t_before = timed(lambda: count(conn, a))
stored_full, t_after = timed(lambda: conn.execute(stored_sql, (m, 1 << 62)).fetchall())
assert [tuple(r) for r in stored_full] == full
result['closed_month_board'] = {'before': {**t_before, 'rows': players * (a[1] - a[0] + 1)}, 'after': {**t_after, 'rows': len(stored_full)}}

# The triggers on the hot path: finishing today's attempt, 1,000 times, with six months stored
def finishes(c):
    def run():
        for u in range(1, players + 1):
            c.execute('UPDATE attempts SET finished_at = finished_at + 1, time_ms = time_ms + 1 WHERE user_id = ? AND day = ?', (u, last))
    return run
plain = database(triggers=False)
result['today_finish_x1000'] = {'without_triggers': timed(finishes(plain))[1], 'with_triggers': timed(finishes(conn))[1]}
assert conn.execute('SELECT COUNT(*) FROM closed_months').fetchone()[0] == len(closed)

# A review of one old result drops its month only
conn.execute('UPDATE attempts SET flag = ? WHERE user_id = 1 AND day = ?', ('review', closed[2][1][0]))
result['after_review'] = {'months_stored': conn.execute('SELECT COUNT(*) FROM closed_months').fetchone()[0], 'standings_rows': conn.execute('SELECT COUNT(*) FROM month_standings').fetchone()[0]}
result['caveat'] = 'SQL plus the equivalent Python aggregation, in memory, one process; not HTTP, Turso round trips or production latency. Rust aggregation/sorting is mirrored in Python only to time a fill.'
print(json.dumps(result, indent=2))
