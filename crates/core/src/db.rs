use crate::error::{CoreError, CoreResult};
use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// 运行时核心状态：当前资料库目录 + SQLite 连接。
/// 连接由互斥锁保护，备份/恢复期间持有写锁以满足一致性要求。
pub struct CoreState {
    pub inner: Mutex<LibraryInner>,
}

pub struct LibraryInner {
    pub library_dir: PathBuf,
    pub conn: Connection,
}

pub struct Settings {
    pub library_dir: Option<PathBuf>,
}

pub fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 应用数据目录（配置与临时导入文件所在）。优先环境变量 LITREVIEW_DATA。
pub fn app_data_dir() -> PathBuf {
    if let Ok(p) = std::env::var("LITREVIEW_DATA") {
        return PathBuf::from(p);
    }
    PathBuf::from("data")
}

pub fn config_path() -> PathBuf {
    app_data_dir().join("config.json")
}

pub fn load_settings() -> Settings {
    let p = config_path();
    if let Ok(text) = fs::read_to_string(&p) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            let dir = v
                .get("library_dir")
                .and_then(|x| x.as_str())
                .map(PathBuf::from);
            return Settings { library_dir: dir };
        }
    }
    Settings { library_dir: None }
}

pub fn save_settings(dir: &Path) -> CoreResult<()> {
    let p = config_path();
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent)?;
    }
    let v = serde_json::json!({ "library_dir": dir.to_string_lossy() });
    fs::write(&p, serde_json::to_string_pretty(&v)?)?;
    Ok(())
}

pub fn default_library_dir() -> PathBuf {
    app_data_dir().join("LiteratureLibrary")
}

pub fn pdf_storage_path(library_dir: &Path, paper_id: &str) -> PathBuf {
    library_dir.join("papers").join(paper_id).join("original.pdf")
}

/// 打开（或初始化）资料库。目录不存在时自动创建。
/// 相对路径会基于当前工作目录转为绝对路径，保证备份/恢复切换后仍指向同一位置。
pub fn open_library(dir: Option<&Path>) -> CoreResult<CoreState> {
    let mut library_dir = match dir {
        Some(d) => d.to_path_buf(),
        None => load_settings().library_dir.unwrap_or_else(default_library_dir),
    };
    if library_dir.is_relative() {
        if let Ok(cwd) = std::env::current_dir() {
            library_dir = cwd.join(library_dir);
        }
    }
    fs::create_dir_all(library_dir.join("papers"))?;
    let conn = Connection::open(library_dir.join("library.sqlite"))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrate(&conn)?;
    let state = CoreState {
        inner: Mutex::new(LibraryInner { library_dir, conn }),
    };
    cleanup_temp_imports()?;
    Ok(state)
}

pub(crate) fn migrate(conn: &Connection) -> CoreResult<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS papers (
            id              TEXT PRIMARY KEY,
            title           TEXT NOT NULL,
            authors         TEXT NOT NULL DEFAULT '[]',
            year            INTEGER,
            doi             TEXT NOT NULL DEFAULT '',
            pdf_rel_path    TEXT NOT NULL,
            pdf_size        INTEGER NOT NULL DEFAULT 0,
            pdf_sha256      TEXT NOT NULL,
            pdf_page_count  INTEGER,
            reading_status  TEXT NOT NULL DEFAULT 'to_read',
            last_page       INTEGER NOT NULL DEFAULT 1,
            last_zoom       REAL NOT NULL DEFAULT 1.0,
            last_mode       TEXT NOT NULL DEFAULT 'split',
            trashed         INTEGER NOT NULL DEFAULT 0,
            created_at      INTEGER NOT NULL,
            updated_at      INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS analyses (
            paper_id        TEXT PRIMARY KEY REFERENCES papers(id) ON DELETE CASCADE,
            schema_version  INTEGER,
            md_content      TEXT NOT NULL,
            prev_md_content TEXT,
            parse_status    TEXT NOT NULL DEFAULT 'ok',
            imported_at     INTEGER NOT NULL,
            updated_at      INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS projects (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            name       TEXT NOT NULL UNIQUE,
            created_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS paper_projects (
            paper_id   TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
            project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            PRIMARY KEY (paper_id, project_id)
        );

        CREATE TABLE IF NOT EXISTS tags (
            id   INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE
        );

        CREATE TABLE IF NOT EXISTS paper_tags (
            paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
            tag_id   INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
            PRIMARY KEY (paper_id, tag_id)
        );

        CREATE TABLE IF NOT EXISTS notes (
            paper_id   TEXT PRIMARY KEY REFERENCES papers(id) ON DELETE CASCADE,
            content    TEXT NOT NULL DEFAULT '',
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS evidence (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            paper_id     TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
            anchor_hash  TEXT NOT NULL,
            section      TEXT NOT NULL DEFAULT '',
            excerpt      TEXT NOT NULL DEFAULT '',
            page         INTEGER,
            check_status TEXT NOT NULL DEFAULT 'unverified',
            stale        INTEGER NOT NULL DEFAULT 0,
            updated_at   INTEGER NOT NULL,
            UNIQUE (paper_id, anchor_hash)
        );

        CREATE TABLE IF NOT EXISTS compares (
            id         TEXT PRIMARY KEY,
            name       TEXT NOT NULL,
            paper_ids  TEXT NOT NULL DEFAULT '[]',
            dimensions TEXT NOT NULL DEFAULT '[]',
            synthesis  TEXT NOT NULL DEFAULT '{}',
            snapshots  TEXT NOT NULL DEFAULT '{}',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS pdf_annotations (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            paper_id   TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
            page       INTEGER NOT NULL,
            kind       TEXT NOT NULL,               -- highlight | rect | note
            rects      TEXT NOT NULL,               -- JSON [{x,y,w,h}]，页面归一化坐标(0-1)
            color      TEXT NOT NULL DEFAULT 'yellow',
            text       TEXT NOT NULL DEFAULT '',    -- 备注/便签文字
            quote      TEXT NOT NULL DEFAULT '',    -- 高亮对应的原文摘录
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_papers_sha ON papers(pdf_sha256);
        CREATE INDEX IF NOT EXISTS idx_papers_doi ON papers(doi);
        CREATE INDEX IF NOT EXISTS idx_evidence_paper ON evidence(paper_id);
        "#,
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO meta (key, value) VALUES ('schema_version', '1')",
        [],
    )?;
    Ok(())
}

/// 清理超过 24 小时的导入临时文件。
pub fn cleanup_temp_imports() -> CoreResult<()> {
    let temp_dir = app_data_dir().join("tmp-import");
    if !temp_dir.exists() {
        return Ok(());
    }
    let cutoff = now_ts() - 24 * 3600;
    if let Ok(entries) = fs::read_dir(&temp_dir) {
        for entry in entries.flatten() {
            let ok = entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| (d.as_secs() as i64) < cutoff)
                .unwrap_or(false);
            if ok {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
    }
    Ok(())
}

/// 规范化 DOI：去空格、统一小写、去掉 https://doi.org/ 前缀。
pub fn normalize_doi(doi: &str) -> String {
    let mut d = doi.trim().to_lowercase();
    for prefix in ["https://doi.org/", "http://doi.org/", "doi.org/", "doi:"] {
        if let Some(rest) = d.strip_prefix(prefix) {
            d = rest.to_string();
            break;
        }
    }
    d
}

/// 标题相似度探测用的规范化：仅保留字母数字（含中文）并小写。
pub fn title_key(title: &str) -> String {
    title
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

pub fn err_paper(id: &str) -> CoreError {
    CoreError::PaperNotFound(id.to_string())
}
