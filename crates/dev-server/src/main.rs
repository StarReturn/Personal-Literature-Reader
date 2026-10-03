//! 本地开发服务器：将 litreview-core 的服务函数镜像为 HTTP API。
//! 生产形态是 Tauri 桌面应用（IPC），此服务器仅供浏览器开发与自动化测试，
//! 两端共用同一套 core 逻辑与数据。

use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
    Json, Router,
};
use litreview_core::db::CoreState;
use litreview_core::model::*;
use litreview_core::service as sv;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;

struct ApiError(litreview_core::CoreError);

impl From<litreview_core::CoreError> for ApiError {
    fn from(e: litreview_core::CoreError) -> Self {
        ApiError(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self.0 {
            litreview_core::CoreError::PaperNotFound(_) | litreview_core::CoreError::NotFound(_) => {
                StatusCode::NOT_FOUND
            }
            litreview_core::CoreError::InvalidImport(_) | litreview_core::CoreError::Msg(_) => {
                StatusCode::BAD_REQUEST
            }
            litreview_core::CoreError::Backup(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(json!({ "error": self.0.to_string() }))).into_response()
    }
}

type ApiResult<T> = Result<T, ApiError>;

#[tokio::main]
async fn main() {
    let state = Arc::new(litreview_core::db::open_library(None).expect("打开资料库失败"));

    let api = Router::new()
        .route("/api/papers", get(list_papers))
        .route("/api/papers/{id}", get(get_paper).patch(patch_paper).delete(delete_paper))
        .route("/api/papers/{id}/restore", post(restore_paper))
        .route("/api/papers/{id}/pdf", get(get_pdf))
        .route("/api/papers/{id}/analysis", get(get_analysis).put(put_analysis))
        .route("/api/papers/{id}/analysis/restore-prev", post(restore_prev))
        .route("/api/papers/{id}/notes", get(get_notes).put(put_notes))
        .route("/api/papers/{id}/evidence", get(list_evidence).put(put_evidence))
        .route("/api/papers/{id}/annotations", get(list_annotations).post(add_annotation))
        .route("/api/annotations/{aid}", patch(patch_annotation).delete(delete_annotation))
        .route("/api/papers/{id}/export", get(export_paper))
        .route("/api/export-papers", get(export_papers_batch))
        .route("/api/export-notes", get(export_notes_batch))
        .route("/api/import/analyze", post(import_analyze))
        .route("/api/import/commit", post(import_commit))
        .route("/api/projects", get(list_projects).post(create_project))
        .route("/api/projects/{id}", patch(patch_project).delete(delete_project))
        .route("/api/tags", get(list_tags))
        .route("/api/tags/{id}", delete(delete_tag))
        .route("/api/compares", get(list_compares).post(create_compare))
        .route("/api/compares/{id}", get(get_compare).put(put_compare).delete(delete_compare))
        .route("/api/compares/{id}/export", get(export_compare))
        .route("/api/backup", post(backup))
        .route("/api/restore", post(restore))
        .route("/api/settings", get(get_settings).put(put_settings))
        .route("/api/template", get(get_template))
        .route("/api/ai/config", get(get_ai_config).put(put_ai_config))
        .route("/api/ai/test", post(test_ai))
        .route("/api/ai/generate-analysis", post(generate_analysis))
        .layer(DefaultBodyLimit::max(512 * 1024 * 1024))
        .with_state(state.clone());

    // 若存在已构建的前端（dist/），则同端口提供静态服务，形成可独立使用的本地网页应用
    let dist_dir = PathBuf::from("dist");
    let app = if dist_dir.exists() {
        let serve = tower_http::services::ServeDir::new(&dist_dir)
            .not_found_service(tower_http::services::ServeFile::new(dist_dir.join("index.html")));
        api.fallback_service(serve)
    } else {
        api
    };
    // 入口页禁用缓存：chunk 带 hash 可长缓存，但 index.html 必须每次校验，
    // 否则应用更新后浏览器仍加载旧入口。API 响应加 no-cache 无副作用。
    let app = app.layer(axum::middleware::from_fn(
        |req: axum::extract::Request, next: axum::middleware::Next| async move {
            let is_entry = req.uri().path() == "/" || req.uri().path().ends_with(".html");
            let resp = next.run(req).await;
            if is_entry {
                let (mut parts, body) = resp.into_parts();
                parts
                    .headers
                    .insert(header::CACHE_CONTROL, "no-cache".parse().unwrap());
                return axum::response::Response::from_parts(parts, body);
            }
            resp
        },
    ));

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 8787));
    let listener = tokio::net::TcpListener::bind(addr).await.expect("端口 8787 被占用");
    println!("dev-server 已启动: http://{addr}");
    if dist_dir.exists() {
        println!("静态前端: dist/（同一地址直接访问）");
    }
    axum::serve(listener, app).await.unwrap();
}

// ---- handlers ----

async fn list_papers(
    State(state): State<Arc<CoreState>>,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Vec<PaperListItem>>> {
    Ok(Json(sv::papers::list_papers(&state, &q)?))
}

async fn get_paper(
    State(state): State<Arc<CoreState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<PaperDetail>> {
    Ok(Json(sv::papers::get_paper(&state, &id)?))
}

async fn patch_paper(
    State(state): State<Arc<CoreState>>,
    Path(id): Path<String>,
    Json(patch): Json<PatchPaper>,
) -> ApiResult<Json<PaperDetail>> {
    Ok(Json(sv::papers::patch_paper(&state, &id, &patch)?))
}

async fn delete_paper(
    State(state): State<Arc<CoreState>>,
    Path(id): Path<String>,
    Query(q): Query<serde_json::Value>,
) -> ApiResult<Json<serde_json::Value>> {
    let mode = q.get("mode").and_then(|m| m.as_str()).unwrap_or("trash");
    match mode {
        "permanent" => {
            sv::papers::delete_paper_permanent(&state, &id)?;
            Ok(Json(json!({ "deleted": true })))
        }
        _ => {
            sv::papers::trash_paper(&state, &id)?;
            Ok(Json(json!({ "trashed": true })))
        }
    }
}

async fn restore_paper(
    State(state): State<Arc<CoreState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    sv::papers::restore_paper(&state, &id)?;
    Ok(Json(json!({ "restored": true })))
}

async fn get_pdf(
    State(state): State<Arc<CoreState>>,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let bytes = sv::import::get_pdf_bytes(&state, &id)?;
    Ok((
        [(header::CONTENT_TYPE, "application/pdf")],
        Body::from(bytes),
    )
        .into_response())
}

async fn get_analysis(
    State(state): State<Arc<CoreState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<Option<AnalysisResponse>>> {
    Ok(Json(sv::import::get_analysis(&state, &id)?))
}

async fn put_analysis(
    State(state): State<Arc<CoreState>>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<Json<AnalysisResponse>> {
    let md = body
        .get("md")
        .and_then(|m| m.as_str())
        .ok_or_else(|| ApiError(litreview_core::CoreError::Msg("缺少 md 字段".into())))?;
    Ok(Json(sv::import::set_analysis(&state, &id, md)?))
}

async fn restore_prev(
    State(state): State<Arc<CoreState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<AnalysisResponse>> {
    Ok(Json(sv::import::restore_prev_analysis(&state, &id)?))
}

async fn get_notes(
    State(state): State<Arc<CoreState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<NoteResponse>> {
    Ok(Json(sv::papers::get_note(&state, &id)?))
}

async fn put_notes(
    State(state): State<Arc<CoreState>>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<Json<NoteResponse>> {
    let content = body.get("content").and_then(|c| c.as_str()).unwrap_or("");
    Ok(Json(sv::papers::set_note(&state, &id, content)?))
}

async fn list_evidence(
    State(state): State<Arc<CoreState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<EvidenceRecord>>> {
    Ok(Json(sv::import::list_evidence(&state, &id)?))
}

async fn put_evidence(
    State(state): State<Arc<CoreState>>,
    Path(id): Path<String>,
    Json(input): Json<EvidenceInput>,
) -> ApiResult<Json<EvidenceRecord>> {
    Ok(Json(sv::import::upsert_evidence(&state, &id, &input)?))
}

async fn list_annotations(
    State(state): State<Arc<CoreState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<PdfAnnotation>>> {
    Ok(Json(sv::annotations::list_annotations(&state, &id)?))
}

async fn add_annotation(
    State(state): State<Arc<CoreState>>,
    Path(id): Path<String>,
    Json(input): Json<AnnotationInput>,
) -> ApiResult<Json<PdfAnnotation>> {
    Ok(Json(sv::annotations::add_annotation(&state, &id, &input)?))
}

#[derive(serde::Deserialize, Default)]
struct AnnIdQuery {
    #[serde(default)]
    paper: Option<String>,
}

async fn patch_annotation(
    State(state): State<Arc<CoreState>>,
    Path(aid): Path<i64>,
    Json(patch): Json<AnnotationPatch>,
) -> ApiResult<Json<PdfAnnotation>> {
    Ok(Json(sv::annotations::update_annotation(&state, aid, &patch)?))
}

async fn delete_annotation(
    State(state): State<Arc<CoreState>>,
    Path(aid): Path<i64>,
) -> ApiResult<Json<serde_json::Value>> {
    sv::annotations::delete_annotation(&state, aid)?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(serde::Deserialize)]
struct ExportQuery {
    #[serde(default)]
    notes: Option<bool>,
}

async fn export_paper(
    State(state): State<Arc<CoreState>>,
    Path(id): Path<String>,
    Query(q): Query<ExportQuery>,
) -> ApiResult<Response> {
    let r = sv::export::export_paper(&state, &id, q.notes.unwrap_or(true))?;
    download_response(r.filename, r.content)
}

#[derive(serde::Deserialize)]
struct BatchExportQuery {
    #[serde(default)]
    ids: String,
}

async fn export_papers_batch(
    State(state): State<Arc<CoreState>>,
    Query(q): Query<BatchExportQuery>,
) -> ApiResult<Response> {
    let ids: Vec<String> = q.ids.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    let r = sv::export::export_papers_batch(&state, &ids)?;
    download_response(r.filename, r.content)
}

async fn import_analyze(
    State(state): State<Arc<CoreState>>,
    mut multipart: Multipart,
) -> ApiResult<Json<ImportPreview>> {
    let mut pdf_bytes: Option<Vec<u8>> = None;
    let mut pdf_filename: Option<String> = None;
    let mut md_text: Option<String> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError(litreview_core::CoreError::Msg(format!("上传解析失败：{e}"))))?
    {
        match field.name().unwrap_or("") {
            "pdf" => {
                pdf_filename = field.file_name().map(|s| s.to_string());
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| ApiError(litreview_core::CoreError::Msg(format!("读取 PDF 失败：{e}"))))?;
                pdf_bytes = Some(data.to_vec());
            }
            "md" => {
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| ApiError(litreview_core::CoreError::Msg(format!("读取 MD 失败：{e}"))))?;
                md_text = Some(String::from_utf8_lossy(&data).to_string());
            }
            "md_text" => {
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| ApiError(litreview_core::CoreError::Msg(format!("读取粘贴内容失败：{e}"))))?;
                md_text = Some(String::from_utf8_lossy(&data).to_string());
            }
            _ => {}
        }
    }
    Ok(Json(sv::import::analyze_import(
        &state, pdf_bytes, pdf_filename, md_text,
    )?))
}

async fn import_commit(
    State(state): State<Arc<CoreState>>,
    Json(req): Json<CommitRequest>,
) -> ApiResult<Json<CommitResult>> {
    Ok(Json(sv::import::commit_import(&state, &req)?))
}

async fn list_projects(
    State(state): State<Arc<CoreState>>,
) -> ApiResult<Json<Vec<ProjectInfo>>> {
    Ok(Json(sv::taxonomy::list_projects(&state)?))
}

async fn create_project(
    State(state): State<Arc<CoreState>>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<Json<ProjectInfo>> {
    let name = body.get("name").and_then(|n| n.as_str()).unwrap_or("");
    Ok(Json(sv::taxonomy::create_project(&state, name)?))
}

async fn patch_project(
    State(state): State<Arc<CoreState>>,
    Path(id): Path<i64>,
    Json(body): Json<serde_json::Value>,
) -> ApiResult<Json<serde_json::Value>> {
    let name = body.get("name").and_then(|n| n.as_str()).unwrap_or("");
    sv::taxonomy::rename_project(&state, id, name)?;
    Ok(Json(json!({ "ok": true })))
}

async fn delete_project(
    State(state): State<Arc<CoreState>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<serde_json::Value>> {
    sv::taxonomy::delete_project(&state, id)?;
    Ok(Json(json!({ "ok": true })))
}

async fn list_tags(State(state): State<Arc<CoreState>>) -> ApiResult<Json<Vec<TagInfo>>> {
    Ok(Json(sv::taxonomy::list_tags(&state)?))
}

async fn delete_tag(
    State(state): State<Arc<CoreState>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<serde_json::Value>> {
    sv::taxonomy::delete_tag(&state, id)?;
    Ok(Json(json!({ "ok": true })))
}

async fn list_compares(
    State(state): State<Arc<CoreState>>,
) -> ApiResult<Json<Vec<CompareRecord>>> {
    Ok(Json(sv::compares::list_compares(&state)?))
}

async fn create_compare(
    State(state): State<Arc<CoreState>>,
    Json(input): Json<CompareInput>,
) -> ApiResult<Json<CompareRecord>> {
    Ok(Json(sv::compares::create_compare(&state, &input)?))
}

async fn get_compare(
    State(state): State<Arc<CoreState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<CompareWithStatus>> {
    Ok(Json(sv::compares::get_compare(&state, &id)?))
}

async fn put_compare(
    State(state): State<Arc<CoreState>>,
    Path(id): Path<String>,
    Json(input): Json<CompareInput>,
) -> ApiResult<Json<CompareRecord>> {
    Ok(Json(sv::compares::update_compare(&state, &id, &input)?))
}

async fn delete_compare(
    State(state): State<Arc<CoreState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    sv::compares::delete_compare(&state, &id)?;
    Ok(Json(json!({ "ok": true })))
}

async fn export_compare(
    State(state): State<Arc<CoreState>>,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let r = sv::export::export_compare(&state, &id)?;
    download_response(r.filename, r.content)
}

async fn export_notes_batch(
    State(state): State<Arc<CoreState>>,
    Query(q): Query<BatchExportQuery>,
) -> ApiResult<Response> {
    let ids: Vec<String> = q.ids.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    let r = sv::export::export_notes_batch(&state, &ids)?;
    download_response(r.filename, r.content)
}

#[derive(serde::Deserialize)]
struct PathBody {
    path: String,
}

async fn backup(
    State(state): State<Arc<CoreState>>,
    Json(body): Json<PathBody>,
) -> ApiResult<Json<BackupResult>> {
    Ok(Json(
        sv::backup::backup_to(&state, &PathBuf::from(&body.path))?,
    ))
}

#[derive(serde::Deserialize)]
struct RestoreBody {
    zip_path: String,
    #[serde(default)]
    target_dir: Option<String>,
}

async fn restore(
    State(state): State<Arc<CoreState>>,
    Json(body): Json<RestoreBody>,
) -> ApiResult<Json<RestoreResult>> {
    let target = match body.target_dir {
        Some(t) if !t.trim().is_empty() => PathBuf::from(t),
        _ => {
            let stamp = chrono_like_stamp();
            PathBuf::from("data").join(format!("restored-library-{stamp}"))
        }
    };
    Ok(Json(sv::backup::restore_from(
        &state,
        &PathBuf::from(&body.zip_path),
        &target,
    )?))
}

fn chrono_like_stamp() -> String {
    let ts = litreview_core::db::now_ts();
    format!("{ts}")
}

async fn get_settings(
    State(state): State<Arc<CoreState>>,
) -> ApiResult<Json<SettingsResponse>> {
    let dir = state.inner.lock().unwrap().library_dir.clone();
    Ok(Json(SettingsResponse {
        library_dir: dir.to_string_lossy().to_string(),
    }))
}

#[derive(serde::Deserialize)]
struct SettingsBody {
    library_dir: String,
}

async fn put_settings(
    State(state): State<Arc<CoreState>>,
    Json(body): Json<SettingsBody>,
) -> ApiResult<Json<SettingsResponse>> {
    let dir = PathBuf::from(&body.library_dir);
    sv::backup::switch_library(&state, &dir)?;
    Ok(Json(SettingsResponse {
        library_dir: dir.to_string_lossy().to_string(),
    }))
}

// ---------- AI 服务 ----------

async fn get_ai_config(State(state): State<Arc<CoreState>>) -> Json<litreview_core::service::ai::AiConfig> {
    Json(litreview_core::service::ai::get_config(&state))
}

async fn put_ai_config(
    State(state): State<Arc<CoreState>>,
    Json(cfg): Json<litreview_core::service::ai::AiConfig>,
) -> ApiResult<Json<litreview_core::service::ai::AiConfig>> {
    litreview_core::service::ai::set_config(&state, &cfg)?;
    Ok(Json(cfg))
}

async fn test_ai(State(state): State<Arc<CoreState>>) -> ApiResult<Response> {
    let cfg = litreview_core::service::ai::get_config(&state);
    // 阻塞调用放线程池，避免卡住异步运行时
    let reply = tokio::task::spawn_blocking(move || {
        litreview_core::service::ai::test_connection(&cfg)
    })
    .await
    .map_err(|e| ApiError(litreview_core::CoreError::Msg(format!("{e}"))))?
    .map_err(ApiError)?;
    Ok(Json(json!({ "ok": true, "reply": reply })).into_response())
}

#[derive(serde::Deserialize, Default)]
struct GenAnalysisBody {
    /// PDF 全文文本（前端 pdf.js 提取）
    #[serde(default)]
    pdf_text: String,
    /// 额外用户要求（可选）
    #[serde(default)]
    extra_instructions: String,
}

/// SSE 流式生成文献分析：data: {"delta": "..."}，结束 data: {"done": true, "full": "..."}
async fn generate_analysis(
    State(state): State<Arc<CoreState>>,
    Json(body): Json<GenAnalysisBody>,
) -> Response {
    use futures_util::StreamExt;
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<axum::body::Bytes, std::io::Error>>(32);
    tokio::task::spawn_blocking(move || {
        let cfg = litreview_core::service::ai::get_config(&state);
        let system = litreview_core::service::ai::analysis_system_prompt();
        let user = format!(
            "请分析以下论文全文并按模板输出。{}

=== 论文全文开始 ===
{}
=== 论文全文结束 ===",
            if body.extra_instructions.trim().is_empty() {
                String::new()
            } else {
                format!("
额外要求：{}", body.extra_instructions.trim())
            },
            litreview_core::service::ai::truncate_to_budget(&body.pdf_text, cfg.max_context_tokens)
        );
        let tx2 = tx.clone();
        let result = litreview_core::service::ai::generate_stream(&cfg, &system, &user, &mut |delta| {
            let payload = format!("data: {}

", json!({ "delta": delta }));
            let _ = tx2.blocking_send(Ok(axum::body::Bytes::from(payload)));
        });
        let tail = match result {
            Ok(full) => format!("data: {}

", json!({ "done": true, "full": full })),
            Err(e) => format!("data: {}

", json!({ "error": e.to_string() })),
        };
        let _ = tx.blocking_send(Ok(axum::body::Bytes::from(tail)));
    });
    let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    let body = axum::body::Body::from_stream(stream);
    (
        [(header::CONTENT_TYPE, "text/event-stream"), (header::CACHE_CONTROL, "no-cache")],
        body,
    ).into_response()
}

async fn get_template() -> Json<serde_json::Value> {
    Json(json!({
        "filename": "文献AI分析模板.md",
        "content": include_str!("../../../文献AI分析模板.md"),
    }))
}

fn download_response(filename: String, content: String) -> ApiResult<Response> {
    let encoded: String = url_encode(&filename);
    let headers = [
        (header::CONTENT_TYPE, "text/markdown; charset=utf-8".to_string()),
        (
            header::CONTENT_DISPOSITION,
            format!("attachment; filename*=UTF-8''{encoded}"),
        ),
    ];
    Ok((headers, content).into_response())
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
