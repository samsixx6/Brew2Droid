use std::path::PathBuf;
use std::process::Command;

fn scratch() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let path = workspace.join("local-tests").join(format!(
        "probe-test-{}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    assert!(path.canonicalize().unwrap().starts_with(workspace));
    path
}

#[test]
fn synthetic_malloc_crosses_hle_and_returns_a_module() {
    let work = scratch();
    let output = work.join("report.json");
    let result = Command::new(env!("CARGO_BIN_EXE_brew-probe"))
        .args(["synthetic", "--work-dir"])
        .arg(&work)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
    assert_eq!(report["module_load"]["kind"], "returned");
    assert_eq!(report["module_load"]["code"], 0);
    assert_eq!(report["module_pointer"], 0x1000_0000u32);
    assert_eq!(report["api_calls"], 1);
    assert_eq!(report["gameplay_validated"], false);
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn malformed_mif_is_rejected_before_mapping_or_reporting_a_run() {
    let work = scratch();
    let source = work.join("source");
    let run = work.join("run");
    std::fs::create_dir(&source).unwrap();
    std::fs::create_dir(&run).unwrap();
    let module = source.join("test.mod");
    std::fs::write(&module, [0x1e, 0xff, 0x2f, 0xe1, 0, 0, 0, 0]).unwrap();
    let mif = source.join("test.mif");
    let mut header = vec![0u8; 32];
    header[..4].copy_from_slice(&[0x11, 0, 1, 0]);
    header[16..20].copy_from_slice(&32u32.to_le_bytes());
    header[20..24].copy_from_slice(&u32::MAX.to_le_bytes());
    std::fs::write(&mif, header).unwrap();
    let output = run.join("report.json");
    let result = Command::new(env!("CARGO_BIN_EXE_brew-probe"))
        .arg("trace-start")
        .arg(&module)
        .arg(&mif)
        .arg("--work-dir")
        .arg(&run)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("MIF"));
    assert!(!output.exists());
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn excessive_instruction_budget_is_rejected_before_running_guest_code() {
    let work = scratch();
    let output = work.join("report.json");
    let result = Command::new(env!("CARGO_BIN_EXE_brew-probe"))
        .args([
            "synthetic",
            "--budget",
            "18446744073709551615",
            "--work-dir",
        ])
        .arg(&work)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("budget"));
    assert!(!output.exists());
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn bounded_continuation_finishes_the_same_module_call() {
    let work = scratch();
    let output = work.join("report.json");
    let result = Command::new(env!("CARGO_BIN_EXE_brew-probe"))
        .args(["synthetic", "--budget", "1", "--work-dir"])
        .arg(&work)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
    assert_eq!(report["module_load"]["kind"], "returned");
    assert_eq!(report["module_load"]["code"], 0);
    assert_eq!(report["module_pointer"], 0x1000_0000u32);
    assert_eq!(
        report["api_calls"], 1,
        "continuar não pode executar MALLOC de novo"
    );
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn inspection_reports_resource_identity_without_starting_the_guest() {
    let work = scratch();
    let content = work.join("source");
    let run = work.join("run");
    std::fs::create_dir(&content).unwrap();
    std::fs::create_dir(&run).unwrap();
    let module = content.join("test.mod");
    std::fs::write(
        &module,
        [0xe3a0_0000u32.to_le_bytes(), 0xe12f_ff1eu32.to_le_bytes()].concat(),
    )
    .unwrap();
    std::fs::write(content.join("main.bar"), b"abc").unwrap();
    let mif = content.join("test.mif");
    let mut header = vec![0u8; 36];
    header[..4].copy_from_slice(&[0x11, 0, 1, 0]);
    header[16..20].copy_from_slice(&32u32.to_le_bytes());
    header[32..36].copy_from_slice(&36u32.to_le_bytes());
    std::fs::write(&mif, header).unwrap();
    let output = run.join("report.json");
    let result = Command::new(env!("CARGO_BIN_EXE_brew-probe"))
        .arg("inspect")
        .arg(module)
        .arg(mif)
        .arg("--work-dir")
        .arg(&run)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
    let resource = report["package"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["path"] == "main.bar")
        .unwrap();
    assert_eq!(
        resource["sha256"],
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert!(report.get("api_calls").is_none());
    assert!(!run.join("content").exists());
    std::fs::remove_dir_all(work).unwrap();
}

fn fixture(source: &std::path::Path, code: Vec<u8>) -> (PathBuf, PathBuf) {
    std::fs::create_dir_all(source).unwrap();
    let module = source.join("test.mod");
    let mif = source.join("test.mif");
    std::fs::write(&module, code).unwrap();
    let mut bytes = vec![0u8; 36];
    bytes[..4].copy_from_slice(&[0x11, 0, 1, 0]);
    bytes[16..20].copy_from_slice(&32u32.to_le_bytes());
    bytes[32..36].copy_from_slice(&36u32.to_le_bytes());
    std::fs::write(&mif, bytes).unwrap();
    (module, mif)
}

#[test]
fn media_requests_remain_available_in_the_startup_report() {
    let work = scratch();
    let run = work.join("run");
    std::fs::create_dir(&run).unwrap();
    // A chamada atravessa o ARM/HLE real; o relatório precisa conservar objeto e parâmetro.
    let words = [0xe92d4000u32, 0xe3a00088, 0xe3a01004, 0xe3a0202a,
        0xe59f3008, 0xe12fff33, 0xe3a00000, 0xe8bd8000,
        zeebx::brew::aee::encode(zeebx::brew::aee::Interface::Media, 4)];
    let (module, mif) = fixture(&work.join("source"),
        words.into_iter().flat_map(u32::to_le_bytes).collect());
    let output = run.join("report.json");
    let result = Command::new(env!("CARGO_BIN_EXE_brew-probe"))
        .arg("trace-start").arg(module).arg(mif)
        .arg("--work-dir").arg(&run).arg("--output").arg(&output)
        .output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let report: serde_json::Value = serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
    assert_eq!(report["media_log"][0][1], 0x88);
    assert!(report["media_log"][0][2].as_str().unwrap().contains("parm 4, 0x2a"));
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn nested_qsort_failures_remain_visible_when_the_outer_call_returns() {
    for (field, comparator) in [
        (
            "missing_apis",
            vec![
                0xe51f_f004u32,
                zeebx::brew::aee::encode(zeebx::brew::aee::Interface::Shell, 255),
            ],
        ),
        (
            "memory_faults",
            vec![0xe59f_0004u32, 0xe590_0000, 0xe12f_ff1e, 0x6000_0000],
        ),
    ] {
        let work = scratch();
        let run = work.join("run");
        std::fs::create_dir(&run).unwrap();
        let qsort = zeebx::brew::aee_helpers::HELPERS
            .iter()
            .position(|name| *name == "qsort")
            .unwrap() as u32;
        // qsort chama um comparador guest que falha; o donor depois retorna ao módulo.
        let mut words = vec![
            0xe92d_4010,
            0xe1a0_4002,
            0xe591_c000 | qsort * 4,
            0xe28f_0018,
            0xe3a0_1002,
            0xe3a0_2004,
            0xe28f_3014,
            0xe12f_ff3c,
            0xe3a0_0000,
            0xe584_0000,
            0xe8bd_8010,
            2,
            1,
        ];
        words.extend(comparator);
        let (module, mif) = fixture(
            &work.join("source"),
            words.into_iter().flat_map(u32::to_le_bytes).collect(),
        );
        let output = run.join("report.json");
        let result = Command::new(env!("CARGO_BIN_EXE_brew-probe"))
            .arg("trace-start")
            .arg(module)
            .arg(mif)
            .arg("--work-dir")
            .arg(&run)
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
        assert_eq!(report["module_load"]["kind"], "returned");
        assert_eq!(report["module_load"]["code"], 0);
        assert!(
            report["diagnostics"][field]
                .as_array()
                .is_some_and(|values| !values.is_empty()),
            "nested {field} was hidden: {report}"
        );
        assert_eq!(report["diagnostics"]["clean"], false);
        std::fs::remove_dir_all(work).unwrap();
    }
}

#[test]
fn source_output_and_work_trees_must_not_overlap() {
    for case in [
        "output_in_source",
        "work_in_source",
        "source_in_runtime",
        "output_in_mif_source",
    ] {
        let work = scratch();
        let source = if case == "source_in_runtime" {
            work.join("run/runtime/aparelho")
        } else {
            work.join("source")
        };
        let (module, mut mif) = fixture(
            &source,
            [0xe3a0_0000u32.to_le_bytes(), 0xe12f_ff1eu32.to_le_bytes()].concat(),
        );
        let run = if case == "work_in_source" {
            source.join("run")
        } else {
            work.join("run")
        };
        std::fs::create_dir_all(&run).unwrap();
        let output = match case {
            "output_in_source" => source.join("report.json"),
            "output_in_mif_source" => {
                let mif_source = work.join("mifs");
                std::fs::create_dir(&mif_source).unwrap();
                let separate = mif_source.join("test.mif");
                std::fs::rename(&mif, &separate).unwrap();
                mif = separate;
                mif_source.join("report.json")
            }
            _ => run.join("report.json"),
        };
        let result = Command::new(env!("CARGO_BIN_EXE_brew-probe"))
            .arg("inspect")
            .arg(module)
            .arg(mif)
            .arg("--work-dir")
            .arg(&run)
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap();
        assert!(!result.status.success(), "source overlap accepted: {case}");
        assert!(!output.exists(), "source/output changed: {case}");
        std::fs::remove_dir_all(work).unwrap();
    }
}

#[test]
fn existing_runtime_tree_is_rejected_before_storage_initialization() {
    let work = scratch();
    std::fs::create_dir(work.join("runtime")).unwrap();
    std::fs::write(work.join("runtime/sentinel"), b"preserve").unwrap();
    let output = work.join("report.json");
    let result = Command::new(env!("CARGO_BIN_EXE_brew-probe"))
        .args(["synthetic", "--work-dir"])
        .arg(&work)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(std::fs::read_dir(work.join("runtime")).unwrap().count(), 1);
    assert!(!output.exists());
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn extended_diagnostic_captures_the_real_framebuffer_in_private_storage() {
    let work = scratch();
    let output = work.join("report.json");
    let result = Command::new(env!("CARGO_BIN_EXE_brew-probe"))
        .args(["synthetic", "--advance", "600", "--capture", "--work-dir"])
        .arg(&work)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
    let capture = &report["framebuffer_capture"];
    assert_eq!(capture["format"], "RGB565_LE");
    assert_eq!(capture["width"], 640);
    assert_eq!(capture["height"], 480);
    assert_eq!(capture["row_stride_bytes"], 1280);
    let pixels = std::fs::read(work.join(capture["relative_file"].as_str().unwrap())).unwrap();
    assert_eq!(pixels.len(), 640 * 480 * 2);
    assert!(
        pixels.iter().all(|byte| *byte == 0),
        "o módulo só aloca; não desenha"
    );
    assert!(report["guest_clock_ms"].is_u64());
    assert_eq!(report["gameplay_validated"], false);
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn candidate_phone_display_changes_pixels_without_verifying_a_profile() {
    let work = scratch();
    let output = work.join("report.json");
    let result = Command::new(env!("CARGO_BIN_EXE_brew-probe"))
        .args([
            "synthetic",
            "--candidate-display",
            "240x320",
            "--capture",
            "--work-dir",
        ])
        .arg(&work)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
    assert_eq!(report["candidate_display"]["width"], 240);
    assert_eq!(report["candidate_display"]["height"], 320);
    assert_eq!(
        report["candidate_display"]["handset_geometry_verified"],
        false
    );
    assert_eq!(report["handset_profile_verified"], false);
    assert_eq!(report["framebuffer_capture"]["byte_length"], 240 * 320 * 2);
    assert_eq!(report["gameplay_validated"], false);
    std::fs::remove_dir_all(work).unwrap();
}

#[test]
fn callback_resumption_limit_is_explicit_and_bounded_before_running() {
    for limit in ["33", "4294967295"] {
        let work = scratch();
        let output = work.join("report.json");
        let result = Command::new(env!("CARGO_BIN_EXE_brew-probe"))
            .args(["synthetic", "--callback-resumes", limit, "--work-dir"])
            .arg(&work)
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("Callback resume limit"));
        assert!(!work.join("runtime").exists());
        assert!(!output.exists());
        std::fs::remove_dir_all(work).unwrap();
    }
}

#[test]
fn invalid_handset_key_scripts_are_rejected_before_starting_guest_code() {
    for script in [
        r#"[{"step":1,"key":"unknown","down":true}]"#,
        r#"[{"step":2,"key":"5","down":true}]"#,
        r#"[{"step":1,"key":"5","down":true},{"step":0,"key":"5","down":false}]"#,
    ] {
        let work = scratch();
        let run = work.join("run");
        std::fs::create_dir(&run).unwrap();
        let (module, mif) = fixture(
            &work.join("source"),
            [0xe3a00000u32, 0xe12fff1e]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect(),
        );
        let input = work.join("keys.json");
        std::fs::write(&input, script).unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_brew-probe"))
            .arg("trace-start")
            .arg(module)
            .arg(mif)
            .args(["--advance", "2", "--key-script"])
            .arg(input)
            .arg("--work-dir")
            .arg(&run)
            .arg("--output")
            .arg(run.join("report.json"))
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(!run.join("runtime").exists());
        std::fs::remove_dir_all(work).unwrap();
    }
}

#[test]
fn sixty_second_diagnostic_window_is_supported_without_unbounding_steps() {
    for (limit, accepted) in [("3600", true), ("3601", false)] {
        let work = scratch();
        let result = Command::new(env!("CARGO_BIN_EXE_brew-probe"))
            .args(["synthetic", "--advance", limit, "--work-dir"])
            .arg(&work)
            .arg("--output")
            .arg(work.join("report.json"))
            .output()
            .unwrap();
        assert_eq!(result.status.success(), accepted);
        std::fs::remove_dir_all(work).unwrap();
    }
}
