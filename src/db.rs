use crate::Cloudcast;
use log::{debug, info};
use rusqlite::{params, Connection, Error, NO_PARAMS};
use std::path::Path;

const MIGRATIONS: [&'static str; 2] = [
    //========= 1 create DJs table =========
    "
        CREATE TABLE IF NOT EXISTS djs (
            id         INTEGER PRIMARY KEY,
            mixcloud_id TEXT NOT NULL UNIQUE,
            username   TEXT NOT NULL
        )
    ",
    //========= 2 create sets table =========
    "
        CREATE TABLE IF NOT EXISTS sets (
            id           INTEGER PRIMARY KEY,
            dj_id        INTEGER NOT NULL,
            mixcloud_id  TEXT NOT NULL UNIQUE,
            url          TEXT NOT NULL,
            cover_url    TEXT NOT NULL,
            publish_date TEXT NOT NULL,
            updated_date TEXT NOT NULL,

            FOREIGN KEY(dj_id) REFERENCES djs(id)
        )
    ",
];

pub fn init(path: impl AsRef<Path>) -> Result<Connection, Error> {
    info!("opening connection to sqlite db at {:?}", path.as_ref());
    let mut conn = rusqlite::Connection::open(path)?;

    let ver = conn.pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))? as usize;
    info!("database is at version {} (0 is freshly-created)", ver);

    if (ver as usize) < MIGRATIONS.len() {
        info!(
            "need to run migrations! currently at {}, migrations list is at {}",
            ver,
            MIGRATIONS.len()
        );

        for (i, m) in MIGRATIONS[ver..].iter().enumerate() {
            let tx = conn.transaction()?;
            debug!("executing migration {}", i + 1);
            tx.execute(m, NO_PARAMS)?;
            tx.commit()?;

            debug!(
                "migration committed! rewriting database version to {}",
                i + 1
            );
            conn.pragma_update(None, "user_version", &(i as i64 + 1))?;
        }
    }

    Ok(conn)
}

pub fn upsert_dj(conn: &mut Connection, username: &str, mixcloud_id: &str) -> Result<i64, Error> {
    let tx = conn.transaction()?;
    tx.execute(
        "REPLACE INTO djs (id, mixcloud_id, username) VALUES ((SELECT id FROM djs WHERE mixcloud_id = ?1), ?1, ?2)",
        &[mixcloud_id, username],
    )?;
    let id = tx.last_insert_rowid();
    tx.commit()?;

    Ok(id)
}

pub fn insert_api_cloudcasts(conn: &mut Connection, dj_id: i64, sets: &[Cloudcast]) -> Result<(), Error> {
    let tx = conn.transaction()?;

    for cc in sets {
        tx.execute(
            "INSERT INTO sets (url, cover_url, publish_date, updated_date, dj_id) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT DO NOTHING",
            params![
                &cc.url,
                &cc.pictures.extra_large,
                cc.created_time,
                cc.updated_time,
                dj_id
            ],
        )?;
    }

    tx.commit()
}
