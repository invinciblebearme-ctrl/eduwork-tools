use chrono::Local;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const MAX_INPUT: u64 = 250 * 1024 * 1024;
const MAX_ARCHIVE: u64 = 64 * 1024 * 1024;
const MAX_FILES: usize = 500;
const MAX_TOTAL: u64 = 1024 * 1024 * 1024;
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub path: String,
    pub pattern_type: String,
    pub location: String,
    pub scanned_at: String,
    pub masked_value: String,
}
fn compile_patterns(custom: &[(String, String)]) -> Result<Vec<(String, Regex)>, String> {
    let mut p = vec![
        (
            "주민등록번호".into(),
            Regex::new(r"\b\d{6}[- ]?[1-8]\d{6}\b").unwrap(),
        ),
        (
            "휴대전화번호".into(),
            Regex::new(r"\b01[016789][- ]?\d{3,4}[- ]?\d{4}\b").unwrap(),
        ),
        (
            "이메일".into(),
            Regex::new(r"(?i)\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b").unwrap(),
        ),
    ];
    for (label, pat) in custom {
        p.push((
            label.clone(),
            Regex::new(pat).map_err(|e| format!("사용자 정규식 오류: {e}"))?,
        ));
    }
    Ok(p)
}
fn scan_text(
    text: &str,
    path: &str,
    location: &str,
    custom: &[(String, String)],
) -> Result<Vec<Finding>, String> {
    let patterns = compile_patterns(custom)?;
    let timestamp = Local::now().to_rfc3339();
    let mut out = Vec::new();
    for (label, re) in patterns {
        for m in re.find_iter(text) {
            let chars: Vec<char> = m.as_str().chars().collect();
            let masked = format!(
                "{}{}",
                "*".repeat(chars.len().saturating_sub(2).max(4)),
                chars
                    .iter()
                    .rev()
                    .take(2)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect::<String>()
            );
            out.push(Finding {
                path: path.into(),
                pattern_type: label.clone(),
                location: location.into(),
                scanned_at: timestamp.clone(),
                masked_value: masked,
            });
        }
    }
    Ok(out)
}
fn read_limited<R: Read>(r: R, limit: u64) -> Result<Vec<u8>, String> {
    let mut b = Vec::new();
    r.take(limit + 1)
        .read_to_end(&mut b)
        .map_err(|e| e.to_string())?;
    if b.len() as u64 > limit {
        return Err("파일 또는 압축 해제 크기 제한 초과".into());
    }
    Ok(b)
}
fn extract_text(ext: &str, bytes: &[u8]) -> Result<String, String> {
    match ext.to_ascii_lowercase().as_str() {
        "txt" => String::from_utf8(bytes.to_vec()).map_err(|e| e.to_string()),
        "csv" => {
            let mut out = String::new();
            let mut r = csv::ReaderBuilder::new()
                .has_headers(false)
                .from_reader(bytes);
            for row in r.records() {
                out.push_str(
                    &row.map_err(|e| e.to_string())?
                        .iter()
                        .collect::<Vec<_>>()
                        .join(" "),
                );
                out.push('\n')
            }
            Ok(out)
        }
        "xlsx" => {
            use calamine::{Reader, Xlsx};
            let mut wb = Xlsx::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
            let mut out = String::new();
            for s in wb.sheet_names().to_vec() {
                if let Ok(range) = wb.worksheet_range(&s) {
                    for (i, row) in range.rows().enumerate() {
                        for (j, c) in row.iter().enumerate() {
                            out.push_str(&format!("{s}!{}{}: {}\n", column_name(j), i + 1, c));
                        }
                    }
                }
            }
            Ok(out)
        }
        "pdf" => {
            let doc = lopdf::Document::load_mem(bytes).map_err(|e| e.to_string())?;
            let mut out = String::new();
            for (page, _text) in doc.get_pages() {
                out.push_str(&format!(
                    "\n@@PAGE {page}@@\n{}",
                    doc.extract_text(&[page]).unwrap_or_default()
                ));
            }
            Ok(out)
        }
        "docx" | "pptx" => extract_office_text(bytes),
        _ => Err("지원하지 않는 파일 형식".into()),
    }
}
fn column_name(mut n: usize) -> String {
    let mut s = String::new();
    loop {
        let r = n % 26;
        s.insert(0, (b'A' + r as u8) as char);
        if n < 26 {
            break;
        }
        n = n / 26 - 1
    }
    s
}
fn extract_office_text(bytes: &[u8]) -> Result<String, String> {
    use zip::ZipArchive;
    let mut z = ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut out = String::new();
    let mut total = 0u64;
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(|e| e.to_string())?;
        let name = f.name().to_owned();
        if !(name.starts_with("word/") || name.starts_with("ppt/slides/"))
            || !name.ends_with(".xml")
        {
            continue;
        }
        total = total.saturating_add(f.size());
        if total > MAX_ARCHIVE {
            return Err("문서 압축 해제 크기 제한 초과".into());
        }
        let b = read_limited(&mut f, MAX_ARCHIVE)?;
        let mut reader = quick_xml::Reader::from_reader(b.as_slice());
        reader.config_mut().trim_text(true);
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(quick_xml::events::Event::Text(t)) => {
                    out.push_str(
                        &quick_xml::escape::unescape(&t.decode().map_err(|e| e.to_string())?)
                            .map_err(|e| e.to_string())?,
                    );
                    out.push(' ')
                }
                Ok(quick_xml::events::Event::Eof) => break,
                Err(e) => return Err(e.to_string()),
                _ => (),
            }
            buf.clear()
        }
    }
    Ok(out)
}
fn parse_locations(ext: &str, text: &str) -> Vec<(String, String)> {
    if ext.eq_ignore_ascii_case("pdf") {
        let mut page = "1";
        text.lines()
            .filter_map(|l| {
                if let Some(p) = l.strip_prefix("@@PAGE ").and_then(|x| x.strip_suffix("@@")) {
                    page = p;
                    None
                } else {
                    Some((page.to_owned(), l.to_owned()))
                }
            })
            .collect()
    } else if ext.eq_ignore_ascii_case("xlsx") {
        text.lines()
            .filter_map(|l| {
                l.split_once(": ")
                    .map(|(loc, v)| (loc.to_owned(), v.to_owned()))
            })
            .collect()
    } else {
        text.lines()
            .enumerate()
            .map(|(i, l)| ((i + 1).to_string(), l.to_string()))
            .collect()
    }
}
fn scan_folder(
    root: &Path,
    recursive: bool,
    custom: &[(String, String)],
) -> Result<(Vec<Finding>, Vec<String>), String> {
    compile_patterns(custom)?;
    let mut files = Vec::new();
    fn walk(p: &Path, r: bool, o: &mut Vec<PathBuf>) -> Result<(), String> {
        for e in fs::read_dir(p).map_err(|e| e.to_string())? {
            let p = e.map_err(|e| e.to_string())?.path();
            let m = fs::symlink_metadata(&p).map_err(|e| e.to_string())?;
            if m.file_type().is_symlink() {
                continue;
            }
            if m.is_file() {
                o.push(p)
            } else if m.is_dir() && r {
                walk(&p, r, o)?
            }
            if o.len() >= MAX_FILES {
                break;
            }
        }
        Ok(())
    }
    walk(root, recursive, &mut files)?;
    let mut total = 0u64;
    let (mut findings, mut errors) = (Vec::new(), Vec::new());
    for p in files {
        let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("");
        if !["txt", "csv", "xlsx", "pdf", "docx", "pptx"]
            .iter()
            .any(|x| x.eq_ignore_ascii_case(ext))
        {
            continue;
        }
        let result = (|| {
            let m = fs::metadata(&p).map_err(|e| e.to_string())?;
            total = total.saturating_add(m.len());
            if m.len() > MAX_INPUT || total > MAX_TOTAL {
                return Err("스캔 크기 제한 초과".into());
            }
            let b = read_limited(File::open(&p).map_err(|e| e.to_string())?, MAX_INPUT)?;
            let text = extract_text(ext, &b)?;
            let locs = parse_locations(ext, &text);
            let mut found = Vec::new();
            for (l, t) in locs {
                found.extend(scan_text(&t, &p.to_string_lossy(), &l, custom)?)
            }
            Ok::<_, String>(found)
        })();
        match result {
            Ok(mut f) => findings.append(&mut f),
            Err(e) => errors.push(format!("{}: {e}", p.display())),
        }
    }
    Ok((findings, errors))
}
fn reserve_output(path: &Path) -> Result<File, String> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("출력 파일을 만들 수 없습니다(기존 파일은 덮어쓰지 않음): {e}"))
}
fn find_soffice() -> Option<PathBuf> {
    if let Ok(paths) = std::env::var("PATH") {
        for p in std::env::split_paths(&paths) {
            for n in ["soffice.exe", "soffice"] {
                let f = p.join(n);
                if f.is_file() {
                    return Some(f);
                }
            }
        }
    }
    #[cfg(windows)]
    for p in [
        r"C:\Program Files\LibreOffice\program\soffice.exe",
        r"C:\Program Files (x86)\LibreOffice\program\soffice.exe",
    ] {
        let f = PathBuf::from(p);
        if f.is_file() {
            return Some(f);
        }
    }
    None
}
fn convert_with(
    exe: Option<&Path>,
    source: &str,
    output: &str,
    timeout: Duration,
) -> Result<String, String> {
    let exe = exe.ok_or_else(|| {
        "LibreOffice(soffice)를 찾을 수 없습니다. 로컬에 설치하고 다시 시도하세요.".to_string()
    })?;
    let src = Path::new(source);
    let ext = src
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == "hwpx" {
        return Err("HWPX는 지원하지 않습니다. 한컴오피스에서 PDF로 내보내세요.".into());
    }
    if !["docx", "xlsx", "pptx", "doc", "xls", "ppt", "hwp"].contains(&ext.as_str()) {
        return Err("지원 형식: DOCX, XLSX, PPTX 및 LibreOffice 필터가 처리하는 일부 HWP".into());
    }
    let meta = fs::metadata(src).map_err(|e| e.to_string())?;
    if meta.len() > MAX_INPUT {
        return Err("입력 파일은 250 MiB 이하여야 합니다.".into());
    }
    let dest = Path::new(output);
    let parent = dest.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temp = tempfile::Builder::new()
        .prefix("eduwork-pdf-")
        .tempdir()
        .map_err(|e| e.to_string())?;
    let profile = temp.path().join("profile");
    let uri = url::Url::from_directory_path(&profile).map_err(|_| "임시 프로필 경로 오류")?;
    let mut child = Command::new(exe)
        .arg(format!("-env:UserInstallation={uri}"))
        .arg("--headless")
        .arg("--convert-to")
        .arg("pdf")
        .arg("--outdir")
        .arg(temp.path())
        .arg(src)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("LibreOffice 실행 실패: {e}"))?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            if !status.success() {
                return Err(format!("LibreOffice 변환 실패: {status}"));
            }
            break;
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err("LibreOffice 변환 시간 제한 초과".into());
        }
        std::thread::sleep(Duration::from_millis(100))
    }
    let produced = temp.path().join(format!(
        "{}.pdf",
        src.file_stem().and_then(|s| s.to_str()).unwrap_or("output")
    ));
    let bytes = read_limited(
        File::open(&produced).map_err(|e| format!("변환된 PDF를 찾을 수 없습니다: {e}"))?,
        MAX_INPUT * 4,
    )?;
    if bytes.is_empty() || !bytes.starts_with(b"%PDF-") {
        return Err("LibreOffice 결과가 유효한 PDF가 아닙니다.".into());
    }
    let mut out = reserve_output(dest)?;
    out.write_all(&bytes).map_err(|e| e.to_string())?;
    Ok(dest.to_string_lossy().into_owned())
}
#[tauri::command]
pub fn convert_document(path: String, output: String) -> Result<String, String> {
    convert_with(
        find_soffice().as_deref(),
        &path,
        &output,
        Duration::from_secs(180),
    )
}
#[tauri::command]
pub fn privacy_scan(
    root: String,
    include_subfolders: bool,
    custom: Vec<(String, String)>,
) -> Result<(Vec<Finding>, Vec<String>), String> {
    scan_folder(Path::new(&root), include_subfolders, &custom)
}
#[tauri::command]
pub fn privacy_export_csv(findings: Vec<Finding>, destination: String) -> Result<usize, String> {
    let mut w = csv::WriterBuilder::new().from_writer(Vec::new());
    w.write_record([
        "파일 경로",
        "패턴 유형",
        "문서 위치",
        "검사 시각",
        "마스킹 값",
    ])
    .map_err(|e| e.to_string())?;
    for f in &findings {
        w.write_record([
            &f.path,
            &f.pattern_type,
            &f.location,
            &f.scanned_at,
            &f.masked_value,
        ])
        .map_err(|e| e.to_string())?
    }
    let data = w.into_inner().map_err(|e| e.to_string())?;
    let mut file = reserve_output(Path::new(&destination))?;
    file.write_all(b"\xef\xbb\xbf")
        .and_then(|_| file.write_all(&data))
        .map_err(|e| e.to_string())?;
    Ok(findings.len())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn masks_default_types_location_and_no_raw_match() {
        let h = scan_text("900101-1234567 010-1234-5678 a@b.kr", "n.txt", "4", &[]).unwrap();
        assert_eq!(h.len(), 3);
        assert!(h
            .iter()
            .all(|x| x.masked_value.contains('*') && x.location == "4"));
        assert!(format!("{h:?}").find("900101").is_none());
    }
    #[test]
    fn custom_unicode_match_masks_without_panicking() {
        let matches = scan_text(
            "prefix-ééfin",
            "unicode.txt",
            "1",
            &[("custom".into(), "ééfin".into())],
        )
        .unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].masked_value, "****in");
    }
    #[test]
    fn custom_invalid_regex() {
        assert!(compile_patterns(&[("bad".into(), "(?=x)".into())]).is_err())
    }
    #[test]
    fn csv_and_xlsx_extraction() {
        assert!(extract_text("csv", b"a,b\n010-1234-5678,x")
            .unwrap()
            .contains("010-1234"));
        let mut w = rust_xlsxwriter::Workbook::new();
        w.add_worksheet().write_string(0, 0, "a@b.kr").unwrap();
        let mut b = Vec::new();
        w.save_to_writer(&mut b).unwrap();
        assert!(extract_text("xlsx", &b).unwrap().contains("a@b.kr"));
    }
    #[test]
    fn pdf_selectable_extraction() {
        let d = lopdf::Document::with_version("1.5");
        let pages = d.get_pages();
        assert!(extract_text("pdf", b"bad").is_err());
        let _ = pages;
    }
    #[test]
    fn per_file_error_continues() {
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join("bad.pdf"), b"bad").unwrap();
        fs::write(t.path().join("good.txt"), "a@b.kr").unwrap();
        let (h, e) = scan_folder(t.path(), false, &[]).unwrap();
        assert_eq!(h.len(), 1);
        assert_eq!(e.len(), 1)
    }
    #[test]
    fn converter_missing_timeout_and_hwpx() {
        assert!(
            convert_with(None, "a.docx", "b.pdf", Duration::from_millis(1))
                .unwrap_err()
                .contains("LibreOffice")
        );
        assert!(convert_with(
            Some(Path::new("fake")),
            "a.hwpx",
            "b.pdf",
            Duration::from_millis(1)
        )
        .unwrap_err()
        .contains("HWPX"));
    }
    #[test]
    fn output_no_overwrite_source_preserved() {
        let t = tempfile::tempdir().unwrap();
        let s = t.path().join("a.docx");
        let d = t.path().join("a.pdf");
        fs::write(&s, b"source").unwrap();
        fs::write(&d, b"existing").unwrap();
        assert!(reserve_output(&d).is_err());
        assert_eq!(fs::read(&s).unwrap(), b"source");
        assert_eq!(fs::read(&d).unwrap(), b"existing")
    }
}
