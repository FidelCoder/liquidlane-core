use super::model::now;
use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Serialize, de::DeserializeOwned};
use std::{path::Path, sync::Mutex, time::Duration};

pub struct Db(Mutex<Connection>);

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        if path != Path::new(":memory:") {
            let mut options = std::fs::OpenOptions::new();
            options.create(true).append(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            options.open(path)?;
        }
        let connection = Connection::open(path)?;
        let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        anyhow::ensure!(
            version <= 1,
            "database is newer than this marketplace build"
        );
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            PRAGMA foreign_keys=ON;
            CREATE TABLE IF NOT EXISTS records(kind TEXT NOT NULL,id TEXT NOT NULL,owner TEXT NOT NULL,body TEXT NOT NULL,PRIMARY KEY(kind,id));
            CREATE INDEX IF NOT EXISTS records_owner ON records(kind,owner);
            CREATE TABLE IF NOT EXISTS challenges(id TEXT PRIMARY KEY,address TEXT NOT NULL,message TEXT NOT NULL,expires INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS sessions(hash TEXT PRIMARY KEY,actor TEXT NOT NULL,scope TEXT NOT NULL,expires INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS unique_refs(namespace TEXT NOT NULL,value TEXT NOT NULL,order_id TEXT NOT NULL,PRIMARY KEY(namespace,value));
            CREATE TABLE IF NOT EXISTS events(seq INTEGER PRIMARY KEY,order_id TEXT NOT NULL,actor TEXT NOT NULL,action TEXT NOT NULL,at INTEGER NOT NULL);
            PRAGMA user_version=1;")?;
        Ok(Self(Mutex::new(connection)))
    }
    pub fn transaction<T>(&self, op: impl FnOnce(&Transaction<'_>) -> Result<T>) -> Result<T> {
        let mut connection = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("database unavailable"))?;
        let tx = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let result = op(&tx)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn get<T: DeserializeOwned>(&self, kind: &str, id: &str) -> Result<T> {
        self.transaction(|tx| get(tx, kind, id))
    }
    pub fn list<T: DeserializeOwned>(&self, kind: &str) -> Result<Vec<T>> {
        self.transaction(|tx| list(tx, kind))
    }
    pub fn session(&self, token: &str, scope: &str) -> Result<String> {
        let hash = super::crypto::digest(token.as_bytes());
        self.transaction(|tx| {
            tx.query_row(
                "SELECT actor FROM sessions WHERE hash=?1 AND scope=?2 AND expires>?3",
                params![hash, scope, now()],
                |r| r.get(0),
            )
            .optional()?
            .context("unauthorized: session expired or invalid")
        })
    }
}

pub fn get<T: DeserializeOwned>(tx: &Transaction<'_>, kind: &str, id: &str) -> Result<T> {
    let data: Option<String> = tx
        .query_row(
            "SELECT body FROM records WHERE kind=?1 AND id=?2",
            params![kind, id],
            |r| r.get(0),
        )
        .optional()?;
    serde_json::from_str(&data.context("record not found")?).context("invalid persisted record")
}
pub fn list<T: DeserializeOwned>(tx: &Transaction<'_>, kind: &str) -> Result<Vec<T>> {
    let mut query = tx.prepare("SELECT body FROM records WHERE kind=?1 ORDER BY rowid DESC")?;
    let rows = query.query_map([kind], |r| r.get::<_, String>(0))?;
    rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
}
pub fn put(
    tx: &Transaction<'_>,
    kind: &str,
    id: &str,
    owner: &str,
    value: &impl Serialize,
) -> Result<()> {
    tx.execute("INSERT INTO records(kind,id,owner,body) VALUES(?1,?2,?3,?4) ON CONFLICT(kind,id) DO UPDATE SET owner=excluded.owner,body=excluded.body", params![kind,id,owner,serde_json::to_string(value)?])?;
    Ok(())
}
pub fn bind(tx: &Transaction<'_>, namespace: &str, value: &str, order: &str) -> Result<()> {
    let existing: Option<String> = tx
        .query_row(
            "SELECT order_id FROM unique_refs WHERE namespace=?1 AND value=?2",
            params![namespace, value],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(existing) = existing {
        if existing != order {
            bail!("reference already belongs to another order");
        }
    } else {
        tx.execute(
            "INSERT INTO unique_refs VALUES(?1,?2,?3)",
            params![namespace, value, order],
        )?;
    }
    Ok(())
}
pub fn event(tx: &Transaction<'_>, order: &str, actor: &str, action: &str) -> Result<()> {
    tx.execute(
        "INSERT INTO events(order_id,actor,action,at) VALUES(?1,?2,?3,?4)",
        params![order, actor, action, now()],
    )?;
    Ok(())
}

pub fn bind_outpoint(tx: &Transaction<'_>, value: &str, order: &str) -> Result<()> {
    let mut query =
        tx.prepare("SELECT value,order_id FROM unique_refs WHERE namespace='funding_outpoint'")?;
    let rows = query.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (existing, owner) = row?;
        if super::chain::canonical_outpoint(&existing)? == value {
            anyhow::ensure!(
                owner == order,
                "funding output already belongs to another order"
            );
        }
    }
    bind(tx, "funding_outpoint", value, order)
}
