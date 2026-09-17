-- The index of where to look (D4). Never the answer to what is installed - that is read
-- off disk every time, because a cached copy going stale is the exact failure this tool
-- exists to catch.

CREATE TABLE repos (
    id              INTEGER PRIMARY KEY,
    -- The main worktree, canonicalised. A worktree is recorded against its repo rather
    -- than as a project of its own, so four branches in flight are still one entry.
    path            TEXT NOT NULL UNIQUE,
    first_seen      TEXT NOT NULL,
    last_seen       TEXT NOT NULL,
    -- When the toolkit last wrote here. NULL for a repo that has only been discovered,
    -- which is what distinguishes "I found this" from "you set this up".
    last_configured TEXT
);

-- Where to go looking for repos nobody has configured yet.
CREATE TABLE scan_roots (
    path     TEXT PRIMARY KEY,
    added_at TEXT NOT NULL
);
