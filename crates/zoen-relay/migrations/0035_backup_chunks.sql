-- Ciphertext pieces of an account backup. The relay records the piece, never the history.
CREATE TABLE backup_uploads (
    identity     TEXT NOT NULL REFERENCES identities(id),
    generation   TEXT NOT NULL CHECK (length(generation) = 64),
    total_bytes  BIGINT NOT NULL CHECK (total_bytes > 0 AND total_bytes <= 1073741824),
    chunk_count  INTEGER NOT NULL CHECK (chunk_count > 0 AND chunk_count <= 128),
    sha          TEXT NOT NULL CHECK (length(sha) = 64),
    PRIMARY KEY (identity, generation)
);

CREATE TABLE backup_chunks (
    identity    TEXT NOT NULL,
    generation  TEXT NOT NULL,
    idx         INTEGER NOT NULL CHECK (idx >= 0),
    sha         TEXT NOT NULL CHECK (length(sha) = 64),
    nbytes      INTEGER NOT NULL CHECK (nbytes > 0),
    object_key  TEXT NOT NULL,
    PRIMARY KEY (identity, generation, idx),
    FOREIGN KEY (identity, generation) REFERENCES backup_uploads (identity, generation)
);
