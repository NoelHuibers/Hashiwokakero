SELECT p.day, p.hints, p.checks, p.place, p.finishers, p.first_finish, p.time_ms,
    p.started_at, p.finished_at, p.above, p.below, a.run, a.daypart
 FROM (
    SELECT q.*, q.time_ms - LAG(q.time_ms) OVER w AS above,
        LEAD(q.time_ms) OVER w - q.time_ms AS below
    FROM (
        SELECT a.user_id, a.day, a.hints, a.checks, a.finished_at,
            ROW_NUMBER() OVER (PARTITION BY a.day ORDER BY a.time_ms, a.finished_at) AS place,
            COUNT(*) OVER (PARTITION BY a.day) AS finishers,
            a.finished_at = MIN(a.finished_at) OVER (PARTITION BY a.day) AS first_finish,
            a.time_ms, a.started_at
        FROM attempts a
        WHERE a.finished_at IS NOT NULL AND a.revealed = 0 AND a.flag IS NULL
            AND (a.day IN (SELECT day FROM attempts WHERE user_id = ?1 AND finished_at IS NOT NULL))
    ) q WINDOW w AS (PARTITION BY q.day ORDER BY q.place)
 ) p JOIN attempts a ON a.user_id = p.user_id AND a.day = p.day
 WHERE p.user_id = ?1 ORDER BY p.day
