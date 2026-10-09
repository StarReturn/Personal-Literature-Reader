//! Tauri 2 桌面外壳：把 litreview-core 的服务函数暴露为 IPC 命令。
//! 业务逻辑全部在 core 中，与 dev-server 共用。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use litreview_core::db::CoreState;
use litreview_core::model::*;
use litreview_core::service as sv;
use std::path::PathBuf;
use tauri::ipc::Response;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, State};

/// 显示并聚焦主窗口（托盘左键/菜单调用）。
fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

type R<T> = Result<T, String>;
fn cerr(e: litreview_core::CoreError) -> String {
    e.to_string()
}

#[tauri::command]
fn list_papers(state: State<'_, std::sync::Arc<CoreState>>, query: ListQuery) -> R<Vec<PaperListItem>> {
    sv::papers::list_papers(&state, &query).map_err(cerr)
}

#[tauri::command]
fn get_paper(state: State<'_, std::sync::Arc<CoreState>>, id: String) -> R<PaperDetail> {
    sv::papers::get_paper(&state, &id).map_err(cerr)
}

#[tauri::command]
fn patch_paper(state: State<'_, std::sync::Arc<CoreState>>, id: String, patch: PatchPaper) -> R<PaperDetail> {
    sv::papers::patch_paper(&state, &id, &patch).map_err(cerr)
}

#[tauri::command]
fn delete_paper(state: State<'_, std::sync::Arc<CoreState>>, id: String, permanent: bool) -> R<()> {
    if permanent {
        sv::papers::delete_paper_permanent(&state, &id).map_err(cerr)
    } else {
        sv::papers::trash_paper(&state, &id).map_err(cerr)
    }
}

#[tauri::command]
fn restore_paper(state: State<'_, std::sync::Arc<CoreState>>, id: String) -> R<()> {
    sv::papers::restore_paper(&state, &id).map_err(cerr)
}

#[tauri::command]
fn get_pdf(state: State<'_, std::sync::Arc<CoreState>>, id: String) -> R<Response> {
    let bytes = sv::import::get_pdf_bytes(&state, &id).map_err(cerr)?;
    Ok(Response::new(bytes))
}

#[tauri::command]
fn get_analysis(state: State<'_, std::sync::Arc<CoreState>>, id: String) -> R<Option<AnalysisResponse>> {
    sv::import::get_analysis(&state, &id).map_err(cerr)
}

#[tauri::command]
fn set_analysis(state: State<'_, std::sync::Arc<CoreState>>, id: String, md: String) -> R<AnalysisResponse> {
    sv::import::set_analysis(&state, &id, &md).map_err(cerr)
}

#[tauri::command]
fn restore_prev_analysis(state: State<'_, std::sync::Arc<CoreState>>, id: String) -> R<AnalysisResponse> {
    sv::import::restore_prev_analysis(&state, &id).map_err(cerr)
}

#[tauri::command]
fn get_note(state: State<'_, std::sync::Arc<CoreState>>, id: String) -> R<NoteResponse> {
    sv::papers::get_note(&state, &id).map_err(cerr)
}

#[tauri::command]
fn set_note(state: State<'_, std::sync::Arc<CoreState>>, id: String, content: String) -> R<NoteResponse> {
    sv::papers::set_note(&state, &id, &content).map_err(cerr)
}

#[tauri::command]
fn list_evidence(state: State<'_, std::sync::Arc<CoreState>>, id: String) -> R<Vec<EvidenceRecord>> {
    sv::import::list_evidence(&state, &id).map_err(cerr)
}

#[tauri::command]
fn upsert_evidence(
    state: State<'_, std::sync::Arc<CoreState>>,
    id: String,
    input: EvidenceInput,
) -> R<EvidenceRecord> {
    sv::import::upsert_evidence(&state, &id, &input).map_err(cerr)
}

#[tauri::command]
fn export_paper(state: State<'_, std::sync::Arc<CoreState>>, id: String, include_notes: bool) -> R<ExportResult> {
    sv::export::export_paper(&state, &id, include_notes).map_err(cerr)
}

#[tauri::command]
fn export_papers_batch(state: State<'_, std::sync::Arc<CoreState>>, ids: Vec<String>) -> R<ExportResult> {
    sv::export::export_papers_batch(&state, &ids).map_err(cerr)
}

#[tauri::command]
fn export_notes_batch(state: State<'_, std::sync::Arc<CoreState>>, ids: Vec<String>) -> R<ExportResult> {
    sv::export::export_notes_batch(&state, &ids).map_err(cerr)
}

#[tauri::command]
fn analyze_import(
    state: State<'_, std::sync::Arc<CoreState>>,
    pdf: Option<Vec<u8>>,
    pdf_filename: Option<String>,
    md_text: Option<String>,
) -> R<ImportPreview> {
    sv::import::analyze_import(&state, pdf, pdf_filename, md_text).map_err(cerr)
}

#[tauri::command]
fn analyze_import_paths(
    state: State<'_, std::sync::Arc<CoreState>>,
    pdf_path: Option<String>,
    md_path: Option<String>,
    md_text: Option<String>,
) -> R<ImportPreview> {
    sv::import::analyze_import_paths(
        &state,
        pdf_path.as_deref().map(PathBuf::from).as_deref(),
        md_path.as_deref().map(PathBuf::from).as_deref(),
        md_text,
    )
    .map_err(cerr)
}

/// 把导出的 Markdown 保存到用户选择的路径。
#[tauri::command]
fn save_text_file(path: String, content: String) -> R<()> {
    std::fs::write(&path, content).map_err(|e| e.to_string())
}

/// 读取用户通过对话框选择的文本文件（补录 MD 导入用）。
#[tauri::command]
fn read_text_file(path: String) -> R<String> {
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}

/// 读取对话框选择文件的字节（PDF 文本提取等前端处理用）。
#[tauri::command]
fn read_binary_file(path: String) -> R<Vec<u8>> {
    std::fs::read(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn commit_import(state: State<'_, std::sync::Arc<CoreState>>, req: CommitRequest) -> R<CommitResult> {
    sv::import::commit_import(&state, &req).map_err(cerr)
}

#[tauri::command]
fn list_projects(state: State<'_, std::sync::Arc<CoreState>>) -> R<Vec<ProjectInfo>> {
    sv::taxonomy::list_projects(&state).map_err(cerr)
}

#[tauri::command]
fn create_project(state: State<'_, std::sync::Arc<CoreState>>, name: String) -> R<ProjectInfo> {
    sv::taxonomy::create_project(&state, &name).map_err(cerr)
}

#[tauri::command]
fn rename_project(state: State<'_, std::sync::Arc<CoreState>>, id: i64, name: String) -> R<()> {
    sv::taxonomy::rename_project(&state, id, &name).map_err(cerr)
}

#[tauri::command]
fn delete_project(state: State<'_, std::sync::Arc<CoreState>>, id: i64) -> R<()> {
    sv::taxonomy::delete_project(&state, id).map_err(cerr)
}

#[tauri::command]
fn list_tags(state: State<'_, std::sync::Arc<CoreState>>) -> R<Vec<TagInfo>> {
    sv::taxonomy::list_tags(&state).map_err(cerr)
}

#[tauri::command]
fn delete_tag(state: State<'_, std::sync::Arc<CoreState>>, id: i64) -> R<()> {
    sv::taxonomy::delete_tag(&state, id).map_err(cerr)
}

#[tauri::command]
fn list_compares(state: State<'_, std::sync::Arc<CoreState>>) -> R<Vec<CompareRecord>> {
    sv::compares::list_compares(&state).map_err(cerr)
}

#[tauri::command]
fn create_compare(state: State<'_, std::sync::Arc<CoreState>>, input: CompareInput) -> R<CompareRecord> {
    sv::compares::create_compare(&state, &input).map_err(cerr)
}

#[tauri::command]
fn get_compare(state: State<'_, std::sync::Arc<CoreState>>, id: String) -> R<CompareWithStatus> {
    sv::compares::get_compare(&state, &id).map_err(cerr)
}

#[tauri::command]
fn update_compare(state: State<'_, std::sync::Arc<CoreState>>, id: String, input: CompareInput) -> R<CompareRecord> {
    sv::compares::update_compare(&state, &id, &input).map_err(cerr)
}

#[tauri::command]
fn delete_compare(state: State<'_, std::sync::Arc<CoreState>>, id: String) -> R<()> {
    sv::compares::delete_compare(&state, &id).map_err(cerr)
}

#[tauri::command]
fn export_compare(state: State<'_, std::sync::Arc<CoreState>>, id: String) -> R<ExportResult> {
    sv::export::export_compare(&state, &id).map_err(cerr)
}

#[tauri::command]
fn backup(state: State<'_, std::sync::Arc<CoreState>>, target_dir: String) -> R<BackupResult> {
    sv::backup::backup_to(&state, &PathBuf::from(target_dir)).map_err(cerr)
}

#[tauri::command]
fn restore(state: State<'_, std::sync::Arc<CoreState>>, zip_path: String, target_dir: Option<String>) -> R<RestoreResult> {
    let target = match target_dir {
        Some(t) if !t.trim().is_empty() => PathBuf::from(t),
        _ => {
            let ts = litreview_core::db::now_ts();
            litreview_core::db::app_data_dir().join(format!("restored-library-{ts}"))
        }
    };
    sv::backup::restore_from(&state, &PathBuf::from(zip_path), &target).map_err(cerr)
}

#[tauri::command]
fn get_settings(state: State<'_, std::sync::Arc<CoreState>>) -> R<SettingsResponse> {
    let dir = state.inner.lock().unwrap().library_dir.clone();
    Ok(SettingsResponse {
        library_dir: dir.to_string_lossy().to_string(),
    })
}

#[tauri::command]
fn switch_library(state: State<'_, std::sync::Arc<CoreState>>, library_dir: String) -> R<SettingsResponse> {
    sv::backup::switch_library(&state, &PathBuf::from(&library_dir)).map_err(cerr)?;
    Ok(SettingsResponse { library_dir })
}

#[tauri::command]
fn list_annotations(state: State<'_, std::sync::Arc<CoreState>>, id: String) -> R<Vec<PdfAnnotation>> {
    sv::annotations::list_annotations(&state, &id).map_err(cerr)
}

#[tauri::command]
fn add_annotation(state: State<'_, std::sync::Arc<CoreState>>, id: String, input: AnnotationInput) -> R<PdfAnnotation> {
    sv::annotations::add_annotation(&state, &id, &input).map_err(cerr)
}

#[tauri::command]
fn update_annotation(state: State<'_, std::sync::Arc<CoreState>>, aid: i64, patch: AnnotationPatch) -> R<PdfAnnotation> {
    sv::annotations::update_annotation(&state, aid, &patch).map_err(cerr)
}

#[tauri::command]
fn delete_annotation(state: State<'_, std::sync::Arc<CoreState>>, aid: i64) -> R<()> {
    sv::annotations::delete_annotation(&state, aid).map_err(cerr)
}

// ---------- AI 服务 ----------

#[tauri::command]
fn ai_get_config(state: State<'_, std::sync::Arc<CoreState>>) -> litreview_core::service::ai::AiConfig {
    sv::ai::get_config(&state)
}

#[tauri::command]
fn ai_set_config(state: State<'_, std::sync::Arc<CoreState>>, cfg: litreview_core::service::ai::AiConfig) -> R<()> {
    sv::ai::set_config(&state, &cfg).map_err(cerr)
}

#[tauri::command]
fn ai_test_connection(state: State<'_, std::sync::Arc<CoreState>>) -> R<String> {
    let cfg = sv::ai::get_config(&state);
    sv::ai::test_connection(&cfg).map_err(cerr)
}

/// 流式生成文献分析：通过 `ai_chunk` 事件推送增量，返回完整文本。
#[tauri::command]
async fn ai_generate_analysis(
    app: tauri::AppHandle,
    state: tauri::State<'_, std::sync::Arc<CoreState>>,
    pdf_text: String,
    extra_instructions: Option<String>,
) -> R<String> {
    use tauri::Emitter;
    let state: std::sync::Arc<CoreState> = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = sv::ai::get_config(&state);
        let system = sv::ai::analysis_system_prompt();
        let extra = extra_instructions.unwrap_or_default();
        let user = format!(
            "请分析以下论文全文并按模板输出。{}

=== 论文全文开始 ===
{}
=== 论文全文结束 ===",
            if extra.trim().is_empty() { String::new() } else { format!("
额外要求：{}", extra.trim()) },
            sv::ai::truncate_to_budget(&pdf_text, cfg.max_context_tokens)
        );
        let app2 = app.clone();
        sv::ai::generate_stream(&cfg, &system, &user, &mut |delta| {
            let _ = app2.emit("ai_chunk", delta);
        })
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("任务执行失败：{e}"))?
}

/// 流式生成组会大纲：ai_chunk 事件推送增量，返回完整 Markdown。
#[tauri::command]
async fn ai_generate_outline(
    app: tauri::AppHandle,
    state: tauri::State<'_, std::sync::Arc<CoreState>>,
    paper_ids: Vec<String>,
    extra: Option<String>,
) -> R<String> {
    use tauri::Emitter;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut papers: Vec<(String, String)> = Vec::new();
        for pid in &paper_ids {
            let detail = match sv::papers::get_paper(&state, pid) {
                Ok(d) => d,
                Err(_) => continue,
            };
            let analysis = sv::import::get_analysis(&state, pid)
                .ok()
                .flatten()
                .map(|a| a.md_content)
                .unwrap_or_default();
            if analysis.is_empty() {
                continue;
            }
            let per = 8000;
            let truncated = sv::ai::truncate_to_budget(&analysis, per);
            papers.push((detail.item.title.clone(), truncated));
        }
        if papers.is_empty() {
            return Err("所选文献均无 AI 分析内容，请先导入或生成分析".to_string());
        }
        let cfg = sv::ai::get_config(&state);
        let system = sv::ai::outline_system_prompt();
        let user = sv::ai::build_outline_prompt(&papers, &extra.unwrap_or_default());
        let app2 = app.clone();
        sv::ai::generate_stream(&cfg, &system, &user, &mut |delta| {
            let _ = app2.emit("ai_chunk", delta);
        })
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("任务执行失败：{e}"))?
}

#[tauri::command]
fn get_template() -> serde_json::Value {
    serde_json::json!({
        "filename": "文献AI分析模板.md",
        "content": include_str!("../../文献AI分析模板.md"),
    })
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // 已有实例被再次启动：唤起主窗口（若在托盘中隐藏则一并显示）
            show_main(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // 桌面版数据目录放用户 AppData（有写权限、升级不丢、独立于安装目录），
            // 对应需求 8.1：资料库独立于应用安装目录。
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            std::fs::create_dir_all(&data_dir)?;
            std::env::set_var("LITREVIEW_DATA", &data_dir);
            let lib = std::sync::Arc::new(
                litreview_core::db::open_library(None)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?,
            );
            app.manage(lib);

            // 系统托盘：兔子图标常驻；左键恢复主窗口，右键菜单（打开/退出）
            let show = tauri::menu::MenuItem::with_id(app, "show", "打开文献综述中心", true, None::<&str>)?;
            let quit = tauri::menu::MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = tauri::menu::Menu::with_items(app, &[&show, &quit])?;
            TrayIconBuilder::with_id("main-tray")
                .icon(app.default_window_icon().expect("缺省窗口图标").clone())
                .tooltip("文献综述中心")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main(tray.app_handle());
                    }
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // 关闭主窗口 = 隐藏到托盘（数据与后台逻辑继续保留）；真正的退出走托盘菜单
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            list_papers, get_paper, patch_paper, delete_paper, restore_paper, get_pdf,
            get_analysis, set_analysis, restore_prev_analysis, get_note, set_note,
            list_evidence, upsert_evidence, export_paper, export_papers_batch, export_notes_batch, analyze_import, analyze_import_paths,
            save_text_file, read_text_file, read_binary_file, commit_import,
            list_projects, create_project, rename_project, delete_project, list_tags,
            delete_tag, list_compares, create_compare, get_compare, update_compare,
            delete_compare, export_compare, backup, restore, get_settings, switch_library,
            get_template, list_annotations, add_annotation, update_annotation, delete_annotation,
        ])
        .run(tauri::generate_context!())
        .expect("运行 Tauri 应用失败");
}
