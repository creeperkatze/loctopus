use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::locs::Locs;

// Two-tier cache: an in-process moka layer in front of a SQLite table, so entries survive process restarts.
#[derive(Clone)]
pub struct Cache {
    memory: moka::future::Cache<String, Arc<Locs>>,
    db: SqlitePool,
    ttl: Duration,
}

impl Cache {
    pub async fn open(path: &str, ttl: Duration) -> Result<Self, sqlx::Error> {
        if let Some(dir) = std::path::Path::new(path).parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(sqlx::Error::Io)?;
        }

        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal);

        let db = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS cache (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                expires_at INTEGER NOT NULL
            )",
        )
        .execute(&db)
        .await?;

        Ok(Self {
            memory: moka::future::Cache::builder().time_to_live(ttl).build(),
            db,
            ttl,
        })
    }

    pub async fn get(&self, key: &str) -> Option<Arc<Locs>> {
        if let Some(value) = self.memory.get(key).await {
            return Some(value);
        }

        let row: Option<(String,)> =
            match sqlx::query_as("SELECT value FROM cache WHERE key = ? AND expires_at > ?")
                .bind(key)
                .bind(now_secs())
                .fetch_optional(&self.db)
                .await
            {
                Ok(row) => row,
                Err(e) => {
                    tracing::warn!(error = %e, "cache read failed");
                    return None;
                }
            };

        let (raw,) = row?;
        let value: Locs = match serde_json::from_str(&raw) {
            Ok(value) => value,
            Err(e) => {
                tracing::warn!(error = %e, "cache row deserialize failed");
                return None;
            }
        };

        let value = Arc::new(value);
        self.memory.insert(key.to_string(), Arc::clone(&value)).await;
        Some(value)
    }

    pub async fn entry_count(&self) -> i64 {
        sqlx::query_as("SELECT COUNT(*) FROM cache WHERE expires_at > ?")
            .bind(now_secs())
            .fetch_one(&self.db)
            .await
            .map(|(count,): (i64,)| count)
            .unwrap_or(0)
    }

    pub async fn insert(&self, key: String, value: Arc<Locs>) {
        self.memory.insert(key.clone(), Arc::clone(&value)).await;

        let Ok(raw) = serde_json::to_string(&*value) else {
            return;
        };
        let expires_at = now_secs() + self.ttl.as_secs() as i64;

        let result = sqlx::query(
            "INSERT INTO cache (key, value, expires_at) VALUES (?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, expires_at = excluded.expires_at",
        )
        .bind(key)
        .bind(raw)
        .bind(expires_at)
        .execute(&self.db)
        .await;

        if let Err(e) = result {
            tracing::warn!(error = %e, "cache write failed");
            return;
        }

        // Opportunistically sweep expired rows so the table doesn't grow unbounded.
        let _ = sqlx::query("DELETE FROM cache WHERE expires_at <= ?")
            .bind(now_secs())
            .execute(&self.db)
            .await;
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
