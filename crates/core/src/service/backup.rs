//! 完整备份与恢复：SQLite 快照（VACUUM INTO）+ 托管 PDF 目录打包为 ZIP；
//! 恢复时先校验再切换资料库目录。

use crate::db::{save_settings, CoreState};
use crate::error::{CoreError, CoreResult};
use crate::model::{BackupResult, RestoreResult};
use rusqlite::{params, Connection};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

fn timestamp_name() -> String {
    let ts = crate::db::now_ts();
    let days = ts / 86400;
    let rem = ts % 86400;
    let (y, mo, d) = civil_from_days(days);
    format!(
        "LiteratureLibrary-backup-{y:04}{mo:02}{d:02}-{:02}{:02}{:02}.zip",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
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

/// 备份当前资料库到用户选择的目标目录。备份期间持写锁，保证库与文件一致。
pub fn backup_to(state: &CoreState, target_dir: &Path) -> CoreResult<BackupResult> {
    let target_dir = target_dir.to_path_buf();
    fs::create_dir_all(&target_dir)?;
    let zip_path = target_dir.join(timestamp_name());
    let snap_path = target_dir.join(".library-snapshot.sqlite");

    let inner = state.inner.lock().unwrap();
    // 快照数据库（VACUUM INTO 产出独立一致的副本）
    if snap_path.exists() {
        fs::remove_file(&snap_path)?;
    }
    inner
        .conn
        .execute("VACUUM INTO ?1", params![snap_path.to_string_lossy()])?;
    let papers: i64 = inner
        .conn
        .query_row("SELECT COUNT(*) FROM papers", [], |r| r.get(0))?;

    let file = fs::File::create(&zip_path)?;
    let mut zw = zip::ZipWriter::new(file);
    let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    zw.start_file("library.sqlite", opts)?;
    let mut snap = fs::File::open(&snap_path)?;
    let mut buf = Vec::new();
    snap.read_to_end(&mut buf)?;
    zw.write_all(&buf)?;

    let papers_dir = inner.library_dir.join("papers");
    if papers_dir.exists() {
        for entry in walk_files(&papers_dir)? {
            let rel = entry.strip_prefix(&papers_dir).unwrap_or(&entry);
            let name = Path::new("papers").join(rel);
            zw.start_file(name.to_string_lossy().replace('\\', "/"), opts)?;
            let data = fs::read(&entry)?;
            zw.write_all(&data)?;
        }
    }
    zw.finish()?;
    drop(inner);

    let _ = fs::remove_file(&snap_path);
    let bytes = fs::metadata(&zip_path)?.len();
    Ok(BackupResult {
        zip_path: zip_path.to_string_lossy().to_string(),
        papers,
        bytes,
    })
}

fn walk_files(dir: &Path) -> CoreResult<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk_files(&path)?);
        } else {
            out.push(path);
        }
    }
    Ok(out)
}

/// 从备份 ZIP 恢复到目标目录（可为新资料库目录）。校验通过后才切换当前库。
pub fn restore_from(state: &CoreState, zip_path: &Path, target_dir: &Path) -> CoreResult<RestoreResult> {
    let target_dir = target_dir.to_path_buf();
    if target_dir.exists() && fs::read_dir(&target_dir)?.next().is_some() {
        return Err(CoreError::Backup(format!(
            "目标目录 {} 不为空，请选择空目录或新建目录",
            target_dir.display()
        )));
    }

    let staging = crate::db::app_data_dir().join(format!("tmp-restore-{}", uuid::Uuid::new_v4().simple()));
    fs::create_dir_all(&staging)?;
    let result = (|| -> CoreResult<RestoreResult> {
        let file = fs::File::open(zip_path)?;
        let mut zr = zip::ZipArchive::new(file)?;
        let names: Vec<String> = zr.file_names().map(|s| s.to_string()).collect();
        // 允许备份内容整体嵌在 LiteratureLibrary/ 前缀下
        let prefix = if names.iter().all(|n| n.starts_with("LiteratureLibrary/")) {
            "LiteratureLibrary/"
        } else {
            ""
        };
        let mut has_db = false;
        for i in 0..zr.len() {
            let mut entry = zr.by_index(i)?;
            let name = entry.name().to_string();
            if entry.is_dir() {
                continue;
            }
            let rel = name.strip_prefix(prefix).unwrap_or(&name).to_string();
            if rel == "library.sqlite" {
                has_db = true;
            }
            let dest = staging.join(&rel);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut data = Vec::new();
            entry.read_to_end(&mut data)?;
            // 路径穿越防护
            if !dest.starts_with(&staging) {
                return Err(CoreError::Backup(format!("备份内含非法路径：{rel}")));
            }
            fs::write(&dest, &data)?;
        }
        if !has_db {
            return Err(CoreError::Backup("备份缺少 library.sqlite，不是完整备份".into()));
        }

        // 校验：数据库完整性 + 文献数量 + 关联 PDF 存在性
        let conn = Connection::open(staging.join("library.sqlite"))?;
        let integrity: String =
            conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        if integrity != "ok" {
            return Err(CoreError::Backup(format!("数据库完整性校验失败：{integrity}")));
        }
        let papers: i64 = conn.query_row("SELECT COUNT(*) FROM papers", [], |r| r.get(0))?;
        let mut stmt = conn.prepare("SELECT pdf_rel_path FROM papers")?;
        let rels: Vec<String> = stmt
            .query_map([], |r| r.get(0))?
            .flatten()
            .collect();
        drop(stmt);
        drop(conn);
        let mut warnings = Vec::new();
        let mut missing = 0usize;
        for rel in &rels {
            if rel.is_empty() {
                continue;
            }
            if !staging.join(rel).exists() {
                missing += 1;
            }
        }
        if missing > 0 {
            warnings.push(format!("{missing} 篇文献的 PDF 文件在备份中缺失"));
        }
        if papers == 0 && missing == 0 && rels.is_empty() {
            warnings.push("备份中没有任何文献".into());
        }

        // 校验通过：移动到目标目录并切换
        fs::create_dir_all(&target_dir)?;
        for entry in fs::read_dir(&staging)? {
            let entry = entry?;
            let dest = target_dir.join(entry.file_name());
            fs::rename(entry.path(), &dest)
                .or_else(|_| copy_dir_recursive(&entry.path(), &dest))?;
        }
        let _ = fs::remove_dir_all(&staging);

        switch_library(state, &target_dir)?;
        Ok(RestoreResult {
            library_dir: target_dir.to_string_lossy().to_string(),
            papers,
            warnings,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_dir_recursive(&entry.path(), &dst.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(src, dst)?;
        Ok(())
    }
}

/// 切换当前资料库到新目录（恢复/设置页共用）。
pub fn switch_library(state: &CoreState, new_dir: &Path) -> CoreResult<()> {
    save_settings(new_dir)?;
    let conn = Connection::open(new_dir.join("library.sqlite"))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    crate::db::migrate(&conn)?;
    let mut inner = state.inner.lock().unwrap();
    inner.library_dir = new_dir.to_path_buf();
    let old = std::mem::replace(&mut inner.conn, conn);
    drop(old);
    Ok(())
}
