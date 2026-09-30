//! Tauri 2 桌面外壳：把 litreview-core 的服务函数暴露为 IPC 命令。
//! 业务逻辑全部在 core 中，与 dev-server 共用。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use litreview_core::db::CoreState;
use litreview_core::model::*;
use litreview_core::service as sv;
use rusqlite::{params, OptionalExtension};
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
fn list_papers(state: State<'_, CoreState>, query: ListQuery) -> R<Vec<PaperListItem>> {
    sv::papers::list_papers(&state, &query).map_err(cerr)
}

#[tauri::command]
fn get_paper(state: State<'_, CoreState>, id: String) -> R<PaperDetail> {
    sv::papers::get_paper(&state, &id).map_err(cerr)
}

#[tauri::command]
fn patch_paper(state: State<'_, CoreState>, id: String, patch: PatchPaper) -> R<PaperDetail> {
    sv::papers::patch_paper(&state, &id, &patch).map_err(cerr)
}

#[tauri::command]
fn delete_paper(state: State<'_, CoreState>, id: String, permanent: bool) -> R<()> {
    if permanent {
        sv::papers::delete_paper_permanent(&state, &id).map_err(cerr)
    } else {
        sv::papers::trash_paper(&state, &id).map_err(cerr)
    }
}

#[tauri::command]
fn restore_paper(state: State<'_, CoreState>, id: String) -> R<()> {
    sv::papers::restore_paper(&state, &id).map_err(cerr)
}

#[tauri::command]
fn get_pdf(state: State<'_, CoreState>, id: String) -> R<Response> {
    let bytes = sv::import::get_pdf_bytes(&state, &id).map_err(cerr)?;
    Ok(Response::new(bytes))
}

#[tauri::command]
fn get_analysis(state: State<'_, CoreState>, id: String) -> R<Option<AnalysisResponse>> {
    sv::import::get_analysis(&state, &id).map_err(cerr)
}

#[tauri::command]
fn set_analysis(state: State<'_, CoreState>, id: String, md: String) -> R<AnalysisResponse> {
    sv::import::set_analysis(&state, &id, &md).map_err(cerr)
}

#[tauri::command]
fn restore_prev_analysis(state: State<'_, CoreState>, id: String) -> R<AnalysisResponse> {
    sv::import::restore_prev_analysis(&state, &id).map_err(cerr)
}

#[tauri::command]
fn get_note(state: State<'_, CoreState>, id: String) -> R<NoteResponse> {
    sv::papers::get_note(&state, &id).map_err(cerr)
}

#[tauri::command]
fn set_note(state: State<'_, CoreState>, id: String, content: String) -> R<NoteResponse> {
    sv::papers::set_note(&state, &id, &content).map_err(cerr)
}

#[tauri::command]
fn list_evidence(state: State<'_, CoreState>, id: String) -> R<Vec<EvidenceRecord>> {
    sv::import::list_evidence(&state, &id).map_err(cerr)
}

#[tauri::command]
fn upsert_evidence(
    state: State<'_, CoreState>,
    id: String,
    input: EvidenceInput,
) -> R<EvidenceRecord> {
    sv::import::upsert_evidence(&state, &id, &input).map_err(cerr)
}

#[tauri::command]
fn export_paper(state: State<'_, CoreState>, id: String, include_notes: bool) -> R<ExportResult> {
    sv::export::export_paper(&state, &id, include_notes).map_err(cerr)
}

#[tauri::command]
fn export_papers_batch(state: State<'_, CoreState>, ids: Vec<String>) -> R<ExportResult> {
    sv::export::export_papers_batch(&state, &ids).map_err(cerr)
}

#[tauri::command]
fn export_notes_batch(state: State<'_, CoreState>, ids: Vec<String>) -> R<ExportResult> {
    sv::export::export_notes_batch(&state, &ids).map_err(cerr)
}

#[tauri::command]
fn analyze_import(
    state: State<'_, CoreState>,
    pdf: Option<Vec<u8>>,
    pdf_filename: Option<String>,
    md_text: Option<String>,
) -> R<ImportPreview> {
    sv::import::analyze_import(&state, pdf, pdf_filename, md_text).map_err(cerr)
}

#[tauri::command]
fn analyze_import_paths(
    state: State<'_, CoreState>,
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

#[tauri::command]
fn commit_import(state: State<'_, CoreState>, req: CommitRequest) -> R<CommitResult> {
    sv::import::commit_import(&state, &req).map_err(cerr)
}

#[tauri::command]
fn list_projects(state: State<'_, CoreState>) -> R<Vec<ProjectInfo>> {
    sv::taxonomy::list_projects(&state).map_err(cerr)
}

#[tauri::command]
fn create_project(state: State<'_, CoreState>, name: String) -> R<ProjectInfo> {
    sv::taxonomy::create_project(&state, &name).map_err(cerr)
}

#[tauri::command]
fn rename_project(state: State<'_, CoreState>, id: i64, name: String) -> R<()> {
    sv::taxonomy::rename_project(&state, id, &name).map_err(cerr)
}

#[tauri::command]
fn delete_project(state: State<'_, CoreState>, id: i64) -> R<()> {
    sv::taxonomy::delete_project(&state, id).map_err(cerr)
}

#[tauri::command]
fn list_tags(state: State<'_, CoreState>) -> R<Vec<TagInfo>> {
    sv::taxonomy::list_tags(&state).map_err(cerr)
}

#[tauri::command]
fn delete_tag(state: State<'_, CoreState>, id: i64) -> R<()> {
    sv::taxonomy::delete_tag(&state, id).map_err(cerr)
}

#[tauri::command]
fn list_compares(state: State<'_, CoreState>) -> R<Vec<CompareRecord>> {
    sv::compares::list_compares(&state).map_err(cerr)
}

#[tauri::command]
fn create_compare(state: State<'_, CoreState>, input: CompareInput) -> R<CompareRecord> {
    sv::compares::create_compare(&state, &input).map_err(cerr)
}

#[tauri::command]
fn get_compare(state: State<'_, CoreState>, id: String) -> R<CompareWithStatus> {
    sv::compares::get_compare(&state, &id).map_err(cerr)
}

#[tauri::command]
fn update_compare(state: State<'_, CoreState>, id: String, input: CompareInput) -> R<CompareRecord> {
    sv::compares::update_compare(&state, &id, &input).map_err(cerr)
}

#[tauri::command]
fn delete_compare(state: State<'_, CoreState>, id: String) -> R<()> {
    sv::compares::delete_compare(&state, &id).map_err(cerr)
}

#[tauri::command]
fn export_compare(state: State<'_, CoreState>, id: String) -> R<ExportResult> {
    sv::export::export_compare(&state, &id).map_err(cerr)
}

#[tauri::command]
fn backup(state: State<'_, CoreState>, target_dir: String) -> R<BackupResult> {
    sv::backup::backup_to(&state, &PathBuf::from(target_dir)).map_err(cerr)
}

#[tauri::command]
fn restore(state: State<'_, CoreState>, zip_path: String, target_dir: Option<String>) -> R<RestoreResult> {
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
fn get_settings(state: State<'_, CoreState>) -> R<SettingsResponse> {
    let dir = state.inner.lock().unwrap().library_dir.clone();
    Ok(SettingsResponse {
        library_dir: dir.to_string_lossy().to_string(),
    })
}

#[tauri::command]
fn switch_library(state: State<'_, CoreState>, library_dir: String) -> R<SettingsResponse> {
    sv::backup::switch_library(&state, &PathBuf::from(&library_dir)).map_err(cerr)?;
    Ok(SettingsResponse { library_dir })
}

#[tauri::command]
fn list_annotations(state: State<'_, CoreState>, id: String) -> R<Vec<PdfAnnotation>> {
    sv::annotations::list_annotations(&state, &id).map_err(cerr)
}

#[tauri::command]
fn add_annotation(state: State<'_, CoreState>, id: String, input: AnnotationInput) -> R<PdfAnnotation> {
    sv::annotations::add_annotation(&state, &id, &input).map_err(cerr)
}

#[tauri::command]
fn update_annotation(state: State<'_, CoreState>, aid: i64, patch: AnnotationPatch) -> R<PdfAnnotation> {
    sv::annotations::update_annotation(&state, aid, &patch).map_err(cerr)
}

#[tauri::command]
fn delete_annotation(state: State<'_, CoreState>, aid: i64) -> R<()> {
    sv::annotations::delete_annotation(&state, aid).map_err(cerr)
}

// ---------- 桌宠：配置存 meta 表，形象存 资料库/pets/current.<ext> ----------

fn pet_meta_get(state: &CoreState, key: &str) -> Option<String> {
    let inner = state.inner.lock().unwrap();
    inner
        .conn
        .query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| r.get(0))
        .optional()
        .unwrap_or(None)
}

fn pet_meta_set(state: &CoreState, key: &str, value: &str) -> R<()> {
    let inner = state.inner.lock().unwrap();
    inner
        .conn
        .execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
            params![key, value],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(serde::Serialize)]
struct PetConfig {
    enabled: bool,
    has_image: bool,
    image_name: String,
}

#[tauri::command]
fn pet_get_config(state: State<'_, CoreState>) -> R<PetConfig> {
    let enabled = pet_meta_get(&state, "pet_enabled").as_deref() == Some("1");
    let image_name = pet_meta_get(&state, "pet_image").unwrap_or_default();
    let has_image = !image_name.is_empty()
        && {
            let inner = state.inner.lock().unwrap();
            inner.library_dir.join("pets").join(&image_name).exists()
        };
    Ok(PetConfig { enabled, has_image, image_name })
}

#[tauri::command]
fn pet_set_enabled(state: State<'_, CoreState>, enabled: bool) -> R<()> {
    pet_meta_set(&state, "pet_enabled", if enabled { "1" } else { "0" })
}

/// 桌面模式：按对话框返回的路径读图并导入（前端拿不到本地文件内容）。
#[tauri::command]
fn pet_set_image_path(state: State<'_, CoreState>, path: String) -> R<()> {
    let bytes = std::fs::read(&path).map_err(|e| format!("读取图片失败：{e}"))?;
    let ext = path.rsplit('.').next().unwrap_or("png").to_lowercase();
    pet_set_image(state, bytes, ext)
}

/// 导入形象图片：复制到 资料库/pets/current.<ext>（原文件不动）。
#[tauri::command]
fn pet_set_image(state: State<'_, CoreState>, bytes: Vec<u8>, ext: String) -> R<()> {
    let ext = ext.to_lowercase();
    let name = match ext.as_str() {
        "png" => "current.png".to_string(),
        "jpg" | "jpeg" => "current.jpg".to_string(),
        "webp" => "current.webp".to_string(),
        "gif" => "current.gif".to_string(),
        _ => "current.png".to_string(),
    };
    let inner = state.inner.lock().unwrap();
    let dir = inner.library_dir.join("pets");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(&name), bytes).map_err(|e| e.to_string())?;
    drop(inner);
    pet_meta_set(&state, "pet_image", &name)
}

#[tauri::command]
fn pet_get_image(state: State<'_, CoreState>) -> R<Option<Vec<u8>>> {
    let name = pet_meta_get(&state, "pet_image").unwrap_or_default();
    if name.is_empty() {
        return Ok(None);
    }
    let inner = state.inner.lock().unwrap();
    let path = inner.library_dir.join("pets").join(&name);
    drop(inner);
    match std::fs::read(path) {
        Ok(b) => Ok(Some(b)),
        Err(_) => Ok(None),
    }
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
            let lib = litreview_core::db::open_library(None)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
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
            save_text_file, read_text_file, commit_import,
            list_projects, create_project, rename_project, delete_project, list_tags,
            delete_tag, list_compares, create_compare, get_compare, update_compare,
            delete_compare, export_compare, backup, restore, get_settings, switch_library,
            get_template, list_annotations, add_annotation, update_annotation, delete_annotation,
            pet_get_config, pet_set_enabled, pet_set_image, pet_set_image_path, pet_get_image,
        ])
        .run(tauri::generate_context!())
        .expect("运行 Tauri 应用失败");
}
