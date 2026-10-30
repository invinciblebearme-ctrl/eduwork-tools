use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{BufReader, Read, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RenameOptions {
    pub find: String,
    pub replacement: String,
    pub prefix: String,
    pub suffix: String,
    pub add_date: bool,
    pub sequence_start: Option<u64>,
    pub extensions: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenamePlan {
    pub source: String,
    pub target: String,
    pub conflict: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub extension: String,
    pub modified: String,
    pub is_directory: bool,
}
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SearchOptions {
    pub name: String,
    pub extension: String,
    pub modified_after: Option<String>,
    pub modified_before: Option<String>,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub include_subfolders: bool,
}
fn date_stamp() -> String {
    Local::now().format("%Y%m%d").to_string()
}
fn renamed_name(path: &Path, o: &RenameOptions, i: usize, date: &str) -> Option<String> {
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    if !o.extensions.is_empty()
        && !o
            .extensions
            .iter()
            .any(|x| x.trim_start_matches('.').eq_ignore_ascii_case(ext))
    {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    let base = if o.find.is_empty() {
        stem.to_owned()
    } else {
        stem.replace(&o.find, &o.replacement)
    };
    let d = if o.add_date {
        format!("{date}_")
    } else {
        String::new()
    };
    let seq = o
        .sequence_start
        .map(|n| format!("{:03}_", n + i as u64))
        .unwrap_or_default();
    let stem = format!("{}{}{}{}{}", o.prefix, d, seq, base, o.suffix);
    Some(if ext.is_empty() {
        stem
    } else {
        format!("{stem}.{ext}")
    })
}
fn find_plan_conflicts(plans: &[(String, String)], existing: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    plans
        .iter()
        .filter_map(|(s, t)| {
            let k = t.to_lowercase();
            if existing.iter().any(|e| e.to_lowercase() == k) || !seen.insert(k) {
                Some(s.clone())
            } else {
                None
            }
        })
        .collect()
}
fn csv_escape(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_owned()
    }
}
fn listing_csv(entries: &[FileEntry]) -> String {
    let mut out = String::from("\u{feff}경로,파일명,크기(바이트),확장자,수정 시각\r\n");
    for e in entries {
        out.push_str(
            &[
                &e.path,
                &e.name,
                &e.size.to_string(),
                &e.extension,
                &e.modified,
            ]
            .iter()
            .map(|s| csv_escape(s))
            .collect::<Vec<_>>()
            .join(","),
        );
        out.push_str("\r\n")
    }
    out
}
fn metadata_entry(p: &Path) -> Result<FileEntry, String> {
    let m = fs::metadata(p).map_err(|e| format!("{}: {e}", p.display()))?;
    let dt: DateTime<Local> = m.modified().unwrap_or(SystemTime::UNIX_EPOCH).into();
    let name = p
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_owned();
    Ok(FileEntry {
        path: p.to_string_lossy().into_owned(),
        name,
        size: m.len(),
        extension: p
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_owned(),
        modified: dt.format("%Y-%m-%d %H:%M:%S").to_string(),
        is_directory: m.is_dir(),
    })
}
fn collect(root: &Path, recursive: bool, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(root).map_err(|e| format!("{}: {e}", root.display()))? {
        let p = entry.map_err(|e| e.to_string())?.path();
        let m = fs::symlink_metadata(&p).map_err(|e| e.to_string())?;
        if m.file_type().is_symlink() {
            continue;
        }
        if m.is_file() {
            out.push(p)
        } else if m.is_dir() && recursive {
            collect(&p, true, out)?
        }
    }
    Ok(())
}
fn export_xlsx_entries(entries: &[FileEntry], destination: &Path) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|e| format!("XLSX file creation failed: {e}"))?;
    let result = (|| {
        let mut workbook = rust_xlsxwriter::Workbook::new();
        let worksheet = workbook.add_worksheet();
        let headers = ["경로", "파일명", "크기(바이트)", "확장자", "수정 시각"];
        for (col, header) in headers.iter().enumerate() {
            worksheet
                .write_string(0, col as u16, *header)
                .map_err(|e| e.to_string())?;
        }
        for (row, entry) in entries.iter().enumerate() {
            let row = u32::try_from(row + 1).map_err(|e| e.to_string())?;
            worksheet
                .write_string(row, 0, &entry.path)
                .map_err(|e| e.to_string())?;
            worksheet
                .write_string(row, 1, &entry.name)
                .map_err(|e| e.to_string())?;
            worksheet
                .write_number(row, 2, entry.size as f64)
                .map_err(|e| e.to_string())?;
            worksheet
                .write_string(row, 3, &entry.extension)
                .map_err(|e| e.to_string())?;
            worksheet
                .write_string(row, 4, &entry.modified)
                .map_err(|e| e.to_string())?;
        }
        workbook
            .save_to_writer(&mut file)
            .map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(destination);
    }
    result
}
fn hash_file(p: &Path) -> Result<String, String> {
    let mut r = BufReader::new(File::open(p).map_err(|e| format!("{}: {e}", p.display()))?);
    let mut h = Sha256::new();
    let mut b = [0; 65536];
    loop {
        let n = r.read(&mut b).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        h.update(&b[..n])
    }
    Ok(format!("{:x}", h.finalize()))
}
#[tauri::command]
pub fn preview_rename(
    paths: Vec<String>,
    options: RenameOptions,
) -> Result<Vec<RenamePlan>, String> {
    let mut plans = Vec::new();
    for (i, s) in paths.iter().enumerate() {
        let p = Path::new(s);
        if !p.is_file() {
            continue;
        }
        if let Some(n) = renamed_name(p, &options, i, &date_stamp()) {
            let t = p.with_file_name(n);
            if t == p {
                continue;
            }
            plans.push(RenamePlan {
                source: s.clone(),
                target: t.to_string_lossy().into_owned(),
                conflict: t.exists(),
            })
        }
    }
    let all: Vec<_> = plans
        .iter()
        .map(|p| (p.source.clone(), p.target.clone()))
        .collect();
    for p in &mut plans {
        p.conflict |= find_plan_conflicts(&all, &[]).contains(&p.source)
    }
    Ok(plans)
}
#[tauri::command]
pub fn apply_rename(plans: Vec<RenamePlan>) -> Result<Vec<String>, String> {
    let target_pairs: Vec<_> = plans
        .iter()
        .map(|p| (p.source.clone(), p.target.clone()))
        .collect();
    if !find_plan_conflicts(&target_pairs, &[]).is_empty()
        || plans
            .iter()
            .any(|p| p.conflict || Path::new(&p.target).exists())
    {
        return Err("이름 충돌이 있습니다. 미리보기를 새로고침해 주세요.".into());
    }

    let mut sources = HashSet::new();
    for plan in &plans {
        let source = Path::new(&plan.source);
        let target = Path::new(&plan.target);
        let key = source.to_string_lossy().to_lowercase();
        if !sources.insert(key) {
            return Err("같은 원본 파일이 여러 번 포함됐습니다.".into());
        }
        let metadata = fs::symlink_metadata(source)
            .map_err(|e| format!("원본 파일 확인 실패 {}: {e}", plan.source))?;
        if !metadata.file_type().is_file() {
            return Err(format!(
                "일반 파일만 이름을 바꿀 수 있습니다: {}",
                plan.source
            ));
        }
        let source_parent = source.parent().unwrap_or_else(|| Path::new("."));
        let target_parent = target.parent().unwrap_or_else(|| Path::new("."));
        let source_parent = fs::canonicalize(source_parent).map_err(|e| e.to_string())?;
        let target_parent = fs::canonicalize(target_parent).map_err(|e| e.to_string())?;
        if source_parent != target_parent || target.file_name().is_none() {
            return Err("이름 변경 대상은 원본과 같은 폴더 안의 파일이어야 합니다.".into());
        }
    }

    let mut done: Vec<(PathBuf, PathBuf)> = Vec::new();
    for plan in plans {
        let source = PathBuf::from(&plan.source);
        let target = PathBuf::from(&plan.target);
        if let Err(error) = fs::rename(&source, &target) {
            let mut rollback_errors = Vec::new();
            for (original, renamed) in done.iter().rev() {
                if let Err(rollback_error) = fs::rename(renamed, original) {
                    rollback_errors.push(format!(
                        "{} → {}: {rollback_error}",
                        renamed.display(),
                        original.display()
                    ));
                }
            }
            let rollback = if rollback_errors.is_empty() {
                "이전 변경은 모두 되돌렸습니다.".to_owned()
            } else {
                format!("되돌리지 못한 파일: {}", rollback_errors.join("; "))
            };
            return Err(format!(
                "{} 이름 변경 실패: {error}. {rollback}",
                source.display()
            ));
        }
        done.push((source, target));
    }
    Ok(done
        .into_iter()
        .map(|(_, target)| target.display().to_string())
        .collect())
}
#[tauri::command]
pub fn scan_duplicates(root: String, include_subfolders: bool) -> Result<Vec<Vec<String>>, String> {
    let mut paths = Vec::new();
    collect(Path::new(&root), include_subfolders, &mut paths)?;
    let mut sizes: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for p in paths {
        sizes
            .entry(fs::metadata(&p).map_err(|e| e.to_string())?.len())
            .or_default()
            .push(p)
    }
    let mut hashes: HashMap<String, Vec<String>> = HashMap::new();
    for group in sizes.values().filter(|g| g.len() > 1) {
        for p in group {
            hashes
                .entry(hash_file(p)?)
                .or_default()
                .push(p.to_string_lossy().into_owned())
        }
    }
    Ok(hashes.into_values().filter(|g| g.len() > 1).collect())
}
#[tauri::command]
pub fn list_files(root: String, include_subfolders: bool) -> Result<Vec<FileEntry>, String> {
    let mut paths = Vec::new();
    collect(Path::new(&root), include_subfolders, &mut paths)?;
    paths.iter().map(|p| metadata_entry(p)).collect()
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizePlan {
    pub source: String,
    pub target: String,
}

fn organize_plan(root: &Path, output: &Path, recursive: bool) -> Result<Vec<OrganizePlan>, String> {
    let root = fs::canonicalize(root).map_err(|e| format!("원본 폴더 확인 실패: {e}"))?;
    let output = fs::canonicalize(output).map_err(|e| format!("출력 폴더 확인 실패: {e}"))?;
    if !root.is_dir() || !output.is_dir() {
        return Err("원본과 출력은 존재하는 폴더여야 합니다.".into());
    }
    if output.starts_with(&root) {
        return Err("출력 폴더는 원본 폴더 바깥에 선택해 주세요.".into());
    }
    let mut sources = Vec::new();
    collect(&root, recursive, &mut sources)?;
    let mut plans = Vec::new();
    for source in sources {
        let filename = source
            .file_name()
            .ok_or_else(|| "파일 이름을 읽을 수 없습니다.".to_owned())?;
        let extension = source.extension().and_then(|x| x.to_str()).unwrap_or("");
        let bucket = if extension.is_empty() {
            "확장자 없음".to_owned()
        } else {
            extension.to_ascii_lowercase()
        };
        plans.push(OrganizePlan {
            source: source.to_string_lossy().into_owned(),
            target: output
                .join(bucket)
                .join(filename)
                .to_string_lossy()
                .into_owned(),
        });
    }
    let targets = plans
        .iter()
        .map(|p| (p.source.clone(), p.target.clone()))
        .collect::<Vec<_>>();
    if !find_plan_conflicts(&targets, &[]).is_empty() {
        return Err("같은 확장자 폴더에 같은 파일명이 있어 정리할 수 없습니다.".into());
    }
    Ok(plans)
}

#[tauri::command]
pub fn preview_organize_files(
    root: String,
    output_directory: String,
    include_subfolders: bool,
) -> Result<Vec<OrganizePlan>, String> {
    organize_plan(
        Path::new(&root),
        Path::new(&output_directory),
        include_subfolders,
    )
}

#[tauri::command]
pub fn organize_files(
    root: String,
    output_directory: String,
    include_subfolders: bool,
) -> Result<Vec<String>, String> {
    let plans = organize_plan(
        Path::new(&root),
        Path::new(&output_directory),
        include_subfolders,
    )?;
    if plans.iter().any(|plan| Path::new(&plan.target).exists()) {
        return Err("출력 파일이 이미 있습니다. 기존 파일은 덮어쓰지 않았습니다.".into());
    }
    let mut copied = Vec::<PathBuf>::new();
    for plan in plans {
        let target = PathBuf::from(&plan.target);
        let parent = target
            .parent()
            .ok_or_else(|| "출력 폴더를 확인할 수 없습니다.".to_owned())?;
        let mut created = false;
        let result = (|| {
            fs::create_dir_all(parent).map_err(|e| format!("출력 폴더 생성 실패: {e}"))?;
            let mut source = File::open(&plan.source).map_err(|e| e.to_string())?;
            let mut destination = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
                .map_err(|e| e.to_string())?;
            created = true;
            std::io::copy(&mut source, &mut destination).map_err(|e| e.to_string())?;
            destination.flush().map_err(|e| e.to_string())
        })();
        if let Err(error) = result {
            if created {
                let _ = fs::remove_file(&target);
            }
            let mut cleanup_errors = Vec::new();
            for path in copied.iter().rev() {
                if let Err(cleanup_error) = fs::remove_file(path) {
                    cleanup_errors.push(format!("{}: {cleanup_error}", path.display()));
                }
            }
            if cleanup_errors.is_empty() {
                return Err(format!(
                    "파일 복사 실패: {error}. 이번 실행에서 복사한 파일은 정리했습니다."
                ));
            }
            return Err(format!(
                "파일 복사 실패: {error}. 일부 출력 정리 실패: {}",
                cleanup_errors.join("; ")
            ));
        }
        copied.push(target);
    }
    Ok(copied
        .into_iter()
        .map(|path| path.display().to_string())
        .collect())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VocabularyAudit {
    pub image_count: usize,
    pub matched_words: Vec<String>,
    pub missing_words: Vec<String>,
    pub unmatched_files: Vec<String>,
    pub duplicate_groups: Vec<Vec<String>>,
}

#[tauri::command]
pub fn audit_vocabulary_images(
    root: String,
    words: Vec<String>,
) -> Result<VocabularyAudit, String> {
    let root = Path::new(&root);
    if !root.is_dir() {
        return Err("이미지 폴더를 선택해 주세요.".into());
    }
    if words.len() > 100_000 {
        return Err("단어는 최대 100,000개까지 입력할 수 있습니다.".into());
    }
    fn normalize(value: &str) -> String {
        value
            .chars()
            .filter(|c| !c.is_whitespace())
            .flat_map(char::to_lowercase)
            .collect()
    }
    let mut expected = HashMap::<String, String>::new();
    for word in words {
        let display = word.trim().to_owned();
        let key = normalize(&display);
        if !key.is_empty() {
            expected.entry(key).or_insert(display);
        }
    }
    let mut paths = Vec::new();
    collect(root, true, &mut paths)?;
    let image_extensions = ["png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff"];
    let mut assets = HashMap::<String, Vec<String>>::new();
    for path in paths {
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if image_extensions
            .iter()
            .any(|e| e.eq_ignore_ascii_case(extension))
        {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            assets
                .entry(normalize(stem))
                .or_default()
                .push(path.to_string_lossy().into_owned());
        }
    }
    let mut matched_words = Vec::new();
    let mut missing_words = Vec::new();
    for (key, display) in &expected {
        if assets.contains_key(key) {
            matched_words.push(display.clone());
        } else {
            missing_words.push(display.clone());
        }
    }
    matched_words.sort_by_key(|s| normalize(s));
    missing_words.sort_by_key(|s| normalize(s));
    let mut unmatched_files = Vec::new();
    let mut duplicate_groups = Vec::new();
    for (key, mut files) in assets {
        files.sort();
        if !expected.contains_key(&key) {
            unmatched_files.extend(files.iter().cloned());
        }
        if files.len() > 1 {
            duplicate_groups.push(files);
        }
    }
    unmatched_files.sort();
    duplicate_groups.sort_by(|a, b| a.first().cmp(&b.first()));
    let image_count = assets_count(root)?;
    Ok(VocabularyAudit {
        image_count,
        matched_words,
        missing_words,
        unmatched_files,
        duplicate_groups,
    })
}
fn assets_count(root: &Path) -> Result<usize, String> {
    let mut paths = Vec::new();
    collect(root, true, &mut paths)?;
    Ok(paths
        .iter()
        .filter(|path| {
            let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            ["png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff"]
                .iter()
                .any(|e| e.eq_ignore_ascii_case(extension))
        })
        .count())
}
#[tauri::command]
pub fn export_csv(
    root: String,
    include_subfolders: bool,
    destination: String,
) -> Result<usize, String> {
    let e = list_files(root, include_subfolders)?;
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|e| format!("CSV file creation failed: {e}"))?;
    f.write_all(listing_csv(&e).as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(e.len())
}
#[tauri::command]
pub fn export_xlsx(
    root: String,
    include_subfolders: bool,
    destination: String,
) -> Result<usize, String> {
    let entries = list_files(root, include_subfolders)?;
    export_xlsx_entries(&entries, Path::new(&destination))?;
    Ok(entries.len())
}
#[tauri::command]
pub fn search_files(root: String, options: SearchOptions) -> Result<Vec<FileEntry>, String> {
    let mut paths = Vec::new();
    collect(Path::new(&root), options.include_subfolders, &mut paths)?;
    let entries = paths
        .iter()
        .map(|p| metadata_entry(p))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(entries
        .into_iter()
        .filter(|e| {
            let n = options.name.to_lowercase();
            let x = options.extension.trim_start_matches('.').to_lowercase();
            let after = options
                .modified_after
                .as_ref()
                .map(|d| &e.modified[..10] >= d.as_str())
                .unwrap_or(true);
            let before = options
                .modified_before
                .as_ref()
                .map(|d| &e.modified[..10] <= d.as_str())
                .unwrap_or(true);
            e.name.to_lowercase().contains(&n)
                && (x.is_empty() || e.extension.to_lowercase() == x)
                && after
                && before
                && options.min_size.map(|v| e.size >= v).unwrap_or(true)
                && options.max_size.map(|v| e.size <= v).unwrap_or(true)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn temp() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "eduwork-{}-{}",
            std::process::id(),
            Local::now().timestamp_nanos_opt().unwrap()
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn organize_files_copies_by_extension_preserves_sources_and_refuses_overwrite() {
        let base = temp();
        let root = base.join("source");
        let out = base.join("organized");
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::create_dir_all(&out).unwrap();
        fs::write(root.join("nested/report.PDF"), b"pdf").unwrap();
        fs::write(root.join("photo.png"), b"png").unwrap();

        let plans =
            preview_organize_files(root.display().to_string(), out.display().to_string(), true)
                .unwrap();
        assert_eq!(plans.len(), 2);
        assert!(plans
            .iter()
            .any(|p| Path::new(&p.target).ends_with(Path::new("pdf").join("report.PDF"))));
        let copied =
            organize_files(root.display().to_string(), out.display().to_string(), true).unwrap();
        assert_eq!(copied.len(), 2);
        assert_eq!(fs::read(root.join("nested/report.PDF")).unwrap(), b"pdf");
        assert_eq!(fs::read(out.join("pdf/report.PDF")).unwrap(), b"pdf");

        fs::write(out.join("pdf/report.PDF"), b"keep-existing").unwrap();
        assert!(
            organize_files(root.display().to_string(), out.display().to_string(), true).is_err()
        );
        assert_eq!(
            fs::read(out.join("pdf/report.PDF")).unwrap(),
            b"keep-existing"
        );
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn vocabulary_audit_reports_missing_unmatched_and_duplicate_assets() {
        let root = temp();
        fs::write(root.join("apple.png"), b"a").unwrap();
        fs::write(root.join("Apple.webp"), b"b").unwrap();
        fs::write(root.join("pear.jpg"), b"c").unwrap();

        let report = audit_vocabulary_images(
            root.display().to_string(),
            vec!["apple".into(), "banana".into()],
        )
        .unwrap();

        assert_eq!(report.missing_words, vec!["banana"]);
        assert_eq!(
            report.unmatched_files,
            vec![root.join("pear.jpg").display().to_string()]
        );
        assert_eq!(report.duplicate_groups.len(), 1);
        assert_eq!(report.duplicate_groups[0].len(), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rename_preflights_entire_plan_before_mutating_any_file() {
        let root = temp();
        let first = root.join("first.txt");
        let first_target = root.join("renamed-first.txt");
        fs::write(&first, "keep").unwrap();
        let plans = vec![
            RenamePlan {
                source: first.display().to_string(),
                target: first_target.display().to_string(),
                conflict: false,
            },
            RenamePlan {
                source: root.join("missing.txt").display().to_string(),
                target: root.join("renamed-missing.txt").display().to_string(),
                conflict: false,
            },
        ];

        assert!(apply_rename(plans).is_err());
        assert_eq!(fs::read_to_string(&first).unwrap(), "keep");
        assert!(!first_target.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rename_combines_replacement_prefix_suffix_and_sequence() {
        let o = RenameOptions {
            find: "draft".into(),
            replacement: "final".into(),
            prefix: "2026_".into(),
            suffix: "_ok".into(),
            add_date: true,
            sequence_start: Some(3),
            extensions: vec!["txt".into()],
        };
        assert_eq!(
            renamed_name(Path::new("draft.txt"), &o, 0, "20261028").unwrap(),
            "2026_20261028_003_final_ok.txt"
        );
    }
    #[test]
    fn rename_extensionless_file_has_no_trailing_dot() {
        let o = RenameOptions {
            prefix: "new_".into(),
            ..Default::default()
        };
        assert_eq!(
            renamed_name(Path::new("notes"), &o, 0, "20261028").unwrap(),
            "new_notes"
        );
    }
    #[test]
    fn rename_filters_and_finds_collisions() {
        let o = RenameOptions {
            prefix: "x".into(),
            extensions: vec!["txt".into()],
            ..Default::default()
        };
        assert!(renamed_name(Path::new("a.pdf"), &o, 0, "20261028").is_none());
        let p = vec![("a".into(), "x".into()), ("b".into(), "x".into())];
        assert_eq!(find_plan_conflicts(&p, &[]).len(), 1);
    }
    #[test]
    fn duplicate_targets_are_rejected_without_changing_sources() {
        let dir = temp();
        let a = dir.join("a.txt");
        let b = dir.join("b.txt");
        fs::write(&a, b"source a").unwrap();
        fs::write(&b, b"source b").unwrap();
        let plans = vec![
            RenamePlan {
                source: a.to_string_lossy().into(),
                target: dir.join("same.txt").to_string_lossy().into(),
                conflict: false,
            },
            RenamePlan {
                source: b.to_string_lossy().into(),
                target: dir.join("same.txt").to_string_lossy().into(),
                conflict: false,
            },
        ];
        assert!(apply_rename(plans).is_err());
        assert_eq!(fs::read(&a).unwrap(), b"source a");
        assert_eq!(fs::read(&b).unwrap(), b"source b");
        assert!(!dir.join("same.txt").exists());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn recursive_collection_skips_symlinks() {
        let dir = temp();
        let sub = dir.join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("inside.txt"), b"ok").unwrap();
        #[cfg(unix)]
        let link_result = std::os::unix::fs::symlink(&dir, sub.join("loop"));
        #[cfg(windows)]
        let link_result = std::os::windows::fs::symlink_dir(&dir, sub.join("loop"));
        if link_result.is_ok() {
            let mut paths = Vec::new();
            collect(&dir, true, &mut paths).unwrap();
            assert_eq!(paths, vec![sub.join("inside.txt")]);
        }
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn xlsx_export_is_a_zip_with_korean_headers_and_data() {
        let dir = temp();
        let dest = dir.join("files.xlsx");
        let entry = FileEntry {
            path: "C:/교육/자료.txt".into(),
            name: "자료.txt".into(),
            size: 42,
            extension: "txt".into(),
            modified: "2026-10-29 12:00:00".into(),
            is_directory: false,
        };
        export_xlsx_entries(&[entry], &dest).unwrap();
        let bytes = fs::read(&dest).unwrap();
        assert_eq!(&bytes[..2], b"PK");
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut sheet = String::new();
        archive
            .by_name("xl/sharedStrings.xml")
            .unwrap()
            .read_to_string(&mut sheet)
            .unwrap();
        for expected in [
            "경로",
            "파일명",
            "크기(바이트)",
            "확장자",
            "수정 시각",
            "C:/교육/자료.txt",
            "자료.txt",
        ] {
            assert!(sheet.contains(expected), "missing {expected}: {sheet}");
        }
        assert!(export_xlsx_entries(&[], &dest).is_err());
        assert_eq!(&fs::read(&dest).unwrap()[..2], b"PK");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn csv_is_bom_and_quotes_fields() {
        let e = FileEntry {
            path: "C:/한글,a.txt".into(),
            name: "a, b.txt".into(),
            size: 2,
            extension: "txt".into(),
            modified: "2026-01-01".into(),
            is_directory: false,
        };
        let s = listing_csv(&[e]);
        assert!(s.starts_with("\u{feff}경로,파일명"));
        assert!(s.contains("\"a, b.txt\""));
    }
    #[test]
    fn actual_file_operations_rename_scan_list_search_and_export() {
        let dir = temp();
        let a = dir.join("draft.txt");
        let b = dir.join("same.txt");
        let c = dir.join("sub");
        fs::create_dir_all(&c).unwrap();
        let d = c.join("copy.txt");
        fs::write(&a, b"identical").unwrap();
        fs::write(&b, b"identical").unwrap();
        fs::write(&d, b"identical").unwrap();
        let busy = dir.join("busy.txt");
        let occupied = dir.join("final_busy.txt");
        fs::write(&busy, b"keep source").unwrap();
        fs::write(&occupied, b"keep existing target").unwrap();
        let all = list_files(dir.to_string_lossy().into(), true).unwrap();
        assert_eq!(all.len(), 5);
        assert_eq!(
            scan_duplicates(dir.to_string_lossy().into(), true).unwrap()[0].len(),
            3
        );
        let found = search_files(
            dir.to_string_lossy().into(),
            SearchOptions {
                name: "draft".into(),
                extension: "txt".into(),
                include_subfolders: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(found.len(), 1);
        let conflict = preview_rename(
            vec![busy.to_string_lossy().into()],
            RenameOptions {
                prefix: "final_".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(conflict[0].conflict);
        assert!(apply_rename(conflict).is_err());
        assert_eq!(fs::read(&occupied).unwrap(), b"keep existing target");
        assert_eq!(fs::read(&busy).unwrap(), b"keep source");
        let plans = preview_rename(
            vec![a.to_string_lossy().into()],
            RenameOptions {
                prefix: "final_".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!plans[0].conflict);
        let renamed = apply_rename(plans).unwrap();
        assert!(Path::new(&renamed[0]).exists());
        assert!(!a.exists());
        let csv = dir.join("list.csv");
        assert_eq!(
            export_csv(
                dir.to_string_lossy().into(),
                true,
                csv.to_string_lossy().into()
            )
            .unwrap(),
            5
        );
        assert!(fs::read(csv).unwrap().starts_with(&[0xef, 0xbb, 0xbf]));
        fs::remove_dir_all(dir).unwrap();
    }
}
