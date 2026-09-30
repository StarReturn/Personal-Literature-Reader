//! Markdown 导出：单篇（分析 + 笔记）与对比综述。
//! 导出为静态快照；内部页码链接转为纯文本页码（不承诺外部打开 PDF）。

use crate::db::CoreState;
use crate::error::CoreResult;
use crate::mdparse::parse_analysis;
use crate::model::{CompareRecord, ExportResult, PaperListItem};
use crate::service::papers::get_paper;
use regex::Regex;
use std::sync::OnceLock;

fn page_link_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[([^\]]*)\]\(#pdf-page-(\d+)\)").expect("页码链接正则合法"))
}

/// 把 `#pdf-page-N` 链接替换为纯文本（保留页码信息，便于人工追溯）。
pub fn inline_page_links(text: &str) -> String {
    page_link_regex()
        .replace_all(text, "$1")
        .replace("[PDF 第", "PDF 第")
        .replace("页]", "页")
}

fn fmt_ts(ts: i64) -> String {
    // 本地时间简单格式化（导出用途，无需时区库）
    let secs = ts.max(0);
    let days = secs / 86400;
    let rem = secs % 86400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // 1970-01-01 起的天数转日期（民用算法，够用）
    let (y, mo, d) = civil_from_days(days);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02}:{s:02}")
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn export_paper(
    state: &CoreState,
    paper_id: &str,
    include_notes: bool,
) -> CoreResult<ExportResult> {
    let detail = get_paper(state, paper_id)?;
    let item: &PaperListItem = &detail.item;
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", item.title));
    if !item.authors.is_empty() {
        out.push_str(&format!("**作者**：{}\n\n", item.authors.join(", ")));
    }
    if let Some(y) = item.year {
        out.push_str(&format!("**年份**：{y}\n\n"));
    }
    if !item.doi.is_empty() {
        out.push_str(&format!("**DOI**：{}\n\n", item.doi));
    }
    if !item.tags.is_empty() {
        out.push_str(&format!("**标签**：{}\n\n", item.tags.join("、")));
    }
    out.push_str(&format!("**导出时间**：{}\n\n---\n\n", fmt_ts(crate::db::now_ts())));

    let analysis = crate::service::import::get_analysis(state, paper_id)?;
    if let Some(a) = analysis {
        out.push_str(&format!(
            "## AI 分析（更新于 {}）\n\n",
            fmt_ts(a.updated_at)
        ));
        // 去掉前置元数据块，仅导出正文；页码链接转纯文本
        let parsed = parse_analysis(&a.md_content);
        let body = strip_front_matter(&a.md_content, parsed.has_front_matter);
        out.push_str(&inline_page_links(&body));
        out.push_str("\n\n");
        if let Some(prev) = &a.prev_md_content {
            if !prev.is_empty() {
                out.push_str(&format!(
                    "> 注：本篇存在更新前的分析副本（{} 字符），可在应用内恢复。\n\n",
                    prev.chars().count()
                ));
            }
        }
    } else {
        out.push_str("## AI 分析\n\n（未导入）\n\n");
    }

    if include_notes {
        let note = crate::service::papers::get_note(state, paper_id)?;
        out.push_str("## 个人笔记\n\n");
        if note.content.trim().is_empty() {
            out.push_str("（空）\n");
        } else {
            out.push_str(&note.content);
            out.push('\n');
        }
    }

    let filename = format!("{}.md", sanitize_filename(&item.title));
    Ok(ExportResult {
        filename,
        content: out,
    })
}

fn strip_front_matter(md: &str, has_fm: bool) -> String {
    if !has_fm {
        return md.to_string();
    }
    let mut lines = md.split_inclusive('\n');
    let first = lines.next().unwrap_or("");
    let rest = &md[first.len()..];
    for line in rest.split_inclusive('\n') {
        let t = line.trim_end_matches(['\r', '\n']);
        if t == "---" || t == "..." {
            return rest[line.len()..].to_string();
        }
    }
    md.to_string()
}

pub fn export_compare(state: &CoreState, id: &str) -> CoreResult<ExportResult> {
    let record: CompareRecord = crate::service::compares::get_compare_record(state, id)?;
    let mut out = String::new();
    out.push_str(&format!("# 对比综述：{}\n\n", record.name));
    out.push_str(&format!("**导出时间**：{}\n\n", fmt_ts(crate::db::now_ts())));

    // 收录文献（标题、DOI 可追溯）
    out.push_str("## 收录文献\n\n");
    let mut titles: Vec<(String, String)> = Vec::new();
    for pid in &record.paper_ids {
        match get_paper(state, pid) {
            Ok(p) => {
                titles.push((p.item.title.clone(), pid.clone()));
                out.push_str(&format!("- **{}**", p.item.title));
                if let Some(y) = p.item.year {
                    out.push_str(&format!("（{y}）"));
                }
                if !p.item.doi.is_empty() {
                    out.push_str(&format!(" DOI: {}", p.item.doi));
                }
                out.push('\n');
            }
            Err(e) => {
                out.push_str(&format!("- （文献不可用：{e}）\n"));
                titles.push(("未知文献".into(), pid.clone()));
            }
        }
    }
    out.push('\n');

    // 按维度分节导出（长内容比表格单元更适合阅读）
    out.push_str("## 对比内容\n\n");
    for dim in &record.dimensions {
        out.push_str(&format!("### {dim}\n\n"));
        for (i, pid) in record.paper_ids.iter().enumerate() {
            let title = titles
                .get(i)
                .map(|(t, _)| t.clone())
                .unwrap_or_else(|| "未知文献".into());
            out.push_str(&format!("#### {}. {}\n\n", i + 1, title));
            let analysis = crate::service::import::get_analysis(state, pid)?;
            match analysis {
                Some(a) => {
                    let parsed = parse_analysis(&a.md_content);
                    match parsed.section(dim) {
                        Some(sec) if !sec.content.trim().is_empty() => {
                            out.push_str(&inline_page_links(&sec.content));
                            out.push_str("\n\n");
                        }
                        _ => {
                            out.push_str("（待补充）\n\n");
                        }
                    }
                }
                None => out.push_str("（未导入分析）\n\n"),
            }
        }
    }

    // 个人综合结论
    let s = &record.synthesis;
    out.push_str("## 我的综合结论\n\n");
    let sections = [
        ("共识", &s.agreement),
        ("分歧", &s.differences),
        ("研究空白", &s.gap),
        ("个人结论", &s.conclusion),
    ];
    for (name, content) in sections {
        out.push_str(&format!("**{name}**\n\n"));
        if content.trim().is_empty() {
            out.push_str("（未填写）\n\n");
        } else {
            out.push_str(content.trim());
            out.push_str("\n\n");
        }
    }

    let filename = format!("{}.md", sanitize_filename(&record.name));
    Ok(ExportResult {
        filename,
        content: out,
    })
}

/// 批量导出多篇文献为一份合并 Markdown（含目录、每篇分析+笔记，篇间分隔）。
/// ids 为空时导出全部文献。
pub fn export_papers_batch(state: &CoreState, ids: &[String]) -> CoreResult<ExportResult> {
    let mut papers = Vec::new();
    if ids.is_empty() {
        let all = crate::service::papers::list_papers(
            state,
            &crate::model::ListQuery::default(),
        )?;
        if all.is_empty() {
            return Err(crate::error::CoreError::Msg("资料库中没有可导出的文献".into()));
        }
        for item in all {
            papers.push(crate::service::papers::get_paper(state, &item.id)?);
        }
    } else {
        for id in ids {
            papers.push(crate::service::papers::get_paper(state, id)?);
        }
    }

    let mut out = String::new();
    out.push_str("# 批量导出文献\n\n");
    out.push_str(&format!(
        "**导出时间**：{}　**共 {} 篇**\n\n",
        fmt_ts(crate::db::now_ts()),
        papers.len()
    ));
    out.push_str("## 目录\n\n");
    for (i, p) in papers.iter().enumerate() {
        out.push_str(&format!("{}. {}\n", i + 1, p.item.title));
    }
    out.push_str("\n---\n\n");

    for (i, p) in papers.iter().enumerate() {
        let single = export_paper(state, &p.item.id, true)?;
        out.push_str(&single.content);
        if i + 1 < papers.len() {
            out.push_str("\n\n---\n\n");
        }
    }

    let (y, mo, d) = civil_from_days(crate::db::now_ts() / 86400);
    let filename = format!("文献批量导出-{y:04}{mo:02}{d:02}.md");
    Ok(ExportResult {
        filename,
        content: out,
    })
}

/// 纯笔记汇总导出：每篇一节（题录 + 个人笔记），不含 AI 分析——
/// 适合作为外部 AI（如生成 PPT 大纲）的干净输入。ids 为空时导出全部有笔记的文献。
pub fn export_notes_batch(state: &CoreState, ids: &[String]) -> CoreResult<ExportResult> {
    let target_ids: Vec<String> = if ids.is_empty() {
        crate::service::papers::list_papers(state, &crate::model::ListQuery::default())?
            .into_iter()
            .map(|p| p.id)
            .collect()
    } else {
        ids.to_vec()
    };

    struct Entry {
        title: String,
        meta: String,
        note: String,
    }
    let mut entries: Vec<Entry> = Vec::new();
    for id in &target_ids {
        let paper = crate::service::papers::get_paper(state, id)?;
        let note = crate::service::papers::get_note(state, id)?;
        if note.content.trim().is_empty() {
            continue;
        }
        let mut meta = String::new();
        if !paper.item.authors.is_empty() {
            meta.push_str(&paper.item.authors.join(", "));
        }
        if let Some(y) = paper.item.year {
            if !meta.is_empty() {
                meta.push_str(" · ");
            }
            meta.push_str(&y.to_string());
        }
        if !paper.item.doi.is_empty() {
            if !meta.is_empty() {
                meta.push_str(" · ");
            }
            meta.push_str(&format!("DOI: {}", paper.item.doi));
        }
        entries.push(Entry {
            title: paper.item.title.clone(),
            meta,
            note: note.content,
        });
    }

    if entries.is_empty() {
        return Err(crate::error::CoreError::Msg(
            "所选文献都没有个人笔记：先在阅读页写下笔记再导出".into(),
        ));
    }

    let mut out = String::new();
    out.push_str("# 文献笔记汇总

");
    out.push_str(&format!(
        "**导出时间**：{}　**共 {} 篇文献的个人笔记**

",
        fmt_ts(crate::db::now_ts()),
        entries.len()
    ));
    out.push_str("> 用途提示：以下按文献整理的个人笔记（含题录信息，不含 AI 分析原文），可直接交给 AI 汇总生成 PPT 大纲、综述草稿等。

---

");
    for (i, e) in entries.iter().enumerate() {
        out.push_str(&format!("## {}. {}

", i + 1, e.title));
        if !e.meta.is_empty() {
            out.push_str(&format!("**{}**

", e.meta));
        }
        out.push_str("### 个人笔记

");
        out.push_str(e.note.trim());
        out.push_str("

");
        if i + 1 < entries.len() {
            out.push_str("---

");
        }
    }

    let (y, mo, d) = civil_from_days(crate::db::now_ts() / 86400);
    let filename = format!("文献笔记汇总-{y:04}{mo:02}{d:02}.md");
    Ok(ExportResult {
        filename,
        content: out,
    })
}

fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches('.');
    if trimmed.is_empty() {
        "export".to_string()
    } else {
        trimmed.chars().take(80).collect()
    }
}
