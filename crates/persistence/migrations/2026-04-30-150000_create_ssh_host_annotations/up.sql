CREATE TABLE ssh_host_annotations (
    alias TEXT PRIMARY KEY NOT NULL,
    tags TEXT NOT NULL DEFAULT '[]',
    color TEXT,
    notes TEXT,
    last_connected_at BIGINT,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL
);
