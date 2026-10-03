//! PDF 批注：高亮 / 矩形框 / 便签，坐标归一化存储（与缩放无关）。
//! 托管 PDF 副本永不修改，批注数据只存数据库（随备份迁移）。

use crate::db::{err_paper, now_ts, CoreState};
use crate::error::{CoreError, CoreResult};
use crate::model::{AnnotationInput, AnnotationPatch, AnnotationPoint, AnnotationRect, PdfAnnotation};
use crate::service::papers::paper_exists;
use rusqlite::{params, Connection, OptionalExtension};

const KINDS: [&str; 6] = ["highlight", "underline", "strike", "rect", "note", "ink"];

fn valid_color(c: &str) -> bool {
    matches!(c, "yellow" | "red" | "blue" | "green")
        || (c.starts_with('#') && c.len() == 7 && c[1..].chars().all(|x| x.is_ascii_hexdigit()))
}

fn row_to_annotation(row: &rusqlite::Row<'_>) -> rusqlite::Result<PdfAnnotation> {
    let rects_json: String = row.get(4)?;
    let strokes_json: Option<String> = row.get(10)?;
    let tags_json: Option<String> = row.get(11)?;
    Ok(PdfAnnotation {
        id: row.get(0)?,
        paper_id: row.get(1)?,
        page: row.get(2)?,
        kind: row.get(3)?,
        rects: serde_json::from_str(&rects_json).unwrap_or_default(),
        strokes: strokes_json
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default(),
        color: row.get(5)?,
        text: row.get(6)?,
        quote: row.get(7)?,
        tags: tags_json
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default(),
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

const COLS: &str = "id, paper_id, page, kind, rects, color, text, quote, created_at, updated_at, strokes, tags";

pub fn list_annotations(state: &CoreState, paper_id: &str) -> CoreResult<Vec<PdfAnnotation>> {
    let inner = state.inner.lock().unwrap();
    let mut stmt = inner.conn.prepare(&format!(
        "SELECT {COLS} FROM pdf_annotations WHERE paper_id = ?1 ORDER BY page, created_at"
    ))?;
    let rows = stmt.query_map(params![paper_id], row_to_annotation)?;
    Ok(rows.flatten().collect())
}

pub fn add_annotation(
    state: &CoreState,
    paper_id: &str,
    input: &AnnotationInput,
) -> CoreResult<PdfAnnotation> {
    if !KINDS.contains(&input.kind.as_str()) {
        return Err(CoreError::Msg(format!("未知批注类型：{}", input.kind)));
    }
    if input.rects.is_empty() && input.strokes.is_empty() {
        return Err(CoreError::Msg("批注缺少坐标区域".into()));
    }
    if input.page < 1 {
        return Err(CoreError::Msg("页码应从 1 开始".into()));
    }
    let color = input.color.clone().unwrap_or_else(|| "yellow".into());
    if !valid_color(&color) {
        return Err(CoreError::Msg(format!("不支持的颜色：{color}")));
    }
    let strokes_norm: Vec<Vec<AnnotationPoint>> = input
        .strokes
        .iter()
        .map(|path| {
            path.iter()
                .map(|pt| AnnotationPoint {
                    x: pt.x.clamp(0.0, 1.0),
                    y: pt.y.clamp(0.0, 1.0),
                })
                .collect()
        })
        .collect();
    let inner = state.inner.lock().unwrap();
    if !paper_exists(&inner.conn, paper_id)? {
        return Err(err_paper(paper_id));
    }
    let now = now_ts();
    inner.conn.execute(
        "INSERT INTO pdf_annotations (paper_id, page, kind, rects, color, text, quote, strokes, tags, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
        params![
            paper_id,
            input.page,
            input.kind,
            serde_json::to_string(&normalize(&input.rects))?,
            color,
            input.text.clone().unwrap_or_default(),
            input.quote.clone().unwrap_or_default(),
            serde_json::to_string(&strokes_norm)?,
            serde_json::to_string(&input.tags)?,
            now
        ],
    )?;
    let id = inner.conn.last_insert_rowid();
    get_by_id(&inner.conn, id)
}

/// 坐标夹取到 (0,1) 区间，避免异常输入导致渲染越界。
fn normalize(rects: &[AnnotationRect]) -> Vec<AnnotationRect> {
    rects
        .iter()
        .map(|r| AnnotationRect {
            x: r.x.clamp(0.0, 1.0),
            y: r.y.clamp(0.0, 1.0),
            w: r.w.clamp(0.0, 1.0),
            h: r.h.clamp(0.0, 1.0),
        })
        .collect()
}

fn get_by_id(conn: &Connection, id: i64) -> CoreResult<PdfAnnotation> {
    conn.query_row(
        &format!("SELECT {COLS} FROM pdf_annotations WHERE id = ?1"),
        params![id],
        row_to_annotation,
    )
    .optional()?
    .ok_or_else(|| CoreError::NotFound(format!("批注 {id} 不存在")))
}

/// 修改批注（颜色/备注文字）。
pub fn update_annotation(
    state: &CoreState,
    id: i64,
    patch: &AnnotationPatch,
) -> CoreResult<PdfAnnotation> {
    let inner = state.inner.lock().unwrap();
    if let Some(c) = &patch.color {
        if !valid_color(c) {
            return Err(CoreError::Msg(format!("不支持的颜色：{c}")));
        }
    }
    let n = inner.conn.execute(
        "UPDATE pdf_annotations SET
            color = COALESCE(?1, color),
            text = COALESCE(?2, text),
            tags = COALESCE(?3, tags),
            updated_at = ?4
         WHERE id = ?5",
        params![
            patch.color,
            patch.text,
            patch.tags.as_ref().map(|t| serde_json::to_string(t).unwrap_or_default()),
            now_ts(),
            id
        ],
    )?;
    if n == 0 {
        return Err(CoreError::NotFound(format!("批注 {id} 不存在")));
    }
    get_by_id(&inner.conn, id)
}

pub fn delete_annotation(state: &CoreState, id: i64) -> CoreResult<()> {
    let inner = state.inner.lock().unwrap();
    let n = inner
        .conn
        .execute("DELETE FROM pdf_annotations WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(CoreError::NotFound(format!("批注 {id} 不存在")));
    }
    Ok(())
}

/// 供导出/统计用：某文献批注数。
pub fn count_annotations(conn: &Connection, paper_id: &str) -> CoreResult<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM pdf_annotations WHERE paper_id = ?1",
        params![paper_id],
        |r| r.get(0),
    )?)
}
