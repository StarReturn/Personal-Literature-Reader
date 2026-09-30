//! 对比集合：文献组合、维度、个人综合结论与"分析已更新"提示。

use crate::db::{now_ts, CoreState};
use crate::error::{CoreError, CoreResult};
use crate::model::{CompareInput, CompareRecord, CompareWithStatus, Synthesis};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::BTreeMap;

pub const DEFAULT_DIMENSIONS: [&str; 6] = [
    "研究问题",
    "研究方法",
    "样本与数据",
    "主要发现",
    "创新点",
    "局限性",
];

pub const MAX_PAPERS: usize = 5;

fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<CompareRecord> {
    Ok(CompareRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        paper_ids: serde_json::from_str(&row.get::<_, String>(2)?).unwrap_or_default(),
        dimensions: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
        synthesis: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
        snapshots: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default(),
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

const COLS: &str = "id, name, paper_ids, dimensions, synthesis, snapshots, created_at, updated_at";

pub fn list_compares(state: &CoreState) -> CoreResult<Vec<CompareRecord>> {
    let inner = state.inner.lock().unwrap();
    let mut stmt = inner
        .conn
        .prepare(&format!("SELECT {COLS} FROM compares ORDER BY updated_at DESC"))?;
    let rows = stmt.query_map([], row_to_record)?;
    Ok(rows.flatten().collect())
}

pub fn create_compare(state: &CoreState, input: &CompareInput) -> CoreResult<CompareRecord> {
    if input.paper_ids.len() > MAX_PAPERS {
        return Err(CoreError::Msg(format!(
            "对比最多支持 {} 篇文献",
            MAX_PAPERS
        )));
    }
    let id = uuid::Uuid::new_v4().simple().to_string();
    let name = if input.name.trim().is_empty() {
        "未命名对比".to_string()
    } else {
        input.name.trim().to_string()
    };
    let dimensions = if input.dimensions.is_empty() {
        DEFAULT_DIMENSIONS.iter().map(|s| s.to_string()).collect()
    } else {
        input.dimensions.clone()
    };
    let now = now_ts();
    {
        let inner = state.inner.lock().unwrap();
        inner.conn.execute(
            "INSERT INTO compares (id, name, paper_ids, dimensions, synthesis, snapshots, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, '{}', ?6, ?6)",
            params![
                id,
                name,
                serde_json::to_string(&input.paper_ids)?,
                serde_json::to_string(&dimensions)?,
                serde_json::to_string(&input.synthesis)?,
                now
            ],
        )?;
    }
    get_compare_record(state, &id)
}

/// 读取对比并在读取时刷新快照；返回哪些文献的分析已更新（提示重新核查）。
pub fn get_compare(state: &CoreState, id: &str) -> CoreResult<CompareWithStatus> {
    let record = get_compare_record(state, id)?;
    let mut updated_papers: Vec<String> = Vec::new();
    {
        let inner = state.inner.lock().unwrap();
        let conn = &inner.conn;
        let mut new_snapshots: BTreeMap<String, i64> = BTreeMap::new();
        for pid in &record.paper_ids {
            let current: Option<i64> = conn
                .query_row(
                    "SELECT updated_at FROM analyses WHERE paper_id = ?1",
                    params![pid],
                    |r| r.get(0),
                )
                .optional()?;
            match current {
                Some(ts) => {
                    let seen = record.snapshots.get(pid);
                    // 快照缺失视为从未查看过；时间戳变化视为已更新
                    if seen.map(|s| *s != ts).unwrap_or(false) {
                        updated_papers.push(pid.clone());
                    }
                    new_snapshots.insert(pid.clone(), ts);
                }
                None => {
                    // 文献当前没有分析（例如从未导入或被清空）
                    new_snapshots.insert(pid.clone(), 0);
                }
            }
        }
        conn.execute(
            "UPDATE compares SET snapshots = ?1 WHERE id = ?2",
            params![serde_json::to_string(&new_snapshots)?, id],
        )?;
    }
    Ok(CompareWithStatus {
        record,
        updated_papers,
    })
}

pub fn get_compare_record(state: &CoreState, id: &str) -> CoreResult<CompareRecord> {
    let inner = state.inner.lock().unwrap();
    let rec = inner
        .conn
        .query_row(
            &format!("SELECT {COLS} FROM compares WHERE id = ?1"),
            params![id],
            row_to_record,
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound(format!("对比 {} 不存在", id)))?;
    Ok(rec)
}

/// 保存对比：文献顺序、维度、综合结论。不触碰快照与已核查状态。
pub fn update_compare(state: &CoreState, id: &str, input: &CompareInput) -> CoreResult<CompareRecord> {
    if input.paper_ids.len() > MAX_PAPERS {
        return Err(CoreError::Msg(format!(
            "对比最多支持 {} 篇文献",
            MAX_PAPERS
        )));
    }
    let name = if input.name.trim().is_empty() {
        "未命名对比".to_string()
    } else {
        input.name.trim().to_string()
    };
    let dimensions = if input.dimensions.is_empty() {
        DEFAULT_DIMENSIONS.iter().map(|s| s.to_string()).collect()
    } else {
        input.dimensions.clone()
    };
    {
        let inner = state.inner.lock().unwrap();
        let n = inner.conn.execute(
            "UPDATE compares SET name = ?1, paper_ids = ?2, dimensions = ?3, synthesis = ?4,
                    updated_at = ?5 WHERE id = ?6",
            params![
                name,
                serde_json::to_string(&input.paper_ids)?,
                serde_json::to_string(&dimensions)?,
                serde_json::to_string(&input.synthesis)?,
                now_ts(),
                id
            ],
        )?;
        if n == 0 {
            return Err(CoreError::NotFound(format!("对比 {} 不存在", id)));
        }
    }
    get_compare_record(state, id)
}

pub fn delete_compare(state: &CoreState, id: &str) -> CoreResult<()> {
    let inner = state.inner.lock().unwrap();
    let n = inner
        .conn
        .execute("DELETE FROM compares WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(CoreError::NotFound(format!("对比 {} 不存在", id)));
    }
    Ok(())
}

/// 空综合结论判断（供导出时跳过空栏目）。
pub fn synthesis_is_empty(s: &Synthesis) -> bool {
    s.agreement.trim().is_empty()
        && s.differences.trim().is_empty()
        && s.gap.trim().is_empty()
        && s.conclusion.trim().is_empty()
}

/// 供备份/恢复后校验使用。
pub fn count_compares(conn: &Connection) -> CoreResult<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM compares", [], |r| r.get(0))?)
}
