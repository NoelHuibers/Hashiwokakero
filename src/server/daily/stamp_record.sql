-- One user's eligible history. Existing partial indexes provide daily counts/neighbors.
-- Indistinguishable own ranking keys and NULL own times retain the original windows:
-- ROW_NUMBER gives exact ties distinct places in an unspecified order.
-- Keep the fallback window definitions and eligibility in step with fame::placed.
WITH mine AS MATERIALIZED (
    SELECT user_id, day, hints, checks, time_ms, started_at, finished_at, run, daypart
    FROM attempts WHERE user_id = ?1
        AND finished_at IS NOT NULL AND revealed = 0 AND flag IS NULL
), ambiguous AS MATERIALIZED (
    SELECT a.day FROM mine a WHERE a.time_ms IS NULL OR EXISTS (
        SELECT 1 FROM attempts b
        WHERE b.day = a.day AND b.finished_at IS NOT NULL AND b.revealed = 0 AND b.flag IS NULL
            AND b.time_ms = a.time_ms AND b.finished_at = a.finished_at AND b.user_id != a.user_id
    )
)
SELECT a.day, a.hints, a.checks,
    (SELECT COUNT(*) + 1 FROM attempts b
     WHERE b.day = a.day AND b.finished_at IS NOT NULL AND b.revealed = 0 AND b.flag IS NULL
        AND (b.time_ms IS NULL OR (b.time_ms, b.finished_at) < (a.time_ms, a.finished_at))) AS place,
    (SELECT COUNT(*) FROM attempts b
     WHERE b.day = a.day AND b.finished_at IS NOT NULL AND b.revealed = 0 AND b.flag IS NULL) AS finishers,
    a.finished_at = (SELECT MIN(b.finished_at) FROM attempts b
     WHERE b.day = a.day AND b.finished_at IS NOT NULL AND b.revealed = 0 AND b.flag IS NULL) AS first_finish,
    a.time_ms, a.started_at, a.finished_at,
    a.time_ms - (SELECT b.time_ms FROM attempts b
     WHERE b.day = a.day AND b.finished_at IS NOT NULL AND b.revealed = 0 AND b.flag IS NULL
        AND (b.time_ms, b.finished_at) < (a.time_ms, a.finished_at)
     ORDER BY b.time_ms DESC, b.finished_at DESC LIMIT 1) AS above,
    (SELECT b.time_ms FROM attempts b
     WHERE b.day = a.day AND b.finished_at IS NOT NULL AND b.revealed = 0 AND b.flag IS NULL
        AND (b.time_ms, b.finished_at) > (a.time_ms, a.finished_at)
     ORDER BY b.time_ms, b.finished_at LIMIT 1) - a.time_ms AS below,
    a.run, a.daypart
FROM mine a WHERE a.day NOT IN (SELECT day FROM ambiguous)
UNION ALL
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
            AND (a.day IN (SELECT day FROM ambiguous))
    ) q WINDOW w AS (PARTITION BY q.day ORDER BY q.place)
) p JOIN attempts a ON a.user_id = p.user_id AND a.day = p.day
WHERE p.user_id = ?1 ORDER BY day
