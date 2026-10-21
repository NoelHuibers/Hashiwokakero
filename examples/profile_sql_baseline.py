"""Reproduce the 2026-10-21 profile SQL duplicate-work baseline.

Run with Python 3.9+ from the Hashiwokakero checkout root. No third-party
packages, application database, application server or Rust build are used.
This is the exact benchmark body used for the preserved results, with this
docstring added. It reads current source SQL, so record the checkout revision.
"""
from pathlib import Path
import sqlite3, time, statistics, json, datetime, calendar
conn=sqlite3.connect(':memory:')
conn.executescript('''CREATE TABLE attempts(user_id INTEGER,day INTEGER,started_at INTEGER,finished_at INTEGER,hints INTEGER DEFAULT 0,checks INTEGER DEFAULT 0,revealed INTEGER DEFAULT 0,time_ms INTEGER,flag TEXT,run TEXT,daypart TEXT,PRIMARY KEY(user_id,day)); CREATE INDEX attempts_ranking ON attempts(day,time_ms); CREATE INDEX attempts_user_ranked ON attempts(user_id,day,time_ms) WHERE finished_at IS NOT NULL AND revealed=0 AND flag IS NULL; CREATE INDEX attempts_day_ranked ON attempts(day,time_ms,finished_at) WHERE finished_at IS NOT NULL AND revealed=0 AND flag IS NULL;''')
epoch=datetime.date(1970,1,1)
today=datetime.date(2026,10,21)
last=(today-epoch).days
players=1000; days=180
conn.executemany('INSERT INTO attempts(user_id,day,started_at,finished_at,time_ms) VALUES(?,?,?,?,?)',((u,d,d*86400000,d*86400000+((u*173+d*97)%1000000)+60000,((u*173+d*97)%1000000)+60000) for d in range(last-days+1,last+1) for u in range(1,players+1)))
conn.commit()
fame=Path('src/server/daily/fame.rs').read_text()
placed='SELECT a.user_id, a.day, a.hints'+fame.split('"SELECT a.user_id, a.day, a.hints')[1].split('"\n    )')[0]
month_sql=placed.format(filter='a.day BETWEEN ?1 AND ?2')
record_sql=Path('src/server/daily/stamp_record.sql').read_text()
stats_source=Path('src/server/daily/stats.rs').read_text()
rank_sql='SELECT a.day, a.time_ms, ('+stats_source.split('"SELECT a.day, a.time_ms, (')[1].split('",\n            params!')[0]
month_keys=sorted({(epoch+datetime.timedelta(days=d)).replace(day=1) for d in range(last-days+1,last+1)})
month_args=[((m-epoch).days,(m.replace(day=calendar.monthrange(m.year,m.month)[1])-epoch).days) for m in month_keys]
closed_args=month_args[:-1]
queries=[('profile_ranks',rank_sql,(1,)),('current_stamp_record',record_sql,(1,)),('current_month',month_sql,month_args[-1])]
queries += [('champion_'+str(m),month_sql,args) for m,args in zip(month_keys[:-1],closed_args)]
measured=[]
for label,sql,args in queries:
    runs=[]
    for _ in range(3):
        st=time.perf_counter(); rows=conn.execute(sql,args).fetchall(); runs.append((time.perf_counter()-st)*1000)
    measured.append({'query':label,'rows':len(rows),'median_ms':round(statistics.median(runs),2),'runs_ms':[round(x,2) for x in runs]})

def expensive_profile_queries():
    return sum(len(conn.execute(sql,args).fetchall()) for _,sql,args in queries)
batches=[]
for n in [1,10]:
    st=time.perf_counter(); rowcount=sum(expensive_profile_queries() for _ in range(n)); elapsed=(time.perf_counter()-st)*1000
    batches.append({'same_key_uncached_computations':n,'queries':len(queries)*n,'rows':rowcount,'total_sql_ms':round(elapsed,2)})
print(json.dumps({'sqlite':sqlite3.sqlite_version,'shape':{'players':players,'days':days,'attempts':players*days,'from':str(epoch+datetime.timedelta(days=last-days+1)),'through':str(today)},'queries':measured,'serial_duplicate_work_experiment':batches,'caveat':'SQL-only isolated in-memory synthetic benchmark; serial duplicated work, not HTTP concurrency, production latency or Rust aggregation. Excludes identity, solved_days, stored stamps, holders, shown stamp, Dojo and joined queries.'},indent=2))
