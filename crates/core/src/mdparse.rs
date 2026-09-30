//! 解析按约定模板生成的文献分析 Markdown：
//! YAML 前置元数据（受限子集）+ 固定二级标题栏目 + `#pdf-page-N` 页码链接。
//! 解析器只做确定性规则匹配，不调用任何 AI。

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::OnceLock;

pub const FIXED_SECTIONS: [&str; 8] = [
    "一句话总结",
    "研究问题",
    "研究方法",
    "样本与数据",
    "主要发现",
    "创新点",
    "局限性",
    "阅读重点",
];

pub const SCHEMA_VERSION_CURRENT: i64 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct AnalysisMeta {
    pub schema_version: Option<i64>,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i64>,
    pub doi: String,
    pub source_pdf: String,
    pub tags: Vec<String>,
    /// 模板外的未知字段，原样保留（字符串形式）。
    pub extra: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SectionInfo {
    pub title: String,
    /// 栏目内 Markdown 正文（已去除首尾空白）。
    pub content: String,
    pub fixed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PageLink {
    pub page: u32,
    /// 链接附近最多 60 字符上下文，用于导入预览与证据定位。
    pub context: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ParsedAnalysis {
    pub meta: AnalysisMeta,
    pub has_front_matter: bool,
    /// 按文档顺序保留全部栏目（含未知栏目）。
    pub sections: Vec<SectionInfo>,
    /// 首个二级栏目之前的松散正文（含一级标题行），保证正文不丢失。
    pub preamble: String,
    pub missing_fixed: Vec<String>,
    pub duplicate_fixed: Vec<String>,
    pub warnings: Vec<String>,
    pub page_links: Vec<PageLink>,
    /// ok = 前置元数据有效且 8 个固定栏目齐全
    /// partial = 结构可识别但有缺失/告警
    /// unparsed = 无法识别固定结构
    pub parse_status: String,
}

impl ParsedAnalysis {
    pub fn section(&self, title: &str) -> Option<&SectionInfo> {
        self.sections.iter().find(|s| s.title == title)
    }

    /// 分析状态映射（需求 P0：未导入 / 格式待整理 / 可对比）。
    pub fn analysis_status(&self) -> &'static str {
        match self.parse_status.as_str() {
            "ok" | "partial" => "comparable",
            _ => "needs_formatting",
        }
    }
}

pub fn parse_analysis(text: &str) -> ParsedAnalysis {
    let mut warnings: Vec<String> = Vec::new();
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);

    let (fm_text, body, has_front_matter) = split_front_matter(text, &mut warnings);
    let meta = if has_front_matter {
        parse_front_matter(fm_text, &mut warnings)
    } else {
        warnings.push("未找到 YAML 前置元数据（文件应以 --- 开始）".into());
        AnalysisMeta::default()
    };

    if let Some(v) = meta.schema_version {
        if v != SCHEMA_VERSION_CURRENT {
            warnings.push(format!("未知模板版本 schema_version={}（当前支持 1）", v));
        }
    } else if has_front_matter {
        warnings.push("前置元数据缺少 schema_version".into());
    }

    let (sections, preamble) = split_sections(body);
    let mut fixed_seen: Vec<&'static str> = Vec::new();
    let mut duplicate_fixed: Vec<String> = Vec::new();
    let mut kept: Vec<SectionInfo> = Vec::new();
    for sec in sections {
        if let Some(name) = FIXED_SECTIONS.iter().find(|n| **n == sec.title) {
            if fixed_seen.contains(&name) {
                duplicate_fixed.push(name.to_string());
                warnings.push(format!("栏目「{}」出现多次，保留第一处内容", name));
                // 重复内容仍并入保留栏，避免正文丢失
                if let Some(first) = kept.iter_mut().find(|k| k.title == sec.title) {
                    first.content.push_str("\n\n");
                    first.content.push_str(&sec.content);
                }
                continue;
            }
            fixed_seen.push(name);
            kept.push(sec);
        } else {
            warnings.push(format!("未知栏目「{}」，已保留在正文中，不参与默认对比", sec.title));
            kept.push(sec);
        }
    }
    let missing_fixed: Vec<String> = FIXED_SECTIONS
        .iter()
        .filter(|n| !fixed_seen.contains(n))
        .map(|n| n.to_string())
        .collect();
    for m in &missing_fixed {
        warnings.push(format!("缺少固定栏目「{}」，导入后显示待补充", m));
    }

    let page_links = extract_page_links(body);

    let parse_status = if !has_front_matter && fixed_seen.is_empty() {
        "unparsed".to_string()
    } else if warnings.is_empty() && missing_fixed.is_empty() {
        "ok".to_string()
    } else {
        "partial".to_string()
    };

    ParsedAnalysis {
        meta,
        has_front_matter,
        sections: kept,
        preamble,
        missing_fixed,
        duplicate_fixed,
        warnings,
        page_links,
        parse_status,
    }
}

/// 分离 `---` 包裹的前置元数据。返回 (front_matter, body, 是否存在)。
fn split_front_matter<'a>(text: &'a str, warnings: &mut Vec<String>) -> (&'a str, &'a str, bool) {
    let mut lines = text.split_inclusive('\n');
    let first = lines.next().unwrap_or("");
    let first_trim = first.trim_end_matches(['\r', '\n']);
    if first_trim != "---" {
        return ("", text, false);
    }
    let rest_start = first.len();
    let rest = &text[rest_start..];
    // 寻找闭合的 --- 行
    let mut offset = 0usize;
    for line in rest.split_inclusive('\n') {
        let t = line.trim_end_matches(['\r', '\n']);
        if t == "---" || t == "..." {
            let fm = &rest[..offset];
            let body = &rest[offset + line.len()..];
            return (fm, body, true);
        }
        offset += line.len();
    }
    warnings.push("YAML 前置元数据未闭合（缺少结束的 ---）".into());
    // 未闭合时按无前置元数据处理，正文不丢弃
    ("", text, false)
}

/// 受限 YAML 子集解析：
/// - `key: value`，value ∈ null | 整数 | "双引号串" | '单引号串' | 行内数组 | 纯文本
/// - `key:` 后跟若干 `- item` 行（字符串数组）
/// - 未知键保留进 extra
pub fn parse_front_matter(fm: &str, warnings: &mut Vec<String>) -> AnalysisMeta {
    let mut meta = AnalysisMeta::default();
    let lines: Vec<&str> = fm.lines().collect();
    let mut i = 0usize;
    while i < lines.len() {
        let raw = lines[i];
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            i += 1;
            continue;
        }
        let colon = match line.find(':') {
            Some(p) => p,
            None => {
                warnings.push(format!("YAML 第 {} 行无法解析（缺少冒号）：{}", i + 1, line));
                i += 1;
                continue;
            }
        };
        let key = line[..colon].trim().to_string();
        let value = line[colon + 1..].trim().to_string();
        if value.is_empty() {
            // 可能是多行数组
            let mut items: Vec<String> = Vec::new();
            let mut j = i + 1;
            while j < lines.len() {
                let l = lines[j].trim();
                if let Some(item) = l.strip_prefix("- ") {
                    items.push(parse_scalar(item, i + 1, warnings).to_string());
                    j += 1;
                } else if l == "-" {
                    items.push(String::new());
                    j += 1;
                } else {
                    break;
                }
            }
            assign(&mut meta, &key, YamlValue::List(items), i + 1, warnings);
            i = j;
        } else {
            let v = parse_value(&value, i + 1, warnings);
            assign(&mut meta, &key, v, i + 1, warnings);
            i += 1;
        }
    }
    meta
}

enum YamlValue {
    Null,
    Int(i64),
    Str(String),
    List(Vec<String>),
}

fn parse_scalar(s: &str, lineno: usize, warnings: &mut Vec<String>) -> String {
    let t = s.trim();
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        return unescape_double(&t[1..t.len() - 1]);
    }
    if t.len() >= 2 && t.starts_with('\'') && t.ends_with('\'') {
        return t[1..t.len() - 1].replace("''", "'");
    }
    if t.is_empty() || t == "null" || t == "~" {
        return String::new();
    }
    if t.starts_with('"') || t.starts_with('\'') {
        warnings.push(format!("YAML 第 {} 行引号未闭合：{}", lineno, t));
    }
    t.to_string()
}

fn unescape_double(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn parse_value(v: &str, lineno: usize, warnings: &mut Vec<String>) -> YamlValue {
    let t = v.trim();
    if t.is_empty() || t == "null" || t == "~" {
        return YamlValue::Null;
    }
    if t.starts_with('[') {
        if !t.ends_with(']') {
            warnings.push(format!("YAML 第 {} 行数组格式不完整：{}", lineno, t));
            return YamlValue::Str(parse_scalar(t, lineno, warnings));
        }
        let inner = &t[1..t.len() - 1];
        let items = split_inline_list(inner)
            .into_iter()
            .map(|item| parse_scalar(&item, lineno, warnings))
            .collect();
        return YamlValue::List(items);
    }
    if let Ok(n) = t.parse::<i64>() {
        return YamlValue::Int(n);
    }
    YamlValue::Str(parse_scalar(t, lineno, warnings))
}

/// 按顶层逗号切分行内数组，尊重引号。
fn split_inline_list(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for c in s.chars() {
        match quote {
            Some(q) => {
                cur.push(c);
                if c == q {
                    quote = None;
                }
            }
            None => {
                if c == '"' || c == '\'' {
                    quote = Some(c);
                    cur.push(c);
                } else if c == ',' {
                    out.push(cur.trim().to_string());
                    cur.clear();
                } else {
                    cur.push(c);
                }
            }
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

fn assign(meta: &mut AnalysisMeta, key: &str, v: YamlValue, lineno: usize, warnings: &mut Vec<String>) {
    let as_str = |v: &YamlValue| -> String {
        match v {
            YamlValue::Str(s) => s.clone(),
            YamlValue::Int(n) => n.to_string(),
            YamlValue::List(l) => l.join(", "),
            YamlValue::Null => String::new(),
        }
    };
    match key {
        "schema_version" => match v {
            YamlValue::Int(n) => meta.schema_version = Some(n),
            YamlValue::Str(s) if !s.is_empty() => {
                match s.parse::<i64>() {
                    Ok(n) => meta.schema_version = Some(n),
                    Err(_) => warnings.push(format!("schema_version 应为整数，第 {} 行得到「{}」", lineno, s)),
                }
            }
            _ => {}
        },
        "title" => meta.title = as_str(&v),
        "authors" => meta.authors = match v {
            YamlValue::List(l) => l,
            YamlValue::Str(s) if !s.is_empty() => vec![s],
            _ => vec![],
        },
        "year" => meta.year = match v {
            YamlValue::Int(n) => Some(n),
            YamlValue::Str(s) => s.parse::<i64>().ok(),
            _ => None,
        },
        "doi" => meta.doi = as_str(&v),
        "source_pdf" => meta.source_pdf = as_str(&v),
        "tags" => meta.tags = match v {
            YamlValue::List(l) => l,
            YamlValue::Str(s) if !s.is_empty() => vec![s],
            _ => vec![],
        },
        other => {
            meta.extra.insert(other.to_string(), as_str(&v));
        }
    }
}

/// 按二级标题（##）切分栏目；一级标题（#）是文档标题，不构成栏目；
/// 更深层级（### 及以下）留在所属栏目内。首个栏目前的散落正文进入 preamble，保证不丢失。
fn split_sections(body: &str) -> (Vec<SectionInfo>, String) {
    let mut out: Vec<SectionInfo> = Vec::new();
    let mut current: Option<SectionInfo> = None;
    let mut preamble = String::new();
    for line in body.lines() {
        let t = line.trim_end();
        let level = if t.starts_with("####") {
            4
        } else if t.starts_with("###") {
            3
        } else if t.starts_with("##") {
            2
        } else if t.starts_with('#') {
            1
        } else {
            0
        };
        match level {
            1 => {
                if let Some(sec) = current.take() {
                    out.push(finish_section(sec));
                }
                preamble.push_str(line);
                preamble.push('\n');
            }
            2 => {
                if let Some(sec) = current.take() {
                    out.push(finish_section(sec));
                }
                let title = t.trim_start_matches('#').trim().to_string();
                let fixed = FIXED_SECTIONS.contains(&title.as_str());
                current = Some(SectionInfo {
                    title,
                    content: String::new(),
                    fixed,
                });
            }
            _ => {
                if let Some(sec) = current.as_mut() {
                    sec.content.push_str(line);
                    sec.content.push('\n');
                } else {
                    preamble.push_str(line);
                    preamble.push('\n');
                }
            }
        }
    }
    if let Some(sec) = current.take() {
        out.push(finish_section(sec));
    }
    (out, preamble.trim().to_string())
}

fn finish_section(mut sec: SectionInfo) -> SectionInfo {
    sec.content = sec.content.trim().to_string();
    sec
}

fn page_link_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[[^\]]*\]\(#pdf-page-(\d+)\)").expect("页码链接正则合法"))
}

pub fn extract_page_links(body: &str) -> Vec<PageLink> {
    let re = page_link_regex();
    let mut out = Vec::new();
    for caps in re.captures_iter(body) {
        let page: u32 = caps[1].parse().unwrap_or(0);
        let m = caps.get(0).unwrap();
        let start = m.start().saturating_sub(30);
        let end = (m.end() + 30).min(body.len());
        // 保证不切在字符边界中间
        let start = floor_char_boundary(body, start);
        let end = ceil_char_boundary(body, end);
        let context: String = body[start..end].split_whitespace().collect::<Vec<_>>().join(" ");
        out.push(PageLink { page, context });
    }
    out
}

fn floor_char_boundary(s: &str, i: usize) -> usize {
    let mut i = i;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn ceil_char_boundary(s: &str, i: usize) -> usize {
    let mut i = i.min(s.len());
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"---
schema_version: 1
title: "Sample Paper: A Study"
authors: ["Alice", "Bob"]
year: 2024
doi: "10.1000/sample"
source_pdf: "sample.pdf"
tags: [机器学习, 对比]
---

# Sample Paper

## 一句话总结

研究了对 X 的处理。

## 研究问题

如何提升 Y？参见 [PDF 第 8 页](#pdf-page-8)。

## 研究方法

- 实验
- 统计

## 样本与数据

N=100

## 主要发现

发现一 [PDF 第 5 页](#pdf-page-5)。

### 子发现

细节。

## 创新点

新方法。

## 局限性

样本有限。

## 阅读重点

关注第 3 页图表。

## 自定义栏目

额外内容。
"#;

    #[test]
    fn parses_full_template() {
        let p = parse_analysis(SAMPLE);
        assert_eq!(p.parse_status, "partial"); // 有未知栏目与告警
        assert_eq!(p.meta.title, "Sample Paper: A Study");
        assert_eq!(p.meta.authors, vec!["Alice", "Bob"]);
        assert_eq!(p.meta.year, Some(2024));
        assert_eq!(p.meta.tags, vec!["机器学习", "对比"]);
        assert!(p.missing_fixed.is_empty());
        assert!(p.duplicate_fixed.is_empty());
        assert!(p.sections.iter().any(|s| s.title == "自定义栏目"));
        let findings = p.section("主要发现").unwrap();
        assert!(findings.content.contains("子发现"), "三级标题应留在所属栏目内");
        assert_eq!(p.page_links.len(), 2);
        assert!(p.page_links.iter().any(|l| l.page == 8));
    }

    #[test]
    fn missing_sections_reported() {
        let text = "---\nschema_version: 1\ntitle: \"T\"\n---\n\n## 研究问题\n\n内容\n";
        let p = parse_analysis(text);
        assert_eq!(p.parse_status, "partial");
        assert!(p.missing_fixed.contains(&"局限性".to_string()));
    }

    #[test]
    fn duplicate_section_kept_once() {
        let text = "---\nschema_version: 1\ntitle: \"T\"\n---\n\n## 研究问题\nA\n\n## 研究问题\nB\n";
        let p = parse_analysis(text);
        assert_eq!(p.duplicate_fixed, vec!["研究问题"]);
        let count = p.sections.iter().filter(|s| s.title == "研究问题").count();
        assert_eq!(count, 1);
        let sec = p.section("研究问题").unwrap();
        assert!(sec.content.contains('A') && sec.content.contains('B'));
    }

    #[test]
    fn no_structure_is_unparsed() {
        let p = parse_analysis("随便一段自由文本，没有前置元数据，也没有固定栏目。");
        assert_eq!(p.parse_status, "unparsed");
    }

    #[test]
    fn unclosed_front_matter_warns() {
        let p = parse_analysis("---\ntitle: \"T\"\n\n正文内容");
        assert!(!p.has_front_matter);
        assert!(p.warnings.iter().any(|w| w.contains("未闭合")));
    }

    #[test]
    fn multiline_array_supported() {
        let text = "---\nschema_version: 1\nauthors:\n  - \"A\"\n  - \"B\"\n---\n\n## 一句话总结\nx\n";
        let p = parse_analysis(text);
        assert_eq!(p.meta.authors, vec!["A", "B"]);
    }

    #[test]
    fn unknown_schema_version_warns() {
        let text = "---\nschema_version: 2\ntitle: \"T\"\n---\n\n## 一句话总结\nx\n";
        let p = parse_analysis(text);
        assert!(p.warnings.iter().any(|w| w.contains("未知模板版本")));
    }

    #[test]
    fn h1_title_is_not_section() {
        let text = "---\nschema_version: 1\ntitle: \"T\"\n---\n\n# My Long Title\n\n散落前言。\n\n## 研究问题\n内容\n";
        let p = parse_analysis(text);
        assert!(!p.sections.iter().any(|s| s.title == "My Long Title"));
        assert!(p.preamble.contains("My Long Title"));
        assert!(p.preamble.contains("散落前言"));
        assert!(
            !p.warnings.iter().any(|w| w.contains("My Long Title")),
            "一级标题不应触发未知栏目告警"
        );
    }

    #[test]
    fn year_null_ok() {
        let text = "---\nschema_version: 1\ntitle: \"T\"\nyear: null\n---\n\n## 一句话总结\nx\n";
        let p = parse_analysis(text);
        assert_eq!(p.meta.year, None);
    }
}
