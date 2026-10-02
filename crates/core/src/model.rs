//! API 层数据结构。axum 与 Tauri 两个外壳共用同一套类型。

use crate::mdparse::ParsedAnalysis;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ListQuery {
    /// 关键词：匹配标题、作者、DOI、标签、当前分析、个人笔记。
    pub query: String,
    pub project: String,
    pub tag: String,
    /// to_read | reading | finished
    pub status: String,
    /// comparable | needs_formatting | none
    pub analysis: String,
    pub year: Option<i64>,
    /// true 时仅列出回收站内的文献
    pub trash: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PaperListItem {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i64>,
    pub doi: String,
    pub reading_status: String,
    /// none | comparable | needs_formatting
    pub analysis_status: String,
    pub analysis_updated_at: Option<i64>,
    pub has_notes: bool,
    pub tags: Vec<String>,
    pub projects: Vec<String>,
    pub pdf_page_count: Option<i64>,
    pub trashed: bool,
    /// 搜索时记录命中来源：title/authors/doi/tag/analysis/notes
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub matched_on: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PaperDetail {
    #[serde(flatten)]
    pub item: PaperListItem,
    pub pdf_size: i64,
    pub pdf_sha256: String,
    pub last_page: i64,
    pub last_zoom: f64,
    pub last_mode: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct PatchPaper {
    pub title: Option<String>,
    pub authors: Option<Vec<String>>,
    pub year: Option<i64>,
    pub doi: Option<String>,
    pub reading_status: Option<String>,
    pub tags: Option<Vec<String>>,
    /// 项目名列表，整体替换
    pub projects: Option<Vec<String>>,
    pub last_page: Option<i64>,
    pub last_zoom: Option<f64>,
    pub last_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DuplicateHit {
    pub paper_id: String,
    pub title: String,
    /// sha256 | doi | title
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportPdfInfo {
    pub sha256: String,
    pub size: u64,
    pub page_count: Option<i64>,
    pub page_count_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct SuggestedMetadata {
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i64>,
    pub doi: String,
    /// PDF 文档信息、PDF 首页或文件名；供预览页说明自动填充依据。
    pub title_source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportPreview {
    pub temp_token: String,
    pub pdf: Option<ImportPdfInfo>,
    pub md: Option<ParsedAnalysis>,
    pub suggested_metadata: Option<SuggestedMetadata>,
    /// 结合 PDF 页数校验后的页码告警（越界等）
    pub page_warnings: Vec<String>,
    pub duplicates: Vec<DuplicateHit>,
    /// PDF 文件名与 MD source_pdf 的配对提示
    pub pairing_hint: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct CommitRequest {
    pub temp_token: Option<String>,
    /// 直接提交粘贴的 Markdown（无临时文件时使用）
    pub md_text: Option<String>,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i64>,
    pub doi: String,
    pub tags: Vec<String>,
    pub project: Option<String>,
    /// 提供时表示更新已有文献的分析（重新导入场景）
    pub update_paper_id: Option<String>,
    /// 更新时是否同时替换 PDF 文件（会提示重新核查）
    pub replace_pdf: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommitResult {
    pub paper: PaperDetail,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalysisResponse {
    pub paper_id: String,
    pub md_content: String,
    pub prev_md_content: Option<String>,
    pub parsed: ParsedAnalysis,
    pub parse_status: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteResponse {
    pub paper_id: String,
    pub content: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRecord {
    pub id: i64,
    pub paper_id: String,
    pub anchor_hash: String,
    pub section: String,
    pub excerpt: String,
    pub page: Option<i64>,
    /// unverified | verified
    pub check_status: String,
    pub stale: bool,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct EvidenceInput {
    pub anchor_hash: String,
    pub section: String,
    pub excerpt: String,
    pub page: Option<i64>,
    pub check_status: String,
}

impl Default for EvidenceInput {
    fn default() -> Self {
        Self {
            anchor_hash: String::new(),
            section: String::new(),
            excerpt: String::new(),
            page: None,
            check_status: "unverified".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnnotationRect {
    /// 页面归一化坐标（0-1，左上角原点，页宽高为 1），与缩放无关
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PdfAnnotation {
    pub id: i64,
    pub paper_id: String,
    pub page: i64,
    /// highlight（选中文字）| rect（矩形框选）| note（便签）
    pub kind: String,
    pub rects: Vec<AnnotationRect>,
    /// yellow | red | blue | green
    pub color: String,
    pub text: String,
    pub quote: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct AnnotationInput {
    pub page: i64,
    pub kind: String,
    pub rects: Vec<AnnotationRect>,
    pub color: Option<String>,
    pub text: Option<String>,
    pub quote: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct AnnotationPatch {
    pub color: Option<String>,
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct Synthesis {
    pub agreement: String,
    pub differences: String,
    pub gap: String,
    pub conclusion: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareRecord {
    pub id: String,
    pub name: String,
    pub paper_ids: Vec<String>,
    pub dimensions: Vec<String>,
    pub synthesis: Synthesis,
    /// {paper_id: 上次查看时该文献分析的 updated_at}
    pub snapshots: std::collections::BTreeMap<String, i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct CompareInput {
    pub name: String,
    pub paper_ids: Vec<String>,
    pub dimensions: Vec<String>,
    pub synthesis: Synthesis,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompareWithStatus {
    #[serde(flatten)]
    pub record: CompareRecord,
    /// 打开对比时，分析已发生更新的文献 ID
    pub updated_papers: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectInfo {
    pub id: i64,
    pub name: String,
    pub paper_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TagInfo {
    pub id: i64,
    pub name: String,
    pub paper_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportResult {
    pub filename: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupResult {
    pub zip_path: String,
    pub papers: i64,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RestoreResult {
    pub library_dir: String,
    pub papers: i64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsResponse {
    pub library_dir: String,
}

pub const READING_STATUSES: [&str; 3] = ["to_read", "reading", "finished"];
