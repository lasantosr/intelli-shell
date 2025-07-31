use rusqlite_migration::{M, Migrations};

/// Database migrations for the SQLite storage
pub const MIGRATIONS: Migrations<'_> = Migrations::from_slice(MIGRATION_SLICE);
const MIGRATION_SLICE: &[M<'_>] = &[
    // Migration 1: Create the command table
    M::up(
        r#"CREATE TABLE command (
            category TEXT NOT NULL,
            alias TEXT NULL,
            cmd TEXT NOT NULL UNIQUE,
            description TEXT NOT NULL,
            usage INTEGER DEFAULT 0
        );"#,
    ),
    // Migration 2: Create the FTS table for commands
    M::up(r#"CREATE VIRTUAL TABLE command_fts USING fts5(flat_cmd, flat_description);"#),
    // Migration 3: Create the label_suggestion table
    M::up(
        r#"CREATE TABLE label_suggestion (
            flat_root_cmd TEXT NOT NULL,
            flat_label TEXT NOT NULL,
            suggestion TEXT NOT NULL,
            usage INTEGER DEFAULT 0,
            PRIMARY KEY (flat_root_cmd, flat_label, suggestion)
        );"#,
    ),
    // Migration 4: Major refactor
    M::up(
        r#"
        -- Rename the existing tables to preserve their content
        ALTER TABLE command RENAME TO command_old;
        ALTER TABLE command_fts RENAME TO command_fts_old;
        ALTER TABLE label_suggestion RENAME TO label_suggestion_old;

        -- Create the new tables
        CREATE TABLE command (
            id BLOB PRIMARY KEY NOT NULL,
            category TEXT NOT NULL,
            source TEXT NOT NULL,
            alias TEXT NULL,
            cmd TEXT NOT NULL UNIQUE,
            flat_cmd TEXT NOT NULL,
            description TEXT NULL,
            flat_description TEXT NULL,
            tags TEXT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime() || '.000+00:00'),
            updated_at TEXT NULL
        );
        CREATE TABLE command_usage (
            command_id BLOB NOT NULL,
            path TEXT NOT NULL,
            usage_count INTEGER NOT NULL DEFAULT 1,
            PRIMARY KEY (command_id, path),
            FOREIGN KEY (command_id) REFERENCES command(id) ON DELETE CASCADE
        );
        CREATE VIRTUAL TABLE command_fuzzy_fts USING fts5(
            flat_cmd,
            flat_description,
            content='command',
            tokenize='trigram'
        );
        CREATE VIRTUAL TABLE command_fts USING fts5(
            cmd,
            description,
            content='command',
            tokenize="unicode61 remove_diacritics 2 tokenchars '-_./\:=+$@#%~'"
        );

        CREATE TABLE variable_value (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            flat_root_cmd TEXT NOT NULL,
            flat_variable TEXT NOT NULL,
            value TEXT NOT NULL,
            UNIQUE(flat_root_cmd, flat_variable, value)
        );
        CREATE TABLE variable_value_usage (
            value_id INTEGER NOT NULL,
            path TEXT NOT NULL,
            context_json TEXT NOT NULL DEFAULT '{}',
            usage_count INTEGER NOT NULL DEFAULT 1,
            PRIMARY KEY (value_id, path, context_json),
            FOREIGN KEY (value_id) REFERENCES variable_value(id) ON DELETE CASCADE
        );

        -- Indexes on the new tables to improve performance
        CREATE INDEX IF NOT EXISTS idx_command_alias ON command(alias) WHERE alias IS NOT NULL;
        CREATE INDEX IF NOT EXISTS idx_command_tags ON command(tags) WHERE tags IS NOT NULL;
        CREATE INDEX IF NOT EXISTS idx_command_usage_path ON command_usage(path);
        CREATE INDEX IF NOT EXISTS idx_command_usage_command_id ON command_usage(command_id);
        CREATE INDEX IF NOT EXISTS idx_variable_usage_path ON variable_value_usage(path);
        CREATE INDEX IF NOT EXISTS idx_variable_usage_context ON variable_value_usage(context_json);
        CREATE INDEX IF NOT EXISTS idx_variable_usage_value_id ON variable_value_usage(value_id);

        -- Triggers to keep the FTS indexes up to date
        CREATE TRIGGER command_ai_fts AFTER INSERT ON command BEGIN
            INSERT INTO command_fuzzy_fts(rowid, flat_cmd, flat_description) 
                VALUES (new.rowid, new.flat_cmd, new.flat_description);
            INSERT INTO command_fts(rowid, cmd, description) 
                VALUES (new.rowid, new.cmd, new.description);
        END;
        CREATE TRIGGER command_ad_fts AFTER DELETE ON command BEGIN
            INSERT INTO command_fuzzy_fts(command_fuzzy_fts, rowid, flat_cmd, flat_description) 
                VALUES ('delete', old.rowid, old.flat_cmd, old.flat_description);
            INSERT INTO command_fts(command_fts, rowid, cmd, description) 
                VALUES ('delete', old.rowid, old.cmd, old.description);
        END;
        CREATE TRIGGER command_au_fts AFTER UPDATE ON command BEGIN
            INSERT INTO command_fuzzy_fts(command_fuzzy_fts, rowid, flat_cmd, flat_description) 
                VALUES ('delete', old.rowid, old.flat_cmd, old.flat_description);
            INSERT INTO command_fuzzy_fts(rowid, flat_cmd, flat_description) 
                VALUES (new.rowid, new.flat_cmd, new.flat_description);
            INSERT INTO command_fts(command_fts, rowid, cmd, description) 
                VALUES ('delete', old.rowid, old.cmd, old.description);
            INSERT INTO command_fts(rowid, cmd, description) 
                VALUES (new.rowid, new.cmd, new.description);
        END;

        -- Copy data from the old tables to the new ones
        WITH RECURSIVE hashtag_parser(
            command_rowid,
            remaining_text,
            extracted_hashtag_value
        ) AS (
            -- Anchor Member:
            -- Initialize with the full description for each command that might contain hashtags.
            SELECT
                co.rowid AS command_rowid,
                co.description AS remaining_text,
                CAST(NULL AS TEXT) AS extracted_hashtag_value
            FROM command_old co
            WHERE co.description IS NOT NULL AND instr(co.description, '#') > 0

            UNION ALL

            -- Recursive Member:
            -- Extract one hashtag at a time and prepare the remaining text for the next iteration.
            SELECT
                hp.command_rowid,
                -- Determine the text remaining after the currently processed segment (tag or non-tag starting with #)
                SUBSTR(hp.remaining_text,
                    -- Start of the '#'
                    instr(hp.remaining_text, '#')
                    -- Account for the '#' character itself
                    + 1 
                    -- Length of the raw "word" part after '#' up to a space or end of string
                    + COALESCE(
                        NULLIF(instr(SUBSTR(hp.remaining_text, instr(hp.remaining_text, '#') + 1), ' '), 0) - 1,
                        LENGTH(SUBSTR(hp.remaining_text, instr(hp.remaining_text, '#') + 1))
                    )
                ) AS remaining_text,

                CASE
                    -- Check if the '#' starts a valid hashtag: first char of preceded by a space
                    WHEN (instr(hp.remaining_text, '#') = 1 OR SUBSTR(hp.remaining_text, instr(hp.remaining_text, '#') - 1, 1) = ' ')
                        -- Extract, clean, and lowercase the current hashtag
                        THEN
                            LOWER(
                                '#' || TRIM(
                                    SUBSTR(
                                        -- Text starting with raw tag word
                                        SUBSTR(hp.remaining_text, instr(hp.remaining_text, '#') + 1),
                                        1,
                                        COALESCE(
                                            -- Length until the next space
                                            NULLIF(instr(SUBSTR(hp.remaining_text, instr(hp.remaining_text, '#') + 1), ' '), 0) - 1,
                                            -- If no space, then length until the end of the string
                                            LENGTH(SUBSTR(hp.remaining_text, instr(hp.remaining_text, '#') + 1))
                                        )
                                    ),
                                    -- Characters to trim from the START/END of the tag word component
                                    '.,!?;:)[]{}''"`<>-_\/' 
                                )
                            )
                    ELSE CAST(NULL AS TEXT)
                END AS extracted_hashtag_value
            FROM hashtag_parser hp
            -- Continue recursion if a '#' is present in the remaining_text for this step
            WHERE instr(hp.remaining_text, '#') > 0
        )
        INSERT INTO command (
            rowid,
            id,
            category,
            source,
            alias,
            cmd,
            flat_cmd,
            description,
            flat_description,
            tags
        )
        SELECT
            c.rowid,
            CAST(zeroblob(8) || unhex(printf('%016X', c.rowid)) AS BLOB),
            c.category,
            CASE WHEN c.category = 'user' THEN 'user' ELSE 'tldr' END,
            c.alias,
            c.cmd,
            f.flat_cmd,
            c.description,
            f.flat_description,
            (
                SELECT NULLIF(json_group_array(DISTINCT hp.extracted_hashtag_value), '[]')
                FROM hashtag_parser hp
                WHERE hp.command_rowid = c.rowid
                    -- Only include actual extracted tags
                    AND hp.extracted_hashtag_value IS NOT NULL
                    -- Ensure tag is not just '#'
                    AND LENGTH(hp.extracted_hashtag_value) > 1
            )
        FROM command_old AS c
        JOIN command_fts_old AS f ON c.rowid = f.rowid;

        INSERT INTO command_usage (command_id, path, usage_count)
        SELECT
            CAST(zeroblob(8) || unhex(printf('%016X', c.rowid)) AS BLOB),
            '<legacy>',
            c.usage
        FROM command_old c
        WHERE c.usage > 0;

        INSERT INTO variable_value (flat_root_cmd, flat_variable, value)
        SELECT flat_root_cmd, flat_label, suggestion FROM label_suggestion_old;

        INSERT INTO variable_value_usage (value_id, path, context_json, usage_count)
        SELECT
            v.id,
            '<legacy>',
            '{}',
            o.usage
        FROM label_suggestion_old o
        JOIN variable_value v ON
            o.flat_root_cmd = v.flat_root_cmd AND
            o.flat_label = v.flat_variable AND
            o.suggestion = v.value;

        -- Drop the old tables
        DROP TABLE command_old;
        DROP TABLE command_fts_old;
        DROP TABLE label_suggestion_old;

        -- This table stores application versioning information
        CREATE TABLE IF NOT EXISTS version_info (
            latest_version TEXT NOT NULL,
            last_checked_at DATETIME NOT NULL
        );
        
        -- Insert an initial record to ensure a row always exists
        INSERT INTO version_info (latest_version, last_checked_at) VALUES ('0.0.0', datetime(0, 'unixepoch'));"#,
    ),
];

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::*;

    #[test]
    fn test_migrations_apply_successfully() -> rusqlite_migration::Result<()> {
        // Create in-memory database
        let mut conn = Connection::open_in_memory()?;

        // Apply migrations
        MIGRATIONS.to_latest(&mut conn)
    }
}
