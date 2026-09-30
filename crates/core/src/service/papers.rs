//! 文献条目 CRUD、搜索、阅读状态与回收站。

use crate::db::{err_paper, now_ts, CoreState};
use crate::error::{CoreError, CoreResult};
use crate::model::{
    ListQuery, NoteResponse, PaperDetail, PaperListItem, PatchPaper,
};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::collections::BTreeMap;

fn authors_from_json(text: &str) -> Vec<String> {
    serde_json::from_str(text).unwrap_or_default()
}

fn like_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

fn analysis_status_of(parse_status: Option<&str>) -> &'static str {
    match parse_status {
        None => "none",
        Some("ok") | Some("partial") => "comparable",
        Some(_) => "needs_formatting",
    }
}

fn base_list_sql(trash: bool) -> String {
    format!(
        "SELECT p.id, p.title, p.authors, p.year, p.doi, p.reading_status,
                a.parse_status, a.updated_at,
                EXISTS(SELECT 1 FROM notes n WHERE n.paper_id = p.id),
                p.pdf_page_count, p.trashed, p.created_at, p.updated_at
         FROM papers p
         LEFT JOIN analyses a ON a.paper_id = p.id
         WHERE p.trashed = {}",
        if trash { 1 } else { 0 }
    )
}

fn row_to_list_item(row: &Row<'_>) -> rusqlite::Result<(PaperListItem, String)> {
    let authors_json: String = row.get(2)?;
    let meta_row = PaperListItem {
        id: row.get(0)?,
        title: row.get(1)?,
        authors: authors_from_json(&authors_json),
        year: row.get(3)?,
        doi: row.get(4)?,
        reading_status: row.get(5)?,
        analysis_status: analysis_status_of(row.get::<_, Option<String>>(6)?.as_deref())
            .to_string(),
        analysis_updated_at: row.get(7)?,
        has_notes: row.get(8)?,
        tags: vec![],
        projects: vec![],
        pdf_page_count: row.get(9)?,
        trashed: row.get::<_, i64>(10)? != 0,
        matched_on: vec![],
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    };
    Ok((meta_row, authors_json))
}

fn fetch_tags(conn: &Connection, paper_id: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare(
            "SELECT t.name FROM tags t
             JOIN paper_tags pt ON pt.tag_id = t.id
             WHERE pt.paper_id = ?1 ORDER BY t.name",
        )
        .expect("查询标签");
    let names = stmt
        .query_map(params![paper_id], |r| r.get::<_, String>(0))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default();
    names
}

fn fetch_projects(conn: &Connection, paper_id: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare(
            "SELECT pr.name FROM projects pr
             JOIN paper_projects pp ON pp.project_id = pr.id
             WHERE pp.paper_id = ?1 ORDER BY pr.name",
        )
        .expect("查询项目");
    let names = stmt
        .query_map(params![paper_id], |r| r.get::<_, String>(0))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default();
    names
}

pub fn list_papers(state: &CoreState, q: &ListQuery) -> CoreResult<Vec<PaperListItem>> {
    let inner = state.inner.lock().unwrap();
    let conn = &inner.conn;
    let trash = q.trash.unwrap_or(false);

    let mut sql = base_list_sql(trash).clone();
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if !q.project.is_empty() {
        sql.push_str(
            " AND p.id IN (SELECT pp.paper_id FROM paper_projects pp
                          JOIN projects pr ON pr.id = pp.project_id WHERE pr.name = ?N)",
        );
        args.push(Box::new(q.project.clone()));
        sql = replace_nth_placeholder(&sql, args.len());
    }
    if !q.tag.is_empty() {
        sql.push_str(
            " AND p.id IN (SELECT pt.paper_id FROM paper_tags pt
                          JOIN tags t ON t.id = pt.tag_id WHERE t.name = ?N)",
        );
        args.push(Box::new(q.tag.clone()));
        sql = replace_nth_placeholder(&sql, args.len());
    }
    if !q.status.is_empty() {
        sql.push_str(" AND p.reading_status = ?N");
        args.push(Box::new(q.status.clone()));
        sql = replace_nth_placeholder(&sql, args.len());
    }
    if let Some(y) = q.year {
        sql.push_str(" AND p.year = ?N");
        args.push(Box::new(y));
        sql = replace_nth_placeholder(&sql, args.len());
    }
    match q.analysis.as_str() {
        "none" => sql.push_str(" AND a.paper_id IS NULL"),
        "comparable" => sql.push_str(" AND a.parse_status IN ('ok','partial')"),
        "needs_formatting" => sql.push_str(" AND a.parse_status = 'unparsed'"),
        _ => {}
    }
    if !q.query.trim().is_empty() {
        let kw = format!("%{}%", like_escape(q.query.trim()));
        sql.push_str(
            " AND (p.title LIKE ?N ESCAPE '\\'
                    OR p.authors LIKE ?N ESCAPE '\\'
                    OR p.doi LIKE ?N ESCAPE '\\'
                    OR p.id IN (SELECT pt.paper_id FROM paper_tags pt
                                JOIN tags t ON t.id = pt.tag_id WHERE t.name LIKE ?N ESCAPE '\\')
                    OR p.id IN (SELECT a2.paper_id FROM analyses a2
                                WHERE a2.md_content LIKE ?N ESCAPE '\\')
                    OR p.id IN (SELECT n.paper_id FROM notes n
                                WHERE n.content LIKE ?N ESCAPE '\\'))",
        );
        for _ in 0..6 {
            args.push(Box::new(kw.clone()));
            sql = replace_nth_placeholder(&sql, args.len());
        }
    }
    sql.push_str(" ORDER BY p.updated_at DESC, p.created_at DESC");

    let mut stmt = conn.prepare(&sql)?;
    let params_ref: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
    let mut items: Vec<PaperListItem> = Vec::new();
    let rows = stmt.query_map(params_ref.as_slice(), row_to_list_item)?;
    for row in rows {
        let (mut item, authors_json) = row?;
        item.tags = fetch_tags(conn, &item.id);
        item.projects = fetch_projects(conn, &item.id);
        // 记录命中来源，前端提示匹配来自哪个栏目
        if !q.query.trim().is_empty() {
            let kw = q.query.trim();
            let norm = |s: &str| s.to_lowercase();
            let kwn = norm(kw);
            if norm(&item.title).contains(&kwn) {
                item.matched_on.push("title".into());
            }
            if authors_json.to_lowercase().contains(&kwn) {
                item.matched_on.push("authors".into());
            }
            if norm(&item.doi).contains(&kwn) {
                item.matched_on.push("doi".into());
            }
            if item.tags.iter().any(|t| norm(t).contains(&kwn)) {
                item.matched_on.push("tag".into());
            }
            let md_hit: Option<String> = conn
                .query_row(
                    "SELECT md_content FROM analyses WHERE paper_id = ?1",
                    params![item.id],
                    |r| r.get(0),
                )
                .optional()
                .unwrap_or(None);
            if md_hit.map(|m| m.to_lowercase().contains(&kwn)).unwrap_or(false) {
                item.matched_on.push("analysis".into());
            }
            let note_hit: Option<String> = conn
                .query_row(
                    "SELECT content FROM notes WHERE paper_id = ?1",
                    params![item.id],
                    |r| r.get(0),
                )
                .optional()
                .unwrap_or(None);
            if note_hit.map(|m| m.to_lowercase().contains(&kwn)).unwrap_or(false) {
                item.matched_on.push("notes".into());
            }
        }
        items.push(item);
    }
    Ok(items)
}

/// 将 SQL 片段中最后一次出现的 ?N 替换为 ?<n>。
fn replace_nth_placeholder(sql: &str, n: usize) -> String {
    // 参数按追加顺序编号：每次追加占位符后把最后出现的 ?N 改为实际序号
    let marker = "?N";
    if let Some(pos) = sql.rfind(marker) {
        let mut s = String::with_capacity(sql.len() + 2);
        s.push_str(&sql[..pos]);
        s.push_str(&format!("?{}", n));
        s.push_str(&sql[pos + marker.len()..]);
        s
    } else {
        sql.to_string()
    }
}

pub fn get_paper(state: &CoreState, id: &str) -> CoreResult<PaperDetail> {
    let inner = state.inner.lock().unwrap();
    let conn = &inner.conn;
    let row = conn
        .query_row(
            "SELECT p.id, p.title, p.authors, p.year, p.doi, p.reading_status,
                    a.parse_status, a.updated_at,
                    EXISTS(SELECT 1 FROM notes n WHERE n.paper_id = p.id),
                    p.pdf_page_count, p.trashed, p.created_at, p.updated_at,
                    p.pdf_size, p.pdf_sha256, p.last_page, p.last_zoom, p.last_mode
             FROM papers p LEFT JOIN analyses a ON a.paper_id = p.id
             WHERE p.id = ?1",
            params![id],
            |row| {
                let (mut item, _) = row_to_list_item(row)?;
                item.tags = fetch_tags(conn, &item.id);
                item.projects = fetch_projects(conn, &item.id);
                Ok((
                    item,
                    row.get::<_, i64>(13)?,
                    row.get::<_, String>(14)?,
                    row.get::<_, i64>(15)?,
                    row.get::<_, f64>(16)?,
                    row.get::<_, String>(17)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| err_paper(id))?;
    Ok(PaperDetail {
        pdf_size: row.1,
        pdf_sha256: row.2,
        last_page: row.3,
        last_zoom: row.4,
        last_mode: row.5,
        item: row.0,
    })
}

pub fn patch_paper(state: &CoreState, id: &str, patch: &PatchPaper) -> CoreResult<PaperDetail> {
    let inner = state.inner.lock().unwrap();
    let conn = &inner.conn;
    let exists: bool = conn
        .query_row("SELECT 1 FROM papers WHERE id = ?1", params![id], |_| Ok(true))
        .optional()?
        .unwrap_or(false);
    if !exists {
        return Err(err_paper(id));
    }
    if let Some(t) = &patch.title {
        let t = t.trim();
        if t.is_empty() {
            return Err(CoreError::InvalidImport("标题不能为空".into()));
        }
        conn.execute("UPDATE papers SET title = ?1 WHERE id = ?2", params![t, id])?;
    }
    if let Some(a) = &patch.authors {
        conn.execute(
            "UPDATE papers SET authors = ?1 WHERE id = ?2",
            params![serde_json::to_string(a)?, id],
        )?;
    }
    if let Some(y) = patch.year {
        conn.execute("UPDATE papers SET year = ?1 WHERE id = ?2", params![y, id])?;
    }
    if let Some(d) = &patch.doi {
        conn.execute("UPDATE papers SET doi = ?1 WHERE id = ?2", params![d.trim(), id])?;
    }
    if let Some(s) = &patch.reading_status {
        conn.execute(
            "UPDATE papers SET reading_status = ?1 WHERE id = ?2",
            params![s, id],
        )?;
    }
    if let Some(last_page) = patch.last_page {
        conn.execute(
            "UPDATE papers SET last_page = MAX(1, ?1) WHERE id = ?2",
            params![last_page, id],
        )?;
    }
    if let Some(z) = patch.last_zoom {
        conn.execute("UPDATE papers SET last_zoom = ?1 WHERE id = ?2", params![z, id])?;
    }
    if let Some(m) = &patch.last_mode {
        conn.execute("UPDATE papers SET last_mode = ?1 WHERE id = ?2", params![m, id])?;
    }
    if let Some(tags) = &patch.tags {
        set_tags(conn, id, tags)?;
    }
    if let Some(projects) = &patch.projects {
        set_projects(conn, id, projects)?;
    }
    conn.execute(
        "UPDATE papers SET updated_at = ?1 WHERE id = ?2",
        params![now_ts(), id],
    )?;
    drop(inner);
    get_paper(state, id)
}

/// 标签按名称整体替换；不存在的标签自动创建。
pub fn set_tags(conn: &Connection, paper_id: &str, names: &[String]) -> CoreResult<()> {
    conn.execute("DELETE FROM paper_tags WHERE paper_id = ?1", params![paper_id])?;
    for raw in names {
        let name = raw.trim();
        if name.is_empty() {
            continue;
        }
        conn.execute("INSERT OR IGNORE INTO tags (name) VALUES (?1)", params![name])?;
        let tag_id: i64 = conn.query_row(
            "SELECT id FROM tags WHERE name = ?1",
            params![name],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO paper_tags (paper_id, tag_id) VALUES (?1, ?2)",
            params![paper_id, tag_id],
        )?;
    }
    // 清理无引用标签，保持侧栏干净
    conn.execute("DELETE FROM tags WHERE id NOT IN (SELECT DISTINCT tag_id FROM paper_tags)", [])?;
    Ok(())
}

/// 项目按名称整体替换；不存在的项目自动创建。
pub fn set_projects(conn: &Connection, paper_id: &str, names: &[String]) -> CoreResult<()> {
    conn.execute(
        "DELETE FROM paper_projects WHERE paper_id = ?1",
        params![paper_id],
    )?;
    for raw in names {
        let name = raw.trim();
        if name.is_empty() {
            continue;
        }
        conn.execute(
            "INSERT OR IGNORE INTO projects (name, created_at) VALUES (?1, ?2)",
            params![name, now_ts()],
        )?;
        let pid: i64 = conn.query_row(
            "SELECT id FROM projects WHERE name = ?1",
            params![name],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO paper_projects (paper_id, project_id) VALUES (?1, ?2)",
            params![paper_id, pid],
        )?;
    }
    Ok(())
}

pub fn trash_paper(state: &CoreState, id: &str) -> CoreResult<()> {
    let inner = state.inner.lock().unwrap();
    let n = conn_execute(&inner.conn, "UPDATE papers SET trashed = 1, updated_at = ?1 WHERE id = ?2", params![now_ts(), id])?;
    if n == 0 {
        return Err(err_paper(id));
    }
    Ok(())
}

pub fn restore_paper(state: &CoreState, id: &str) -> CoreResult<()> {
    let inner = state.inner.lock().unwrap();
    let n = conn_execute(&inner.conn, "UPDATE papers SET trashed = 0, updated_at = ?1 WHERE id = ?2", params![now_ts(), id])?;
    if n == 0 {
        return Err(err_paper(id));
    }
    Ok(())
}

/// 永久删除：仅删除托管副本与数据库记录，不影响用户原始文件。
pub fn delete_paper_permanent(state: &CoreState, id: &str) -> CoreResult<()> {
    let inner = state.inner.lock().unwrap();
    let n = conn_execute(&inner.conn, "DELETE FROM papers WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(err_paper(id));
    }
    let dir = inner.library_dir.join("papers").join(id);
    drop(inner);
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

fn conn_execute(conn: &Connection, sql: &str, p: &[&dyn rusqlite::ToSql]) -> CoreResult<usize> {
    Ok(conn.execute(sql, p)?)
}

pub fn get_note(state: &CoreState, id: &str) -> CoreResult<NoteResponse> {
    let inner = state.inner.lock().unwrap();
    let exists = paper_exists(&inner.conn, id)?;
    if !exists {
        return Err(err_paper(id));
    }
    let row = inner
        .conn
        .query_row(
            "SELECT content, updated_at FROM notes WHERE paper_id = ?1",
            params![id],
            |r| Ok(NoteResponse {
                paper_id: id.to_string(),
                content: r.get(0)?,
                updated_at: r.get(1)?,
            }),
        )
        .optional()?;
    Ok(row.unwrap_or(NoteResponse {
        paper_id: id.to_string(),
        content: String::new(),
        updated_at: 0,
    }))
}

/// 保存个人笔记。笔记独立于 AI 分析，重新导入分析不会覆盖。
pub fn set_note(state: &CoreState, id: &str, content: &str) -> CoreResult<NoteResponse> {
    let inner = state.inner.lock().unwrap();
    if !paper_exists(&inner.conn, id)? {
        return Err(err_paper(id));
    }
    inner.conn.execute(
        "INSERT INTO notes (paper_id, content, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(paper_id) DO UPDATE SET content = excluded.content, updated_at = excluded.updated_at",
        params![id, content, now_ts()],
    )?;
    Ok(NoteResponse {
        paper_id: id.to_string(),
        content: content.to_string(),
        updated_at: now_ts(),
    })
}

pub fn paper_exists(conn: &Connection, id: &str) -> CoreResult<bool> {
    let ok = conn
        .query_row("SELECT 1 FROM papers WHERE id = ?1", params![id], |_| Ok(true))
        .optional()?
        .unwrap_or(false);
    Ok(ok)
}

/// 搜索命中统计（供前端提示），key = 来源，value = 文献 ID 集合。
pub fn match_stats(items: &[PaperListItem]) -> BTreeMap<String, usize> {
    let mut m = BTreeMap::new();
    for it in items {
        for src in &it.matched_on {
            *m.entry(src.clone()).or_default() += 1;
        }
    }
    m
}
