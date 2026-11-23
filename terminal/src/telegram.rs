//! Telegram conversation, read from Hermes' session store (~/.hermes/state.db).
//! Opened read-only so it never blocks the gateway's writes.

use std::path::PathBuf;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};

pub struct TgMsg {
    pub ts: f64,
    pub user: bool,
    pub text: String,
}

pub struct Telegram {
    path: PathBuf,
    conn: Option<Connection>,
    last_id: i64,
}

const SQL: &str = "
SELECT m.id, m.timestamp, m.role, m.content
FROM messages m JOIN sessions s ON s.id = m.session_id
WHERE s.source = 'telegram'
  AND m.role IN ('user', 'assistant')
  AND m._compressed_summary = 0
  AND m.content IS NOT NULL AND trim(m.content) <> ''
  AND m.id > ?1
ORDER BY m.id DESC
LIMIT 200";

impl Telegram {
    pub fn new(path: PathBuf) -> Self {
        Telegram { path, conn: None, last_id: 0 }
    }

    /// New messages since the last call, oldest first.
    pub fn poll(&mut self) -> Result<Vec<TgMsg>, String> {
        if self.conn.is_none() {
            if !self.path.exists() {
                return Err(format!("{} not found", self.path.display()));
            }
            let c = Connection::open_with_flags(
                &self.path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(|e| e.to_string())?;
            let _ = c.busy_timeout(Duration::from_millis(250));
            self.conn = Some(c);
        }
        let conn = self.conn.as_ref().unwrap();
        let last_id = self.last_id;
        let res = (|| -> rusqlite::Result<Vec<(i64, TgMsg)>> {
            let mut st = conn.prepare_cached(SQL)?;
            let rows = st.query_map([last_id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    TgMsg {
                        ts: r.get::<_, f64>(1)?,
                        user: r.get::<_, String>(2)? == "user",
                        text: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    },
                ))
            })?;
            rows.collect()
        })();
        match res {
            Ok(mut v) => {
                v.reverse();
                if let Some((id, _)) = v.last() {
                    self.last_id = *id;
                }
                Ok(v.into_iter().map(|(_, m)| m).collect())
            }
            Err(e) => {
                self.conn = None; // reopen next time
                Err(e.to_string())
            }
        }
    }
}
