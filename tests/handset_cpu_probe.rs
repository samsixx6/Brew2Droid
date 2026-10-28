use zeebx::handset::probe::{run_cpu_probe, write_capability_report};

#[test]
fn arm_thumb_e_falha_de_memoria_passam_pelo_backend_real() {
    let report = run_cpu_probe().expect("o backend nativo executa a sonda");
    assert_eq!(report.arm_result, 42);
    assert_eq!(report.thumb_result, 8);
    assert_eq!(report.unmapped_fault_addr, 0x3000);
    assert!(report.instructions > 0);
}

#[test]
fn o_relatorio_preserva_strings_do_driver_e_resultados_reais() {
    let cpu = run_cpu_probe().unwrap();
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".superpowers");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join(format!("capability-test-{}.json", std::process::id()));
    write_capability_report(
        &path,
        16384,
        "vendor",
        "GPU \"quoted\"",
        "OpenGL ES 3.2",
        [64, 128, 192, 255],
        &cpu,
    )
    .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["native_page_size_bytes"], 16384);
    assert_eq!(report["graphics"]["renderer"], "GPU \"quoted\"");
    assert_eq!(report["graphics"]["triangle_probe_passed"], true);
    assert_eq!(report["cpu_probe"]["arm_result"], 42);
    assert_eq!(report["cpu_probe"]["thumb_result"], 8);
    assert_eq!(report["cpu_probe"]["memory_protection_passed"], true);
    assert_eq!(report["game_compatibility_tested"], false);
}
