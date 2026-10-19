-- 已装技能登记；技能文件在沙盒 .skills/installed/<name>/。
CREATE TABLE skill_installed (
    name         TEXT PRIMARY KEY,
    url          TEXT NOT NULL,
    installed_at INTEGER NOT NULL
) STRICT;
