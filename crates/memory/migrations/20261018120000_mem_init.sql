-- 核心记忆：单行，每次都进上下文，限 2k token（决策 005）。
CREATE TABLE mem_core (
    id         INTEGER PRIMARY KEY CHECK (id = 1),
    content    TEXT NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

-- 记忆条目：agent 按需检索。
CREATE TABLE mem_entry (
    id         TEXT PRIMARY KEY,
    content    TEXT NOT NULL,
    created_at INTEGER NOT NULL
) STRICT;

-- FTS5 外部内容表；trigram 分词，中文无需分词器也能子串检索。
CREATE VIRTUAL TABLE mem_entry_fts USING fts5(
    content, content='mem_entry', content_rowid='rowid', tokenize='trigram'
);

CREATE TRIGGER mem_entry_ai AFTER INSERT ON mem_entry BEGIN
    INSERT INTO mem_entry_fts(rowid, content) VALUES (new.rowid, new.content);
END;
CREATE TRIGGER mem_entry_ad AFTER DELETE ON mem_entry BEGIN
    INSERT INTO mem_entry_fts(mem_entry_fts, rowid, content) VALUES ('delete', old.rowid, old.content);
END;
