use rusqlite::{Connection, Error, NO_PARAMS};
use std::path::Path;
use log::debug;
use crate::Cloudcast;

pub fn init(path: impl AsRef<Path>) -> Result<Connection, Error> {
    debug!("opening connection to sqlite db at {:?}", path.as_ref());
    let conn = rusqlite::Connection::open(path)?;

    debug!("initializing djs table");
    conn.execute("
        CREATE TABLE IF NOT EXISTS djs (
            mixcloudid TEXT PRIMARY KEY,
            username   TEXT NOT NULL
        )
    ", NO_PARAMS)?;

    conn.execute("
        CREATE TABLE IF NOT EXISTS sets (
            id           INTEGER PRIMARY KEY,
            url          TEXT NOT NULL,
            cover_url    TEXT NOT NULL,
            publish_date TEXT NOT NULL,
            updated_date TEXT NOT NULL
        )
    ", NO_PARAMS)?;

    Ok(conn)
}

pub fn upsert_dj(conn: &Connection, username: &str, mixcloud_id: &str) -> Result<usize, Error> {
    conn.execute("INSERT OR REPLACE INTO djs (mixcloudid, username) VALUES (?1, ?2)",
        &[mixcloud_id, username])
}

pub fn insert_api_cloudcasts(conn: &mut Connection, sets: &[Cloudcast]) -> Result<(), Error> {
    let tx = conn.transaction()?;

    for cc in sets {
        tx.execute("INSERT INTO sets (url, cover_url, publish_date, updated_date) VALUES (?1, ?2, ?3, ?4)",
            &[&cc.url, &cc.pictures.extra_large,
            &cc.created_time.to_rfc3339(), &cc.updated_time.to_rfc3339()])?;
    }

    tx.commit()
}