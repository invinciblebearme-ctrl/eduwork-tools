use super::*;
use tempfile::tempdir;
fn template(kind: &str) -> Template {
    Template {
        name: "테스트".into(),
        kind: kind.into(),
        format: "png".into(),
        width: Some(4),
        group_column: None,
    }
}
#[test]
fn persistence_is_atomic_and_malformed_json_is_preserved() {
    let d = tempdir().unwrap();
    let mut s = Store::load(d.path()).unwrap();
    s.templates.push(template("images"));
    s.save(d.path()).unwrap();
    assert_eq!(Store::load(d.path()).unwrap().templates.len(), 1);
    std::fs::write(d.path().join("workflows.json"), b"broken").unwrap();
    assert!(Store::load(d.path()).is_err());
    assert_eq!(
        std::fs::read(d.path().join("workflows.json")).unwrap(),
        b"broken"
    );
}
#[test]
fn images_run_records_partial_errors_and_never_overwrites() {
    let d = tempdir().unwrap();
    let input = d.path().join("in");
    let out = d.path().join("out");
    fs::create_dir(&input).unwrap();
    fs::create_dir(&out).unwrap();
    image::RgbImage::new(8, 4)
        .save(input.join("a.png"))
        .unwrap();
    fs::write(input.join("bad.png"), b"bad").unwrap();
    fs::write(out.join("a.png"), b"keep").unwrap();
    let r = run_saved(d.path(), template("images"), &input, &out).unwrap();
    assert!(!r.success);
    assert_eq!(r.file_count, 2);
    assert_eq!(r.outputs.len(), 1);
    assert_eq!(r.errors.len(), 1);
    assert_eq!(image::image_dimensions(&r.outputs[0]).unwrap(), (4, 2));
    assert_eq!(fs::read(out.join("a.png")).unwrap(), b"keep");
    assert_eq!(Store::load(d.path()).unwrap().history.len(), 1);
    assert!(!r.started_at.is_empty() && !r.finished_at.is_empty());
}
fn pdf_fixture(p: &Path, label: &str) {
    let mut d = lopdf::Document::with_version("1.5");
    let pages = d.new_object_id();
    let font = d.add_object(
        lopdf::dictionary! {"Type"=>"Font", "Subtype"=>"Type1", "BaseFont"=>"Helvetica"},
    );
    let content = d.add_object(lopdf::Stream::new(
        lopdf::dictionary! {},
        format!("BT /F1 12 Tf 10 10 Td ({label}) Tj ET").into_bytes(),
    ));
    let page = d.add_object(lopdf::dictionary!{"Type"=>"Page", "Parent"=>pages,"MediaBox"=>vec![0.into(),0.into(),200.into(),200.into()], "Resources"=>lopdf::dictionary!{"Font"=>lopdf::dictionary!{"F1"=>font}}, "Contents"=>content});
    d.objects.insert(
        pages,
        lopdf::Object::Dictionary(
            lopdf::dictionary! {"Type"=>"Pages", "Kids"=>vec![page.into()], "Count"=>1},
        ),
    );
    let root = d.add_object(lopdf::dictionary! {"Type"=>"Catalog","Pages"=>pages});
    d.trailer.set("Root", root);
    d.save(p).unwrap();
}
#[test]
fn pdf_run_sorts_names_and_records_collision_and_invalid_input() {
    let d = tempdir().unwrap();
    let input = d.path().join("in");
    fs::create_dir(&input).unwrap();
    pdf_fixture(&input.join("b.pdf"), "B");
    pdf_fixture(&input.join("a.pdf"), "A");
    let output = d.path().join("merged.pdf");
    let r = run_saved(d.path(), template("pdf"), &input, &output).unwrap();
    assert!(r.success);
    let doc = lopdf::Document::load(&output).unwrap();
    assert!(doc.extract_text(&[1]).unwrap().contains('A'));
    let before = fs::read(&output).unwrap();
    assert!(
        !run_saved(d.path(), template("pdf"), &input, &output)
            .unwrap()
            .success
    );
    assert_eq!(fs::read(&output).unwrap(), before);
    fs::write(input.join("bad.pdf"), b"bad").unwrap();
    assert!(
        !run_saved(d.path(), template("pdf"), &input, &d.path().join("bad.pdf"))
            .unwrap()
            .success
    );
    assert_eq!(Store::load(d.path()).unwrap().history.len(), 3);
}
#[test]
fn empty_folder_and_unsupported_workflow_are_logged() {
    let d = tempdir().unwrap();
    for kind in ["pdf", "unsupported"] {
        assert!(
            !run_saved(
                d.path(),
                template(kind),
                d.path(),
                &d.path().join("out.pdf")
            )
            .unwrap()
            .success
        );
    }
    assert_eq!(Store::load(d.path()).unwrap().history.len(), 2);
}

#[test]
fn excel_workflow_cleans_and_groups_compatible_rosters() {
    let d = tempdir().unwrap();
    let input = d.path().join("in");
    fs::create_dir(&input).unwrap();
    let rows = vec![
        vec!["Name".into(), "Class".into()],
        vec!["Kim".into(), "A".into()],
        vec![" ".into(), " ".into()],
    ];
    for name in ["a.xlsx", "b.xlsx"] {
        excel::write_single(&input.join(name), "Roster", &rows).unwrap();
    }
    let mut t = template("excel");
    t.group_column = Some(2);
    let out = d.path().join("roster.xlsx");
    let r = run_saved(d.path(), t.clone(), &input, &out).unwrap();
    assert!(r.success, "{:?}", r.errors);
    assert_eq!(r.outputs.len(), 2);
    assert_eq!(excel::read_sheet(&out, "Roster").unwrap().len(), 2);
    let r = run_saved(d.path(), t, &input, &out).unwrap();
    assert!(!r.success);
}
#[test]
fn limits_reject_before_processing_and_record_error() {
    let d = tempdir().unwrap();
    let input = d.path().join("in");
    fs::create_dir(&input).unwrap();
    for i in 0..51 {
        fs::write(input.join(format!("{i}.png")), b"bad").unwrap();
    }
    let r = run_saved(d.path(), template("images"), &input, d.path()).unwrap();
    assert!(!r.success);
    assert!(r.outputs.is_empty());
    assert!(r.errors[0].contains("50"));
}
#[cfg(windows)]
#[test]
fn skips_symlinks_when_os_allows_creation() {
    let d = tempdir().unwrap();
    let input = d.path().join("in");
    fs::create_dir(&input).unwrap();
    let source = d.path().join("source.png");
    image::RgbImage::new(8, 4).save(&source).unwrap();
    if let Err(e) = std::os::windows::fs::symlink_file(&source, input.join("link.png")) {
        eprintln!("symlink test unavailable: {e}");
        return;
    }
    let r = run_saved(d.path(), template("images"), &input, d.path()).unwrap();
    assert_eq!(r.skipped_links, 1);
    assert_eq!(r.file_count, 0);
}

#[test]
fn rejects_wrong_pdf_extension_and_persists_failure() {
    let d = tempdir().unwrap();
    let input = d.path().join("in");
    fs::create_dir(&input).unwrap();
    pdf_fixture(&input.join("a.pdf"), "A");
    pdf_fixture(&input.join("b.pdf"), "B");
    let out = d.path().join("wrong.txt");
    let r = run_saved(d.path(), template("pdf"), &input, &out).unwrap();
    assert!(!r.success);
    assert!(!out.exists());
    let stored = Store::load(d.path()).unwrap();
    assert!(!stored.history[0].success);
    assert!(!stored.history[0].finished_at.is_empty());
    assert!(!stored.history[0].errors.is_empty());
}
#[test]
fn rejects_invalid_inactive_template_fields() {
    let mut t = template("pdf");
    t.width = Some(0);
    assert!(t.validate().is_err());
    t.width = None;
    t.format = "invalid".into();
    assert!(t.validate().is_err());
}

#[test]
fn retention_roundtrip_keeps_latest_200_and_templates() {
    let d = tempdir().unwrap();
    let mut s = Store::default();
    s.templates.push(template("images"));
    s.save(d.path()).unwrap();
    for n in 0..201 {
        let mut t = template("pdf");
        t.name = n.to_string();
        run_saved(d.path(), t, d.path(), &d.path().join("out.pdf")).unwrap();
    }
    let s = Store::load(d.path()).unwrap();
    assert_eq!(s.templates[0].name, template("images").name);
    assert_eq!(s.history.len(), 200);
    assert_eq!(s.history.first().unwrap().template.name, "1");
    assert_eq!(s.history.last().unwrap().template.name, "200");
    assert!(s
        .history
        .iter()
        .all(|r| !r.success && !r.finished_at.is_empty()));
    let mut s = s;
    s.templates[0].width = Some(20);
    s.save(d.path()).unwrap();
    assert_eq!(Store::load(d.path()).unwrap().templates[0].width, Some(20));
    s.templates.clear();
    s.save(d.path()).unwrap();
    assert!(Store::load(d.path()).unwrap().templates.is_empty());
}
#[test]
fn initial_persistence_failure_prevents_outputs() {
    let d = tempdir().unwrap();
    let input = d.path().join("in");
    fs::create_dir(&input).unwrap();
    image::RgbImage::new(8, 4)
        .save(input.join("a.png"))
        .unwrap();
    let blocked = d.path().join("blocked");
    fs::write(&blocked, b"keep").unwrap();
    assert!(run_saved(&blocked, template("images"), &input, d.path()).is_err());
    assert!(!d.path().join("a.png").exists());
    assert_eq!(fs::read(blocked).unwrap(), b"keep");
}
#[test]
fn invalid_path_types_produce_no_outputs() {
    let d = tempdir().unwrap();
    let file = d.path().join("input.png");
    image::RgbImage::new(8, 4).save(&file).unwrap();
    for (input, output) in [(file.as_path(), d.path()), (d.path(), file.as_path())] {
        let r = run_saved(d.path(), template("images"), input, output).unwrap();
        assert!(!r.success);
        assert!(r.outputs.is_empty());
        assert_eq!(r.file_count, 0);
    }
}

#[test]
fn final_persistence_failure_reports_completed_outputs_and_keeps_start_record() {
    let d = tempdir().unwrap();
    let backup = d.path().join("start.json");
    let out = d.path().join("complete.png");
    let err = run_saved_with(
        d.path(),
        template("images"),
        d.path(),
        d.path(),
        |_, _, _, run| {
            crate::output::write_new(&out, b"complete").unwrap();
            run.outputs.push(out.display().to_string());
            fs::rename(d.path().join("workflows.json"), &backup).unwrap();
            fs::create_dir(d.path().join("workflows.json")).unwrap();
            Ok(())
        },
    )
    .err()
    .unwrap();
    assert!(err.contains(out.to_str().unwrap()));
    assert_eq!(fs::read(&out).unwrap(), b"complete");
    fs::remove_dir(d.path().join("workflows.json")).unwrap();
    fs::rename(backup, d.path().join("workflows.json")).unwrap();
    assert!(Store::load(d.path()).unwrap().history[0]
        .finished_at
        .is_empty());
}
#[test]
fn oversized_and_overlimit_stores_are_preserved() {
    let d = tempdir().unwrap();
    let path = d.path().join("workflows.json");
    let file = fs::File::create(&path).unwrap();
    file.set_len(MAX_STORE_BYTES + 1).unwrap();
    drop(file);
    assert!(Store::load(d.path()).is_err());
    assert_eq!(fs::metadata(&path).unwrap().len(), MAX_STORE_BYTES + 1);
    let s = Store {
        templates: vec![template("images"); 101],
        history: vec![],
    };
    fs::write(&path, serde_json::to_vec(&s).unwrap()).unwrap();
    assert!(Store::load(d.path()).is_err());
}

#[test]
fn template_commands_roundtrip_update_delete_and_limit() {
    let d = tempdir().unwrap();
    save_template(d.path(), template("images"), false).unwrap();
    let mut t = Store::load(d.path()).unwrap().templates.remove(0);
    t.width = Some(12);
    save_template(d.path(), t.clone(), false).unwrap();
    let s = Store::load(d.path()).unwrap();
    assert_eq!(s.templates.len(), 1);
    assert_eq!(s.templates[0].width, Some(12));
    save_template(d.path(), t, true).unwrap();
    assert!(Store::load(d.path()).unwrap().templates.is_empty());
    for n in 0..100 {
        let mut t = template("images");
        t.name = n.to_string();
        save_template(d.path(), t, false).unwrap();
    }
    assert!(save_template(d.path(), template("images"), false).is_err());
    assert_eq!(Store::load(d.path()).unwrap().templates.len(), 100);
}

#[test]
fn saved_image_templates_execute_all_formats_and_reload_history() {
    let d = tempdir().unwrap();
    let input = d.path().join("in");
    let output = d.path().join("out");
    fs::create_dir(&input).unwrap();
    fs::create_dir(&output).unwrap();
    image::RgbImage::new(8, 4)
        .save(input.join("한글.png"))
        .unwrap();
    for format in ["png", "jpeg", "webp"] {
        let mut t = template("images");
        t.name = format.into();
        t.format = format.into();
        save_template(d.path(), t, false).unwrap();
        let loaded = Store::load(d.path())
            .unwrap()
            .templates
            .into_iter()
            .find(|t| t.name == format)
            .unwrap();
        let r = run_saved(d.path(), loaded, &input, &output).unwrap();
        assert!(r.success, "{:?}", r.errors);
        assert_eq!(r.outputs.len(), 1);
        assert_eq!(image::image_dimensions(&r.outputs[0]).unwrap(), (4, 2));
        let stored = Store::load(d.path()).unwrap();
        assert_eq!(stored.history.last().unwrap().outputs, r.outputs);
    }
    assert_eq!(Store::load(d.path()).unwrap().history.len(), 3);
}
