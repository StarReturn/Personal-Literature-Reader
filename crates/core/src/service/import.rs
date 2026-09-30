//! 导入流程：临时暂存 → 预览解析 → 正式入库；分析更新与证据核查状态。

use crate::db::{
    app_data_dir, err_paper, normalize_doi, now_ts, pdf_storage_path, sha256_hex, title_key,
    CoreState,
};
use crate::error::{CoreError, CoreResult};
use crate::mdparse::{parse_analysis, ParsedAnalysis};
use crate::model::{
    AnalysisResponse, CommitRequest, CommitResult, DuplicateHit, EvidenceInput, EvidenceRecord,
    ImportPdfInfo, ImportPreview,
};
use crate::service::papers::{paper_exists, set_projects, set_tags};
use rusqlite::{params, Connection, OptionalExtension};
use std::fs;
use std::path::PathBuf;

fn temp_import_dir(token: &str) -> PathBuf {
    app_data_dir().join("tmp-import").join(token)
}

fn count_pdf_pages(bytes: &[u8]) -> Result<i64, String> {
    // lopdf 只做页对象计数；加密或损坏的 PDF 返回可读错误，不阻断导入
    match lopdf::Document::load_mem(bytes) {
        Ok(doc) => Ok(doc.get_pages().len() as i64),
        Err(e) => Err(format!("无法读取 PDF 页数：{e}")),
    }
}

/// 第一步：上传 PDF / 粘贴或上传 MD，暂存并解析，返回预览。不写入库。
pub fn analyze_import(
    state: &CoreState,
    pdf_bytes: Option<Vec<u8>>,
    pdf_filename: Option<String>,
    md_text: Option<String>,
) -> CoreResult<ImportPreview> {
    let token = uuid::Uuid::new_v4().simple().to_string();
    let dir = temp_import_dir(&token);
    fs::create_dir_all(&dir)?;

    let mut pdf_info: Option<ImportPdfInfo> = None;
    if let Some(bytes) = &pdf_bytes {
        fs::write(dir.join("pdf.bin"), bytes)?;
        let (page_count, page_count_error) = match count_pdf_pages(bytes) {
            Ok(n) => (Some(n), None),
            Err(e) => (None, Some(e)),
        };
        pdf_info = Some(ImportPdfInfo {
            sha256: sha256_hex(bytes),
            size: bytes.len() as u64,
            page_count,
            page_count_error,
        });
    }

    let mut parsed: Option<ParsedAnalysis> = None;
    if let Some(text) = &md_text {
        fs::write(dir.join("analysis.md"), text)?;
        parsed = Some(parse_analysis(text));
    }
    let meta = serde_json::json!({
        "pdf_filename": pdf_filename,
        "has_pdf": pdf_bytes.is_some(),
        "has_md": md_text.is_some(),
    });
    fs::write(dir.join("meta.json"), meta.to_string())?;

    // 页码链接越界校验
    let mut page_warnings: Vec<String> = Vec::new();
    if let (Some(info), Some(p)) = (&pdf_info, &parsed) {
        if let Some(pages) = info.page_count {
            for link in &p.page_links {
                if link.page == 0 {
                    page_warnings.push(format!("页码链接「{}」页号为 0，应从 1 开始", link.context));
                } else if link.page as i64 > pages {
                    page_warnings.push(format!(
                        "页码链接指向第 {} 页，超出 PDF 总页数 {}，导入后请修正",
                        link.page, pages
                    ));
                }
            }
        }
    }

    // 配对提示：MD 的 source_pdf 与所选 PDF 文件名是否一致
    let pairing_hint = match (&pdf_filename, &parsed) {
        (Some(pdf_name), Some(p)) if !p.meta.source_pdf.is_empty() => {
            let stem = |s: &str| {
                PathBuf::from(s)
                    .file_stem()
                    .map(|x| x.to_string_lossy().to_lowercase())
                    .unwrap_or_default()
            };
            if stem(pdf_name) == stem(&p.meta.source_pdf) {
                None
            } else {
                Some(format!(
                    "MD 中 source_pdf 为「{}」，与所选 PDF「{}」不一致，请确认是否为同一篇论文",
                    p.meta.source_pdf, pdf_name
                ))
            }
        }
        (Some(_), Some(p)) if p.meta.source_pdf.is_empty() => {
            Some("MD 未填写 source_pdf，无法自动核对配对关系".into())
        }
        _ => None,
    };

    // 重复检测
    let mut duplicates: Vec<DuplicateHit> = Vec::new();
    {
        let inner = state.inner.lock().unwrap();
        let conn = &inner.conn;
        if let Some(info) = &pdf_info {
            let mut stmt = conn.prepare("SELECT id, title FROM papers WHERE pdf_sha256 = ?1")?;
            let rows = stmt.query_map(params![info.sha256], |r| {
                Ok(DuplicateHit {
                    paper_id: r.get(0)?,
                    title: r.get(1)?,
                    reason: "sha256".into(),
                })
            })?;
            duplicates.extend(rows.flatten());
        }
        if let Some(p) = &parsed {
            let nd = normalize_doi(&p.meta.doi);
            if !nd.is_empty() {
                let mut stmt = conn
                    .prepare("SELECT id, title FROM papers WHERE lower(trim(doi)) = ?1")?;
                let rows = stmt.query_map(params![nd], |r| {
                    Ok(DuplicateHit {
                        paper_id: r.get(0)?,
                        title: r.get(1)?,
                        reason: "doi".into(),
                    })
                })?;
                duplicates.extend(rows.flatten());
            }
            let tk = title_key(&p.meta.title);
            if tk.len() >= 8 {
                let mut stmt = conn.prepare("SELECT id, title FROM papers")?;
                let rows = stmt.query_map([], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })?;
                for (id, title) in rows.flatten() {
                    if title_key(&title) == tk {
                        duplicates.push(DuplicateHit {
                            paper_id: id,
                            title,
                            reason: "title".into(),
                        });
                    }
                }
            }
        }
    }
    duplicates.dedup_by(|a, b| a.paper_id == b.paper_id && a.reason == b.reason);

    Ok(ImportPreview {
        temp_token: token,
        pdf: pdf_info,
        md: parsed,
        page_warnings,
        duplicates,
        pairing_hint,
    })
}

/// 按文件路径分析（Tauri 桌面模式：对话框只返回路径，由 Rust 直接读取，避免大文件走 IPC）。
pub fn analyze_import_paths(
    state: &CoreState,
    pdf_path: Option<&std::path::Path>,
    md_path: Option<&std::path::Path>,
    md_text: Option<String>,
) -> CoreResult<ImportPreview> {
    let pdf_bytes = match pdf_path {
        Some(p) => Some(fs::read(p)?),
        None => None,
    };
    let pdf_filename = pdf_path
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string());
    let md = match md_path {
        Some(p) => Some(fs::read_to_string(p)?),
        None => md_text,
    };
    analyze_import(state, pdf_bytes, pdf_filename, md)
}

/// 第二步：正式入库。支持新建、更新已有文献分析、先 PDF 后补 MD。
pub fn commit_import(state: &CoreState, req: &CommitRequest) -> CoreResult<CommitResult> {
    let mut warnings: Vec<String> = Vec::new();
    let temp_dir = req.temp_token.as_ref().map(|t| temp_import_dir(t));

    let has_temp_pdf = temp_dir
        .as_ref()
        .map(|d| d.join("pdf.bin").exists())
        .unwrap_or(false);
    // 新建：有暂存 PDF 即使用；更新：仅在显式要求替换 PDF 时使用
    let use_pdf = if req.update_paper_id.is_some() {
        has_temp_pdf && req.replace_pdf
    } else {
        has_temp_pdf
    };
    let pdf_bytes: Option<Vec<u8>> = if use_pdf {
        Some(fs::read(temp_dir.as_ref().unwrap().join("pdf.bin"))?)
    } else {
        None
    };
    let md_text: Option<String> = match (&req.md_text, &temp_dir) {
        (Some(t), _) => Some(t.clone()),
        (None, Some(d)) if d.join("analysis.md").exists() => {
            Some(fs::read_to_string(d.join("analysis.md"))?)
        }
        _ => None,
    };

    let title = req.title.trim().to_string();
    if req.update_paper_id.is_none() && title.is_empty() {
        return Err(CoreError::InvalidImport(
            "标题不能为空：请在导入预览中补充标题".into(),
        ));
    }

    let paper_id = match &req.update_paper_id {
        Some(id) => {
            {
                let inner = state.inner.lock().unwrap();
                if !paper_exists(&inner.conn, id)? {
                    return Err(err_paper(id));
                }
            }
            id.clone()
        }
        None => uuid::Uuid::new_v4().simple().to_string(),
    };

    // PDF 入库（复制到托管目录），并确定页数与摘要
    let mut pdf_fields: Option<(String, i64, String, Option<i64>)> = None; // (rel, size, sha, pages)
    if let Some(bytes) = &pdf_bytes {
        let dest = pdf_storage_path(&library_dir_of(state), &paper_id);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let replacing_existing = dest.exists();
        fs::write(&dest, bytes)?;
        let (page_count, page_err) = match count_pdf_pages(bytes) {
            Ok(n) => (Some(n), None),
            Err(e) => (None, Some(e)),
        };
        if let Some(e) = page_err {
            warnings.push(e);
        }
        pdf_fields = Some((
            format!("papers/{}/original.pdf", paper_id),
            bytes.len() as i64,
            sha256_hex(bytes),
            page_count,
        ));
        if req.update_paper_id.is_some() && replacing_existing {
            warnings.push(
                "已替换 PDF 文件：原有页码定位可能变化，核查状态已全部重置为待核查".into(),
            );
        }
    }

    let now = now_ts();
    let authors_json = serde_json::to_string(&req.authors)?;
    let doi = req.doi.trim().to_string();
    {
        let inner = state.inner.lock().unwrap();
        let conn = &inner.conn;
        match (&req.update_paper_id, &pdf_fields) {
            (None, Some((rel, size, sha, pages))) => {
                conn.execute(
                    "INSERT INTO papers (id, title, authors, year, doi, pdf_rel_path, pdf_size,
                            pdf_sha256, pdf_page_count, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
                    params![paper_id, title, authors_json, req.year, doi, rel, size, sha, pages, now],
                )?;
            }
            (None, None) => {
                conn.execute(
                    "INSERT INTO papers (id, title, authors, year, doi, pdf_rel_path, pdf_size,
                            pdf_sha256, pdf_page_count, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, '', 0, '', NULL, ?6, ?6)",
                    params![paper_id, title, authors_json, req.year, doi, now],
                )?;
                warnings.push("该文献未关联 PDF，仅保存了分析内容".into());
            }
            (Some(_), Some((rel, size, sha, pages))) => {
                conn.execute(
                    "UPDATE papers SET title = ?1, authors = ?2, year = ?3, doi = ?4,
                            pdf_rel_path = ?5, pdf_size = ?6, pdf_sha256 = ?7,
                            pdf_page_count = ?8, updated_at = ?9
                     WHERE id = ?10",
                    params![title, authors_json, req.year, doi, rel, size, sha, pages, now, paper_id],
                )?;
                // PDF 文件已更换，旧页码证据可能失效
                conn.execute(
                    "UPDATE evidence SET stale = 1, check_status = 'unverified' WHERE paper_id = ?1",
                    params![paper_id],
                )?;
            }
            (Some(_), None) => {
                conn.execute(
                    "UPDATE papers SET title = ?1, authors = ?2, year = ?3, doi = ?4, updated_at = ?5
                     WHERE id = ?6",
                    params![title, authors_json, req.year, doi, now, paper_id],
                )?;
            }
        }
    }

    if !req.tags.is_empty() {
        let inner = state.inner.lock().unwrap();
        set_tags(&inner.conn, &paper_id, &req.tags)?;
    }
    if let Some(project) = req.project.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let inner = state.inner.lock().unwrap();
        set_projects(&inner.conn, &paper_id, &[project.to_string()])?;
    }

    // 分析入库
    if let Some(text) = &md_text {
        set_analysis_locked(state, &paper_id, text)?;
    }

    // 清理暂存
    if let Some(d) = &temp_dir {
        let _ = fs::remove_dir_all(d);
    }

    let paper = crate::service::papers::get_paper(state, &paper_id)?;
    Ok(CommitResult { paper, warnings })
}

pub fn library_dir_of(state: &CoreState) -> PathBuf {
    state.inner.lock().unwrap().library_dir.clone()
}

/// 替换/新建文献分析。保留更新前恢复副本；刷新证据核查状态。
pub fn set_analysis(
    state: &CoreState,
    paper_id: &str,
    md_text: &str,
) -> CoreResult<AnalysisResponse> {
    set_analysis_locked(state, paper_id, md_text)
}

fn set_analysis_locked(
    state: &CoreState,
    paper_id: &str,
    md_text: &str,
) -> CoreResult<AnalysisResponse> {
    {
        let inner = state.inner.lock().unwrap();
        if !paper_exists(&inner.conn, paper_id)? {
            return Err(err_paper(paper_id));
        }
    }
    let parsed = parse_analysis(md_text);
    {
        let inner = state.inner.lock().unwrap();
        let conn = &inner.conn;
        let old: Option<(String, i64)> = conn
            .query_row(
                "SELECT md_content, updated_at FROM analyses WHERE paper_id = ?1",
                params![paper_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let now = now_ts();
        match old {
            Some((prev_md, _)) => {
                conn.execute(
                    "UPDATE analyses SET schema_version = ?1, md_content = ?2,
                            prev_md_content = ?3, parse_status = ?4, updated_at = ?5
                     WHERE paper_id = ?6",
                    params![
                        parsed.meta.schema_version,
                        md_text,
                        prev_md,
                        parsed.parse_status,
                        now,
                        paper_id
                    ],
                )?;
            }
            None => {
                conn.execute(
                    "INSERT INTO analyses (paper_id, schema_version, md_content, prev_md_content,
                            parse_status, imported_at, updated_at)
                     VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?5)",
                    params![paper_id, parsed.meta.schema_version, md_text, parsed.parse_status, now],
                )?;
            }
        }
        refresh_evidence_staleness(conn, paper_id, md_text)?;
        conn.execute(
            "UPDATE papers SET updated_at = ?1 WHERE id = ?2",
            params![now, paper_id],
        )?;
    }
    get_analysis(state, paper_id)?.ok_or_else(|| err_paper(paper_id))
}

/// 内容仍在新分析中的证据保留核查状态；无法对应的标记为待核查（stale）。
fn refresh_evidence_staleness(conn: &Connection, paper_id: &str, new_md: &str) -> CoreResult<()> {
    let norm = |s: &str| s.split_whitespace().collect::<String>();
    let new_norm = norm(new_md);
    let rows: Vec<(i64, String)> = {
        let mut stmt =
            conn.prepare("SELECT id, excerpt FROM evidence WHERE paper_id = ?1")?;
        let rows = stmt.query_map(params![paper_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.flatten().collect()
    };
    for (id, excerpt) in rows {
        let probe: String = norm(&excerpt).chars().take(40).collect();
        let still_present = !probe.is_empty() && new_norm.contains(&probe);
        if still_present {
            conn.execute(
                "UPDATE evidence SET stale = 0 WHERE id = ?1",
                params![id],
            )?;
        } else {
            conn.execute(
                "UPDATE evidence SET stale = 1, check_status = 'unverified' WHERE id = ?1",
                params![id],
            )?;
        }
    }
    Ok(())
}

pub fn get_analysis(state: &CoreState, paper_id: &str) -> CoreResult<Option<AnalysisResponse>> {
    let inner = state.inner.lock().unwrap();
    let row = inner
        .conn
        .query_row(
            "SELECT md_content, prev_md_content, parse_status, updated_at
             FROM analyses WHERE paper_id = ?1",
            params![paper_id],
            |r| {
                Ok(AnalysisResponse {
                    paper_id: paper_id.to_string(),
                    md_content: r.get(0)?,
                    prev_md_content: r.get(1)?,
                    parse_status: r.get(2)?,
                    updated_at: r.get(3)?,
                    parsed: parse_analysis(&r.get::<_, String>(0)?),
                })
            },
        )
        .optional()?;
    Ok(row)
}

/// 恢复上一次更新前的分析内容（重新导入场景的安全网）。
pub fn restore_prev_analysis(state: &CoreState, paper_id: &str) -> CoreResult<AnalysisResponse> {
    let prev: Option<String> = {
        let inner = state.inner.lock().unwrap();
        inner
            .conn
            .query_row(
                "SELECT prev_md_content FROM analyses WHERE paper_id = ?1",
                params![paper_id],
                |r| r.get(0),
            )
            .optional()?
            .flatten()
    };
    match prev {
        Some(text) if !text.is_empty() => set_analysis(state, paper_id, &text),
        _ => Err(CoreError::NotFound("没有可恢复的上一次分析副本".into())),
    }
}

pub fn list_evidence(state: &CoreState, paper_id: &str) -> CoreResult<Vec<EvidenceRecord>> {
    let inner = state.inner.lock().unwrap();
    let mut stmt = inner.conn.prepare(
        "SELECT id, paper_id, anchor_hash, section, excerpt, page, check_status, stale, updated_at
         FROM evidence WHERE paper_id = ?1 ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map(params![paper_id], |r| {
        Ok(EvidenceRecord {
            id: r.get(0)?,
            paper_id: r.get(1)?,
            anchor_hash: r.get(2)?,
            section: r.get(3)?,
            excerpt: r.get(4)?,
            page: r.get(5)?,
            check_status: r.get(6)?,
            stale: r.get::<_, i64>(7)? != 0,
            updated_at: r.get(8)?,
        })
    })?;
    Ok(rows.flatten().collect())
}

pub fn upsert_evidence(
    state: &CoreState,
    paper_id: &str,
    input: &EvidenceInput,
) -> CoreResult<EvidenceRecord> {
    let inner = state.inner.lock().unwrap();
    if !paper_exists(&inner.conn, paper_id)? {
        return Err(err_paper(paper_id));
    }
    inner.conn.execute(
        "INSERT INTO evidence (paper_id, anchor_hash, section, excerpt, page, check_status, stale, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7)
         ON CONFLICT(paper_id, anchor_hash) DO UPDATE SET
            section = excluded.section,
            excerpt = excluded.excerpt,
            page = excluded.page,
            check_status = excluded.check_status,
            stale = 0,
            updated_at = excluded.updated_at",
        params![
            paper_id,
            input.anchor_hash,
            input.section,
            input.excerpt,
            input.page,
            input.check_status,
            now_ts()
        ],
    )?;
    let rec = inner
        .conn
        .query_row(
            "SELECT id, paper_id, anchor_hash, section, excerpt, page, check_status, stale, updated_at
             FROM evidence WHERE paper_id = ?1 AND anchor_hash = ?2",
            params![paper_id, input.anchor_hash],
            |r| {
                Ok(EvidenceRecord {
                    id: r.get(0)?,
                    paper_id: r.get(1)?,
                    anchor_hash: r.get(2)?,
                    section: r.get(3)?,
                    excerpt: r.get(4)?,
                    page: r.get(5)?,
                    check_status: r.get(6)?,
                    stale: r.get::<_, i64>(7)? != 0,
                    updated_at: r.get(8)?,
                })
            },
        )?;
    Ok(rec)
}

pub fn get_pdf_bytes(state: &CoreState, paper_id: &str) -> CoreResult<Vec<u8>> {
    let inner = state.inner.lock().unwrap();
    let rel: String = inner
        .conn
        .query_row(
            "SELECT pdf_rel_path FROM papers WHERE id = ?1",
            params![paper_id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| err_paper(paper_id))?;
    if rel.is_empty() {
        return Err(CoreError::NotFound("该文献没有关联的 PDF".into()));
    }
    let path = inner.library_dir.join(&rel);
    drop(inner);
    Ok(fs::read(path)?)
}
