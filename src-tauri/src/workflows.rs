use crate::{excel, images, pdf};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    sync::Mutex,
};
use tauri::Manager;
static LOCK: Mutex<()> = Mutex::new(());
const MAX_STORE_BYTES: u64 = 8 * 1024 * 1024;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Template {
    pub name: String,
    pub kind: String,
    pub format: String,
    pub width: Option<u32>,
    pub group_column: Option<usize>,
}
impl Template {
    fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() || self.name.len() > 200 {
            return Err("템플릿 이름은 1~200바이트로 입력하세요.".into());
        }
        if !["images", "pdf", "excel"].contains(&self.kind.as_str()) {
            return Err("준비 중인 작업입니다.".into());
        }
        if !["png", "jpeg", "webp"].contains(&self.format.as_str())
            || self.width.is_some_and(|w| w == 0 || w > 30000)
        {
            return Err("이미지 형식 또는 너비가 올바르지 않습니다.".into());
        }
        if self.group_column.is_some_and(|c| c == 0 || c > 16384) {
            return Err("그룹 열은 1~16384 범위입니다.".into());
        }
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    template: Template,
    input: String,
    destination: String,
    started_at: String,
    finished_at: String,
    file_count: usize,
    outputs: Vec<String>,
    success: bool,
    errors: Vec<String>,
    skipped_links: usize,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Store {
    templates: Vec<Template>,
    history: Vec<Run>,
}
impl Store {
    fn load(dir: &Path) -> Result<Self, String> {
        let path = dir.join("workflows.json");
        match fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e.to_string()),
            Ok(m) if m.file_type().is_symlink() || !m.is_file() || m.len() > MAX_STORE_BYTES => {
                return Err("워크플로 저장 파일의 종류/크기 제한 오류".into())
            }
            _ => {}
        }
        let mut bytes = Vec::new();
        fs::File::open(path)
            .map_err(|e| e.to_string())?
            .take(MAX_STORE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_STORE_BYTES {
            return Err("워크플로 저장 용량 8 MiB 초과".into());
        }
        let store: Self = serde_json::from_slice(&bytes)
            .map_err(|e| format!("워크플로 JSON 손상: {e}. 원본을 보존했습니다."))?;
        if store.templates.len() > 100 || store.history.len() > 200 {
            return Err("저장 개수 제한 오류".into());
        }
        for t in &store.templates {
            t.validate()?;
        }
        Ok(store)
    }
    fn save(&self, dir: &Path) -> Result<(), String> {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_STORE_BYTES {
            return Err("워크플로 저장 용량 8 MiB 초과".into());
        }
        let mut tmp = tempfile::NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
        tmp.write_all(&bytes).map_err(|e| e.to_string())?;
        tmp.as_file().sync_all().map_err(|e| e.to_string())?;
        let destination = dir.join("workflows.json");
        let mut attempts = 0;
        loop {
            match tmp.persist(&destination) {
                Ok(_) => break,
                Err(error)
                    if error.error.kind() == std::io::ErrorKind::PermissionDenied
                        && attempts < 5 =>
                {
                    tmp = error.file;
                    attempts += 1;
                    std::thread::sleep(std::time::Duration::from_millis(10 << attempts));
                }
                Err(error) => return Err(error.to_string()),
            }
        }
        Ok(())
    }
}
fn execute(t: &Template, input: &Path, destination: &Path, run: &mut Run) -> Result<(), String> {
    t.validate()?;
    if !input.is_absolute() || !destination.is_absolute() {
        return Err("절대 경로를 선택하세요.".into());
    }
    for path in input.ancestors().chain(destination.ancestors()) {
        match fs::symlink_metadata(path) {
            Ok(m) if m.file_type().is_symlink() => {
                return Err("심볼릭 링크 경로는 지원하지 않습니다.".into())
            }
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
            _ => {}
        }
    }
    if fs::symlink_metadata(input)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("심볼릭 링크 폴더는 지원하지 않습니다.".into());
    }
    if !input.is_dir() {
        return Err("입력은 폴더여야 합니다.".into());
    }
    if t.kind == "images" {
        if !destination.is_dir() {
            return Err("이미지 출력은 기존 폴더여야 합니다.".into());
        }
    } else {
        let extension = if t.kind == "pdf" { "pdf" } else { "xlsx" };
        if destination
            .extension()
            .and_then(|e| e.to_str())
            .is_none_or(|e| !e.eq_ignore_ascii_case(extension))
            || destination.exists()
            || !destination.parent().is_some_and(Path::is_dir)
        {
            return Err(format!(
                "출력은 기존 폴더 안의 새 .{extension} 파일이어야 합니다."
            ));
        }
    }
    let extensions: &[&str] = match t.kind.as_str() {
        "images" => &["jpg", "jpeg", "png", "webp"],
        "pdf" => &["pdf"],
        _ => &["xlsx"],
    };
    let mut paths = Vec::new();
    let mut bytes = 0u64;
    let mut entries = 0;
    for item in fs::read_dir(input).map_err(|e| e.to_string())? {
        entries += 1;
        if entries > 10000 {
            return Err("폴더 항목 10,000개 제한 초과".into());
        }
        let item = item.map_err(|e| e.to_string())?;
        let meta = fs::symlink_metadata(item.path()).map_err(|e| e.to_string())?;
        if meta.file_type().is_symlink() {
            run.skipped_links += 1;
            continue;
        }
        if !meta.is_file() {
            continue;
        }
        let p = item.path();
        let ext = p
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase();
        if !extensions.contains(&ext.as_str()) {
            continue;
        }
        run.file_count += 1;
        bytes = bytes.saturating_add(meta.len());
        if run.file_count > 50 || meta.len() > 100 * 1024 * 1024 || bytes > 250 * 1024 * 1024 {
            return Err("최대 50파일, 파일당 100 MiB, 합계 250 MiB 제한 초과".into());
        }
        paths.push(p);
    }
    paths.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    if paths.is_empty() {
        return Err("지원되는 입력 파일이 없습니다.".into());
    }
    match t.kind.as_str() {
        "images" => {
            let format = match t.format.as_str() {
                "jpeg" => images::OutputFormat::Jpeg,
                "webp" => images::OutputFormat::Webp,
                _ => images::OutputFormat::Png,
            };
            let results = images::convert_batch(
                paths.iter().map(|p| p.display().to_string()).collect(),
                destination.display().to_string(),
                images::ImageOptions {
                    format,
                    width: t.width,
                    height: None,
                    exact: false,
                    square_canvas: false,
                    quality: 85,
                },
            )?;
            for r in results {
                if let Some(p) = r.output {
                    run.outputs.push(p);
                }
                if let Some(e) = r.error {
                    run.errors.push(format!("{}: {e}", r.source));
                }
            }
        }
        "pdf" => {
            pdf::merge_pdfs(&paths, destination)?;
            run.outputs.push(destination.display().to_string());
        }
        _ => {
            if !destination
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("xlsx"))
            {
                return Err("출력 확장자는 .xlsx여야 합니다.".into());
            }
            let stage = tempfile::tempdir().map_err(|e| e.to_string())?;
            let merged = stage.path().join("merged.xlsx");
            excel::merge_workbooks(&paths, &merged, true, None)?;
            let sheets = excel::inspect(&merged)?;
            if sheets.len() != 1 {
                return Err(
                    "자동 명단 정리는 단일 시트 XLSX만 지원합니다. 다중 시트는 준비 중입니다."
                        .into(),
                );
            }
            let cleaned = stage.path().join("cleaned.xlsx");
            excel::clean_sheet(
                &merged,
                &sheets[0].0,
                &cleaned,
                excel::CleanOptions {
                    remove_duplicates: true,
                    remove_blank_rows: true,
                    trim_text: true,
                    ..Default::default()
                },
            )?;
            let rows = excel::read_sheet(&cleaned, &sheets[0].0)?;
            let mut outputs = vec![(destination.to_path_buf(), rows.clone())];
            if let Some(col) = t.group_column {
                if rows.first().is_none_or(|r| col > r.len()) {
                    return Err("그룹 열이 머리글 범위를 벗어났습니다.".into());
                }
                let mut groups = std::collections::BTreeMap::<String, Vec<Vec<String>>>::new();
                for r in rows.iter().skip(1) {
                    groups
                        .entry(r.get(col - 1).cloned().unwrap_or_default())
                        .or_default()
                        .push(r.clone());
                }
                if groups.len() > 100 {
                    return Err("그룹은 최대 100개입니다.".into());
                }
                for (i, (_, data)) in groups.into_iter().enumerate() {
                    let path = destination.with_file_name(format!(
                        "{}_group-{:03}.xlsx",
                        destination
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy(),
                        i + 1
                    ));
                    let mut all = vec![rows[0].clone()];
                    all.extend(data);
                    outputs.push((path, all));
                }
            }
            excel::preflight_new_paths(
                &outputs.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>(),
            )?;
            for (p, rows) in outputs {
                excel::write_single(&p, &sheets[0].0, &rows)?;
                run.outputs.push(p.display().to_string());
            }
        }
    }
    Ok(())
}
fn run_saved(dir: &Path, t: Template, input: &Path, destination: &Path) -> Result<Run, String> {
    run_saved_with(dir, t, input, destination, execute)
}
fn run_saved_with(
    dir: &Path,
    t: Template,
    input: &Path,
    destination: &Path,
    execute: impl FnOnce(&Template, &Path, &Path, &mut Run) -> Result<(), String>,
) -> Result<Run, String> {
    let mut store = Store::load(dir)?;
    let mut run = Run {
        template: t.clone(),
        input: input.display().to_string(),
        destination: destination.display().to_string(),
        started_at: chrono::Utc::now().to_rfc3339(),
        finished_at: String::new(),
        file_count: 0,
        outputs: vec![],
        success: false,
        errors: vec![],
        skipped_links: 0,
    };
    if store.history.len() >= 200 {
        store.history.remove(0);
    }
    store.history.push(run.clone());
    store.save(dir)?;
    if let Err(e) = execute(&t, input, destination, &mut run) {
        run.errors.push(e);
    }
    run.success = run.errors.is_empty();
    run.finished_at = chrono::Utc::now().to_rfc3339();
    *store.history.last_mut().unwrap() = run.clone();
    store.save(dir).map_err(|e| {
        format!(
            "실행 후 기록 저장 실패: {e}; 완료된 출력: {}",
            run.outputs.join("\n")
        )
    })?;
    Ok(run)
}
#[tauri::command]
pub fn workflow_load(app: tauri::AppHandle) -> Result<Store, String> {
    let _guard = LOCK.lock().map_err(|e| e.to_string())?;
    Store::load(&app.path().app_local_data_dir().map_err(|e| e.to_string())?)
}
#[tauri::command]
pub fn workflow_save(
    app: tauri::AppHandle,
    template: Template,
    delete: bool,
) -> Result<Store, String> {
    let _guard = LOCK.lock().map_err(|e| e.to_string())?;
    save_template(
        &app.path().app_local_data_dir().map_err(|e| e.to_string())?,
        template,
        delete,
    )
}
fn save_template(dir: &Path, template: Template, delete: bool) -> Result<Store, String> {
    template.validate()?;
    let mut s = Store::load(dir)?;
    s.templates.retain(|t| t.name != template.name);
    if !delete {
        if s.templates.len() >= 100 {
            return Err("템플릿 최대 100개".into());
        }
        s.templates.push(template);
    }
    s.save(dir)?;
    Ok(s)
}
#[tauri::command]
pub async fn workflow_run(
    app: tauri::AppHandle,
    template: Template,
    input: String,
    destination: String,
) -> Result<Run, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = LOCK
            .try_lock()
            .map_err(|_| "다른 자동화 작업이 실행 중입니다.".to_string())?;
        run_saved(
            &app.path().app_local_data_dir().map_err(|e| e.to_string())?,
            template,
            Path::new(&input),
            Path::new(&destination),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}
#[cfg(test)]
#[path = "workflows_tests.rs"]
mod tests;
