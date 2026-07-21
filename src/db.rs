use crate::Error;
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::str::FromStr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const DEFAULT_DATABASE_PATH: &str = "data/bot.db";

/// 書き込み競合時に即エラー（SQLITE_BUSY）を返さないための待機時間。
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// 現在時刻を unix epoch 秒で返す。
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// migrations ディレクトリのマイグレーションを適用する。
async fn run_migrations(pool: &SqlitePool) -> Result<(), Error> {
    Migrator::new(std::path::Path::new("./migrations"))
        .await?
        .run(pool)
        .await?;
    Ok(())
}

/// SQLite に接続し、WAL・外部キーを有効化した上でマイグレーションを適用する。
pub async fn connect(path: &str) -> Result<SqlitePool, Error> {
    if let Some(parent) = std::path::Path::new(path).parent()
        && !parent.as_os_str().is_empty()
    {
        tokio::fs::create_dir_all(parent).await?;
    }

    let options = SqliteConnectOptions::from_str(path)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        .busy_timeout(BUSY_TIMEOUT);

    let pool = SqlitePoolOptions::new().connect_with(options).await?;
    run_migrations(&pool).await?;
    Ok(pool)
}

pub async fn insert_dareda_result(
    pool: &SqlitePool,
    user_id: u64,
    pokemon_id: i16,
    is_correct: bool,
    attempts: u32,
    answered_at: i64,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO dareda_result (user_id, pokemon_id, is_correct, attempts, answered_at) VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(user_id as i64)
    .bind(pokemon_id as i64)
    .bind(is_correct)
    .bind(attempts as i64)
    .bind(answered_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn insert_verify_log(
    pool: &SqlitePool,
    user_id: u64,
    result: &str,
    at: i64,
) -> Result<(), Error> {
    sqlx::query("INSERT INTO verify_log (user_id, result, at) VALUES (?1, ?2, ?3)")
        .bind(user_id as i64)
        .bind(result)
        .bind(at)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn insert_member_join(
    pool: &SqlitePool,
    user_id: u64,
    joined_at: i64,
    account_created_at: i64,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO member_join (user_id, joined_at, account_created_at) VALUES (?1, ?2, ?3)",
    )
    .bind(user_id as i64)
    .bind(joined_at)
    .bind(account_created_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// ランキングの集計期間。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    All,
    Month,
    Week,
}

impl Period {
    pub(crate) fn cutoff(self, now: i64) -> Option<i64> {
        match self {
            Period::All => None,
            Period::Month => Some(now - 30 * 24 * 60 * 60),
            Period::Week => Some(now - 7 * 24 * 60 * 60),
        }
    }
}

#[derive(Debug)]
pub struct RankingRow {
    pub user_id: i64,
    pub correct_count: i64,
    pub avg_attempts: f64,
}

/// 正解数の多い順のランキングを返す。
/// 同数の場合は正解時の平均試行回数の少ない方、さらに同値なら先に到達した方が上位。
pub async fn dareda_ranking(
    pool: &SqlitePool,
    period: Period,
    now: i64,
    limit: i64,
) -> Result<Vec<RankingRow>, Error> {
    let cutoff = period.cutoff(now);
    let rows = sqlx::query(
        "SELECT user_id, \
                SUM(is_correct) AS correct_count, \
                AVG(CASE WHEN is_correct = 1 THEN CAST(attempts AS REAL) END) AS avg_attempts, \
                MAX(CASE WHEN is_correct = 1 THEN answered_at END) AS reached_at \
         FROM dareda_result \
         WHERE (?1 IS NULL OR answered_at >= ?1) \
         GROUP BY user_id \
         HAVING SUM(is_correct) > 0 \
         ORDER BY correct_count DESC, avg_attempts ASC, reached_at ASC \
         LIMIT ?2",
    )
    .bind(cutoff)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    rows.iter()
        .map(|row| {
            Ok(RankingRow {
                user_id: row.try_get("user_id")?,
                correct_count: row.try_get("correct_count")?,
                avg_attempts: row.try_get("avg_attempts")?,
            })
        })
        .collect()
}

#[derive(Debug)]
pub struct DaredaHistoryEntry {
    pub pokemon_id: i64,
    pub is_correct: bool,
    pub attempts: i64,
    pub answered_at: i64,
}

#[derive(Debug)]
pub struct DaredaStats {
    pub correct_count: i64,
    pub total_count: i64,
    pub avg_attempts: Option<f64>,
    pub recent: Vec<DaredaHistoryEntry>,
}

pub async fn dareda_stats(pool: &SqlitePool, user_id: u64) -> Result<DaredaStats, Error> {
    let row = sqlx::query(
        "SELECT COALESCE(SUM(is_correct), 0) AS correct_count, \
                COUNT(*) AS total_count, \
                AVG(CAST(attempts AS REAL)) AS avg_attempts \
         FROM dareda_result WHERE user_id = ?1",
    )
    .bind(user_id as i64)
    .fetch_one(pool)
    .await?;

    let recent_rows = sqlx::query(
        "SELECT pokemon_id, is_correct, attempts, answered_at \
         FROM dareda_result WHERE user_id = ?1 \
         ORDER BY answered_at DESC, id DESC LIMIT 10",
    )
    .bind(user_id as i64)
    .fetch_all(pool)
    .await?;

    let recent = recent_rows
        .iter()
        .map(|row| {
            Ok(DaredaHistoryEntry {
                pokemon_id: row.try_get("pokemon_id")?,
                is_correct: row.try_get("is_correct")?,
                attempts: row.try_get("attempts")?,
                answered_at: row.try_get("answered_at")?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;

    Ok(DaredaStats {
        correct_count: row.try_get("correct_count")?,
        total_count: row.try_get("total_count")?,
        avg_attempts: row.try_get("avg_attempts")?,
        recent,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> SqlitePool {
        // インメモリ DB は接続ごとに別データベースになるため、プールは1接続に固定する
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        run_migrations(&pool).await.unwrap();
        pool
    }

    async fn add_result(
        pool: &SqlitePool,
        user_id: u64,
        pokemon_id: i16,
        is_correct: bool,
        attempts: u32,
        answered_at: i64,
    ) {
        insert_dareda_result(pool, user_id, pokemon_id, is_correct, attempts, answered_at)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn migrations_create_tables() {
        let pool = test_pool().await;
        insert_verify_log(&pool, 1, "success", 100).await.unwrap();
        insert_member_join(&pool, 1, 100, 50).await.unwrap();
        add_result(&pool, 1, 25, true, 1, 100).await;
    }

    #[tokio::test]
    async fn verify_log_rejects_invalid_result() {
        let pool = test_pool().await;
        let result = insert_verify_log(&pool, 1, "unknown", 100).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn ranking_orders_by_correct_count() {
        let pool = test_pool().await;
        for i in 0..3 {
            add_result(&pool, 1, 1, true, 1, 100 + i).await;
        }
        add_result(&pool, 2, 1, true, 1, 100).await;

        let rows = dareda_ranking(&pool, Period::All, 1000, 10).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].user_id, 1);
        assert_eq!(rows[0].correct_count, 3);
        assert_eq!(rows[1].user_id, 2);
    }

    #[tokio::test]
    async fn ranking_tiebreaks_by_avg_attempts() {
        let pool = test_pool().await;
        // 正解数同数（1）。平均試行回数が少ない user 2 が上位
        add_result(&pool, 1, 1, true, 3, 100).await;
        add_result(&pool, 2, 1, true, 1, 200).await;

        let rows = dareda_ranking(&pool, Period::All, 1000, 10).await.unwrap();
        assert_eq!(rows[0].user_id, 2);
        assert_eq!(rows[1].user_id, 1);
    }

    #[tokio::test]
    async fn ranking_tiebreaks_by_first_reached() {
        let pool = test_pool().await;
        // 正解数同数（1）・平均試行回数同数（1.0）の3人。先に到達した順に user 2, 1, 3
        add_result(&pool, 1, 1, true, 1, 200).await;
        add_result(&pool, 2, 1, true, 1, 100).await;
        add_result(&pool, 3, 1, true, 1, 300).await;

        let rows = dareda_ranking(&pool, Period::All, 1000, 10).await.unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].user_id, 2);
        assert_eq!(rows[1].user_id, 1);
        assert_eq!(rows[2].user_id, 3);
    }

    #[tokio::test]
    async fn ranking_week_boundary() {
        let pool = test_pool().await;
        let now = 1_000_000;
        let cutoff = now - 7 * 24 * 60 * 60;
        add_result(&pool, 1, 1, true, 1, cutoff - 1).await; // 範囲外
        add_result(&pool, 2, 1, true, 1, cutoff).await; // 境界（含む）
        add_result(&pool, 3, 1, true, 1, cutoff + 1).await; // 範囲内

        let rows = dareda_ranking(&pool, Period::Week, now, 10).await.unwrap();
        let ids: Vec<i64> = rows.iter().map(|r| r.user_id).collect();
        assert_eq!(ids, vec![2, 3]);
    }

    #[tokio::test]
    async fn ranking_month_boundary() {
        let pool = test_pool().await;
        let now = 10_000_000;
        let cutoff = now - 30 * 24 * 60 * 60;
        add_result(&pool, 1, 1, true, 1, cutoff - 1).await;
        add_result(&pool, 2, 1, true, 1, cutoff).await;

        let rows = dareda_ranking(&pool, Period::Month, now, 10).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].user_id, 2);
    }

    #[tokio::test]
    async fn ranking_all_includes_old_entries() {
        let pool = test_pool().await;
        add_result(&pool, 1, 1, true, 1, 1).await;

        let rows = dareda_ranking(&pool, Period::All, 10_000_000, 10)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[tokio::test]
    async fn ranking_empty_when_no_correct_answers() {
        let pool = test_pool().await;
        add_result(&pool, 1, 1, false, 3, 100).await;

        let rows = dareda_ranking(&pool, Period::All, 1000, 10).await.unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn ranking_limits_to_top_10() {
        let pool = test_pool().await;
        for user in 1..=12 {
            add_result(&pool, user, 1, true, 1, 100 + user as i64).await;
        }

        let rows = dareda_ranking(&pool, Period::All, 1000, 10).await.unwrap();
        assert_eq!(rows.len(), 10);
    }

    #[tokio::test]
    async fn stats_computes_aggregate_fields() {
        let pool = test_pool().await;
        add_result(&pool, 1, 25, true, 2, 100).await;
        add_result(&pool, 1, 26, true, 4, 200).await;
        add_result(&pool, 1, 27, false, 3, 300).await;

        let stats = dareda_stats(&pool, 1).await.unwrap();
        assert_eq!(stats.correct_count, 2);
        assert_eq!(stats.total_count, 3);
        assert_eq!(stats.avg_attempts, Some(3.0));
    }

    #[tokio::test]
    async fn stats_returns_zero_for_unknown_user() {
        let pool = test_pool().await;
        let stats = dareda_stats(&pool, 999).await.unwrap();
        assert_eq!(stats.correct_count, 0);
        assert_eq!(stats.total_count, 0);
        assert_eq!(stats.avg_attempts, None);
        assert!(stats.recent.is_empty());
    }

    #[tokio::test]
    async fn stats_recent_returns_10_newest_first() {
        let pool = test_pool().await;
        for i in 0..12 {
            add_result(&pool, 1, i as i16, true, 1, 100 + i as i64).await;
        }

        let stats = dareda_stats(&pool, 1).await.unwrap();
        assert_eq!(stats.recent.len(), 10);
        assert_eq!(stats.recent[0].pokemon_id, 11);
        assert_eq!(stats.recent[9].pokemon_id, 2);
    }

    #[tokio::test]
    async fn ranking_avg_attempts_uses_correct_answers_only() {
        let pool = test_pool().await;
        // 正解数は2人とも1。user 1 は正解1回（1回で正解）＋別ゲームの不正解（5回）、
        // user 2 は正解1回（2回で正解）のみ。
        // 正解のみの平均: user 1 = 1.0 < user 2 = 2.0 なので user 1 が上位。
        // 全レコードの平均だと user 1 = 3.0 > user 2 = 2.0 となり順位が逆転する。
        add_result(&pool, 1, 1, true, 1, 100).await;
        add_result(&pool, 1, 2, false, 5, 200).await;
        add_result(&pool, 2, 1, true, 2, 300).await;

        let rows = dareda_ranking(&pool, Period::All, 1000, 10).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].user_id, 1);
        assert_eq!(rows[0].avg_attempts, 1.0);
        assert_eq!(rows[1].user_id, 2);
    }

    #[tokio::test]
    async fn concurrent_writes_do_not_fail() {
        // busy_timeout 設定込みの connect() を使い、複数接続からの同時書き込みが
        // SQLITE_BUSY で失敗しないことを確認する
        let path = std::env::temp_dir().join(format!(
            "ayanamist-test-{}-{}.db",
            std::process::id(),
            now_unix()
        ));
        let path_str = path.to_string_lossy().into_owned();

        let pool = connect(&path_str).await.unwrap();
        let mut handles = Vec::new();
        for i in 0..20u64 {
            let pool = pool.clone();
            handles.push(tokio::spawn(async move {
                insert_dareda_result(&pool, i, 1, true, 1, 100).await
            }));
        }
        for handle in handles {
            handle.await.unwrap().unwrap();
        }

        let stats = dareda_stats(&pool, 7).await.unwrap();
        assert_eq!(stats.total_count, 1);

        pool.close().await;
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{path_str}{suffix}"));
        }
    }
}
