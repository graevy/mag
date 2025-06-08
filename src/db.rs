use rusqlite::{params, Connection, Result, OptionalExtension};
use std::path::Path;


// connect to the db
pub fn connect() -> Result<Connection> {
    let db_path = "music.db";
    let conn = Connection::open(db_path)?;

    conn.execute_batch(
        "BEGIN;
        CREATE TABLE IF NOT EXISTS songs (
            id INTEGER PRIMARY KEY,
            path TEXT NOT NULL UNIQUE
        );

        CREATE TABLE IF NOT EXISTS tags (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE
        );

        CREATE TABLE IF NOT EXISTS song_tags (
            song_id INTEGER,
            tag_id INTEGER,
            value INTEGER CHECK(value BETWEEN 0 AND 9),
            PRIMARY KEY (song_id, tag_id),
            FOREIGN KEY(song_id) REFERENCES songs(id),
            FOREIGN KEY(tag_id) REFERENCES tags(id)
        );

        CREATE TABLE IF NOT EXISTS contexts (
            id INTEGER PRIMARY KEY,
            timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
            query TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS play_events (
            id INTEGER PRIMARY KEY,
            song_id INTEGER,
            context_id INTEGER,
            started_at DATETIME,
            ended_at DATETIME,
            skipped BOOLEAN DEFAULT 0,
            FOREIGN KEY(song_id) REFERENCES songs(id),
            FOREIGN KEY(context_id) REFERENCES contexts(id)
        );

        CREATE TABLE IF NOT EXISTS feedback (
            id INTEGER PRIMARY KEY,
            play_event_id INTEGER,
            tag_id INTEGER,
            feedback INTEGER CHECK(feedback IN (-1, 1)),
            FOREIGN KEY(play_event_id) REFERENCES play_events(id),
            FOREIGN KEY(tag_id) REFERENCES tags(id)
        );
        COMMIT;"
    )?;

    Ok(conn)
}

// Struct to hold song results
#[derive(Debug)]
pub struct Song {
    pub id: i64,
    pub path: String,
}

// idempotent song add
pub fn add_song(path: &str) -> Result<()> {
    let conn = connect()?;
    conn.execute("INSERT OR IGNORE INTO songs (path) VALUES (?1)", params![path])?;
    Ok(())
}

// idempotent tag add
pub fn add_tag(name: &str) -> Result<()> {
    let conn = connect()?;
    conn.execute("INSERT OR IGNORE INTO tags (name) VALUES (?1)", params![name])?;
    Ok(())
}

// Remove song and all its relationships
pub fn remove_song(path: &str) -> Result<()> {
    let conn = connect()?;
    
    // Get the song ID first
    let song_id: Option<i64> = conn.query_row(
        "SELECT id FROM songs WHERE path = ?1", 
        params![path], 
        |row| row.get(0)
    ).optional()?;
    
    if let Some(id) = song_id {
        // Remove all tag relationships for this song
        conn.execute("DELETE FROM song_tags WHERE song_id = ?1", params![id])?;
        // Remove play events
        conn.execute("DELETE FROM play_events WHERE song_id = ?1", params![id])?;
        // Remove feedback related to this song's play events
        conn.execute("DELETE FROM feedback WHERE play_event_id IN (SELECT id FROM play_events WHERE song_id = ?1)", params![id])?;
        // Finally remove the song
        conn.execute("DELETE FROM songs WHERE id = ?1", params![id])?;
    }
    
    Ok(())
}

pub fn remove_tag(name: &str) -> Result<()> {
    let conn = connect()?;
    
    let tag_id: Option<i64> = conn.query_row(
        "SELECT id FROM tags WHERE name = ?1", 
        params![name], 
        |row| row.get(0)
    ).optional()?;
    
    if let Some(id) = tag_id {
        // Remove all song-tag relationships
        conn.execute("DELETE FROM song_tags WHERE tag_id = ?1", params![id])?;
        // Remove feedback for this tag
        conn.execute("DELETE FROM feedback WHERE tag_id = ?1", params![id])?;
        // Remove the tag itself
        conn.execute("DELETE FROM tags WHERE id = ?1", params![id])?;
    }
    
    Ok(())
}

pub fn tag_song(song_path: &str, tag_name: &str, value: u8) -> Result<()> {
    let conn = connect()?;
    let song_id: i64 = conn.query_row(
        "SELECT id FROM songs WHERE path = ?1",
        params![song_path],
        |row| row.get(0),
    )?;
    let tag_id: i64 = conn.query_row(
        "SELECT id FROM tags WHERE name = ?1",
        params![tag_name],
        |row| row.get(0),
    )?;

    conn.execute(
        "INSERT INTO song_tags (song_id, tag_id, value)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(song_id, tag_id) DO UPDATE SET value = excluded.value",
        params![song_id, tag_id, value],
    )?;

    Ok(())
}

// builds and sends the query
pub fn query_songs(conditions: &[(String, u8, String)]) -> Result<Vec<Song>> {
    if conditions.is_empty() {
        return Ok(Vec::new());
    }

    let conn = connect()?;
    
    // Build dynamic query with multiple JOINs - scales to any number of conditions
    let mut query = String::from("SELECT DISTINCT s.id, s.path FROM songs s");
    let mut where_conditions = Vec::new();
    let mut params_vec = Vec::new();
    
    for (i, (tag_name, value, operator)) in conditions.iter().enumerate() {
        // TODO: actual input sanitization
        let valid_operators = ["=", ">", "<", ">=", "<=", "!="];
        if !valid_operators.contains(&operator.as_str()) {
            return Err(rusqlite::Error::InvalidParameterName(
                format!("Invalid operator: {}", operator)
            ));
        }
        
        // Add JOIN clauses - each condition gets its own alias (st0, t0, st1, t1, etc.)
        query.push_str(&format!(
            " JOIN song_tags st{} ON s.id = st{}.song_id JOIN tags t{} ON st{}.tag_id = t{}.id",
            i, i, i, i, i
        ));
        
        // Add WHERE condition
        where_conditions.push(format!("(t{}.name = ? AND st{}.value {} ?)", i, i, operator));
        
        // Add parameters in order: tag_name, value
        params_vec.push(tag_name.clone());
        params_vec.push(value.to_string());
    }
    
    // Combine all WHERE conditions with AND
    query.push_str(" WHERE ");
    query.push_str(&where_conditions.join(" AND "));
    query.push_str(" ORDER BY s.path");
    
    // Convert params to the format rusqlite expects
    let params: Vec<&dyn rusqlite::ToSql> = params_vec.iter()
        .map(|p| p as &dyn rusqlite::ToSql)
        .collect();
    
    let mut stmt = conn.prepare(&query)?;
    let song_iter = stmt.query_map(&params[..], |row| {
        Ok(Song {
            id: row.get(0)?,
            path: row.get(1)?,
        })
    })?;

    let mut songs = Vec::new();
    for song in song_iter {
        songs.push(song?);
    }

    Ok(songs)
}
