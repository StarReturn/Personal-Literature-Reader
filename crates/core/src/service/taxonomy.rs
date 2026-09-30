//! 项目与标签管理。

use crate::db::{now_ts, CoreState};
use crate::error::{CoreError, CoreResult};
use crate::model::{ProjectInfo, TagInfo};
use rusqlite::params;

pub fn list_projects(state: &CoreState) -> CoreResult<Vec<ProjectInfo>> {
    let inner = state.inner.lock().unwrap();
    let mut stmt = inner.conn.prepare(
        "SELECT pr.id, pr.name,
                (SELECT COUNT(*) FROM paper_projects pp
                 JOIN papers p ON p.id = pp.paper_id
                 WHERE pp.project_id = pr.id AND p.trashed = 0)
         FROM projects pr ORDER BY pr.name",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(ProjectInfo {
            id: r.get(0)?,
            name: r.get(1)?,
            paper_count: r.get(2)?,
        })
    })?;
    Ok(rows.flatten().collect())
}

pub fn create_project(state: &CoreState, name: &str) -> CoreResult<ProjectInfo> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CoreError::Msg("项目名不能为空".into()));
    }
    let inner = state.inner.lock().unwrap();
    inner
        .conn
        .execute("INSERT OR IGNORE INTO projects (name, created_at) VALUES (?1, ?2)", params![name, now_ts()])?;
    let id: i64 = inner.conn.query_row(
        "SELECT id FROM projects WHERE name = ?1",
        params![name],
        |r| r.get(0),
    )?;
    Ok(ProjectInfo {
        id,
        name: name.to_string(),
        paper_count: 0,
    })
}

pub fn rename_project(state: &CoreState, id: i64, new_name: &str) -> CoreResult<()> {
    let name = new_name.trim();
    if name.is_empty() {
        return Err(CoreError::Msg("项目名不能为空".into()));
    }
    let inner = state.inner.lock().unwrap();
    let n = inner
        .conn
        .execute("UPDATE projects SET name = ?1 WHERE id = ?2", params![name, id])?;
    if n == 0 {
        return Err(CoreError::NotFound(format!("项目 {} 不存在", id)));
    }
    Ok(())
}

/// 删除项目只取消归类关系，不删除任何文献。
pub fn delete_project(state: &CoreState, id: i64) -> CoreResult<()> {
    let inner = state.inner.lock().unwrap();
    let n = inner
        .conn
        .execute("DELETE FROM projects WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(CoreError::NotFound(format!("项目 {} 不存在", id)));
    }
    Ok(())
}

pub fn list_tags(state: &CoreState) -> CoreResult<Vec<TagInfo>> {
    let inner = state.inner.lock().unwrap();
    let mut stmt = inner.conn.prepare(
        "SELECT t.id, t.name,
                (SELECT COUNT(*) FROM paper_tags pt
                 JOIN papers p ON p.id = pt.paper_id
                 WHERE pt.tag_id = t.id AND p.trashed = 0)
         FROM tags t ORDER BY t.name",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(TagInfo {
            id: r.get(0)?,
            name: r.get(1)?,
            paper_count: r.get(2)?,
        })
    })?;
    Ok(rows.flatten().collect())
}

pub fn delete_tag(state: &CoreState, id: i64) -> CoreResult<()> {
    let inner = state.inner.lock().unwrap();
    let n = inner
        .conn
        .execute("DELETE FROM tags WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(CoreError::NotFound(format!("标签 {} 不存在", id)));
    }
    Ok(())
}
