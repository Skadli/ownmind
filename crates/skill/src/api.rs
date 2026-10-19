//! 公共入口：其他模块只能用这里导出的东西。
//! 技能 = 带 SKILL.md（front matter 有 name、description）的目录。
//! 预设技能在沙盒镜像的只读目录；已装技能在沙盒工作目录 `.skills/installed/<name>/`，升级镜像不会覆盖。

use ownmind_kernel::{new_id, now_ms};
use ownmind_sandbox::api::{ExecArgs, Sandbox, SandboxError};
use serde::Serialize;
use sqlx::SqlitePool;

/// 本模块的数据库迁移，由 server 合并执行。
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// 预设技能在沙盒镜像里的默认目录。
pub const DEFAULT_BUILTIN_DIR: &str = "/opt/ownmind/skills/builtin";
const INSTALLED_DIR: &str = ".skills/installed";
const DESC_MAX_CHARS: usize = 200;

#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    #[error("{0}")]
    Invalid(String),
    #[error("安装失败：{0}")]
    Install(String),
    #[error("沙盒：{0}")]
    Sandbox(#[from] SandboxError),
    #[error("数据库错误：{0}")]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    /// 沙盒内 SKILL.md 的路径，agent 需要时读它。
    pub path: String,
    /// builtin 或 installed。
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Installed {
    pub name: String,
    pub url: String,
    pub installed_at: i64,
}

#[derive(Clone)]
pub struct Skills {
    db: SqlitePool,
    sandbox: Sandbox,
    builtin_dir: String,
}

/// 技能名：小写字母数字开头，只含小写字母、数字、`-`、`_`。
pub fn valid_name(name: &str) -> bool {
    let mut cs = name.chars();
    matches!(cs.next(), Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit())
        && name.len() <= 64
        && cs.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// 只收 http(s) 的 Git 链接，字符限制在 URL 常见字符，避免注入命令。
pub fn valid_git_url(url: &str) -> bool {
    (url.starts_with("https://") || url.starts_with("http://"))
        && url.len() <= 300
        && url
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._~:/@%+-".contains(c))
        && !url.contains("//-")
}

/// 解析 SKILL.md 的 front matter，返回 (name, description)。
pub fn parse_front_matter(text: &str) -> Option<(String, String)> {
    let mut lines = text.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    let (mut name, mut desc) = (None, None);
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let v = v.trim().trim_matches(|c| c == '"' || c == '\'').trim();
        match k.trim() {
            "name" => name = Some(v.to_owned()),
            "description" => desc = Some(v.chars().take(DESC_MAX_CHARS).collect::<String>()),
            _ => {}
        }
    }
    Some((name.filter(|n| !n.is_empty())?, desc.unwrap_or_default()))
}

/// 解析索引脚本的输出：每个技能以 `@@@<路径>` 起头，后接 SKILL.md 开头几行。
fn parse_index(out: &str, builtin_dir: &str) -> Vec<SkillInfo> {
    out.split("@@@")
        .filter_map(|chunk| {
            let (path, body) = chunk.split_once('\n')?;
            let (name, description) = parse_front_matter(body)?;
            let path = path.trim().to_owned();
            let source = if path.starts_with(builtin_dir) {
                "builtin"
            } else {
                "installed"
            };
            Some(SkillInfo {
                name,
                description,
                path,
                source: source.into(),
            })
        })
        .collect()
}

impl Skills {
    pub fn new(db: SqlitePool, sandbox: Sandbox, builtin_dir: &str) -> Self {
        Self {
            db,
            sandbox,
            builtin_dir: builtin_dir.trim_end_matches('/').into(),
        }
    }

    async fn sh(&self, command: String) -> Result<ownmind_sandbox::api::ExecResult, SkillError> {
        Ok(self
            .sandbox
            .exec(&ExecArgs {
                command,
                timeout_secs: Some(120),
                ..Default::default()
            })
            .await?)
    }

    /// 技能索引（预设 + 已装），上下文第 3 层用。
    pub async fn index(&self) -> Result<Vec<SkillInfo>, SkillError> {
        let script = format!(
            "for f in '{b}'/*/SKILL.md {i}/*/SKILL.md; do [ -f \"$f\" ] || continue; \
             echo \"@@@$f\"; head -n 12 \"$f\"; done",
            b = self.builtin_dir,
            i = INSTALLED_DIR
        );
        let r = self.sh(script).await?;
        let mut v = parse_index(&r.stdout, &self.builtin_dir);
        v.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(v)
    }

    pub async fn list_installed(&self) -> Result<Vec<Installed>, SkillError> {
        Ok(
            sqlx::query_as("SELECT name, url, installed_at FROM skill_installed ORDER BY name")
                .fetch_all(&self.db)
                .await?,
        )
    }

    /// 在沙盒里 git clone 安装；校验有 SKILL.md 且 name 合法，同名覆盖（即升级）。
    pub async fn install(&self, url: &str) -> Result<Installed, SkillError> {
        if !valid_git_url(url) {
            return Err(SkillError::Invalid(
                "只支持 http(s) 的 Git 链接，例如 https://github.com/用户/仓库".into(),
            ));
        }
        let tmp = format!(".skills/tmp-{}", new_id());
        let r = self
            .sh(format!(
                "mkdir -p {INSTALLED_DIR} && rm -rf {tmp} && \
                 export GIT_TERMINAL_PROMPT=0; \
                 {{ git clone --depth 1 -- '{url}' {tmp} || {{ rm -rf {tmp}; git clone -- '{url}' {tmp}; }}; }} 2>&1"
            ))
            .await?;
        if r.exit_code != Some(0) {
            let _ = self.sh(format!("rm -rf {tmp}")).await;
            return Err(SkillError::Install(format!(
                "git clone 没成功：{}",
                r.stdout.trim().chars().take(300).collect::<String>()
            )));
        }
        match self.finish_install(url, &tmp).await {
            Ok(i) => Ok(i),
            Err(e) => {
                let _ = self.sh(format!("rm -rf {tmp}")).await;
                Err(e)
            }
        }
    }

    async fn finish_install(&self, url: &str, tmp: &str) -> Result<Installed, SkillError> {
        let md = self
            .sandbox
            .read(&format!("{tmp}/SKILL.md"))
            .await
            .map_err(|_| SkillError::Install("仓库根目录没有 SKILL.md".into()))?;
        let (name, _) = parse_front_matter(&md.content).ok_or_else(|| {
            SkillError::Install("SKILL.md 缺少 front matter（name、description）".into())
        })?;
        if !valid_name(&name) {
            return Err(SkillError::Install(format!(
                "技能名 {name:?} 不合法：只能用小写字母、数字、- 和 _"
            )));
        }
        let r = self
            .sh(format!(
                "rm -rf {INSTALLED_DIR}/{name} && mv {tmp} {INSTALLED_DIR}/{name}"
            ))
            .await?;
        if r.exit_code != Some(0) {
            return Err(SkillError::Install(r.stderr.trim().to_owned()));
        }
        let at = now_ms();
        sqlx::query(
            "INSERT INTO skill_installed (name, url, installed_at) VALUES (?, ?, ?) \
             ON CONFLICT(name) DO UPDATE SET url = excluded.url, installed_at = excluded.installed_at",
        )
        .bind(&name)
        .bind(url)
        .bind(at)
        .execute(&self.db)
        .await?;
        Ok(Installed {
            name,
            url: url.to_owned(),
            installed_at: at,
        })
    }

    /// 卸载已装技能；预设技能不能卸载。返回是否存在。
    pub async fn remove(&self, name: &str) -> Result<bool, SkillError> {
        if !valid_name(name) {
            return Err(SkillError::Invalid("技能名不合法".into()));
        }
        let done = sqlx::query("DELETE FROM skill_installed WHERE name = ?")
            .bind(name)
            .execute(&self.db)
            .await?
            .rows_affected();
        if done == 0 {
            return Ok(false);
        }
        self.sh(format!("rm -rf {INSTALLED_DIR}/{name}")).await?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_matter() {
        let t = "---\nname: demo\ndescription: \"做演示\"\n---\n正文";
        assert_eq!(
            parse_front_matter(t),
            Some(("demo".into(), "做演示".into()))
        );
        assert_eq!(parse_front_matter("没有"), None);
        assert_eq!(parse_front_matter("---\ndescription: x\n---\n"), None);
    }

    #[test]
    fn names_and_urls() {
        assert!(valid_name("research-report"));
        assert!(!valid_name("../x") && !valid_name("A") && !valid_name(""));
        assert!(valid_git_url("https://github.com/a/b.git"));
        assert!(!valid_git_url("https://x.com/a;rm -rf /"));
        assert!(!valid_git_url("file:///etc") && !valid_git_url("ssh://git@x/y"));
        assert!(!valid_git_url("https://x.com/a'b"));
    }

    #[test]
    fn index_parsing() {
        let out = "@@@/opt/b/x/SKILL.md\n---\nname: x\ndescription: 甲\n---\n@@@.skills/installed/y/SKILL.md\n---\nname: y\n---\n";
        let v = parse_index(out, "/opt/b");
        assert_eq!(v.len(), 2);
        assert_eq!(
            (v[0].source.as_str(), v[1].source.as_str()),
            ("builtin", "installed")
        );
        assert_eq!(v[1].description, "");
    }
}
