//! QI pode trocar identidade; validar o IWidget adquirido antes de inserir.
include!("common/modern_ui_fixture.rs");
#[test]
fn softkey_resolved_widget_identity_cannot_alias_its_own_private_parent() {
    const FLAG: &str = "ZEEBX_SOFTKEY_ALIAS_SUBPROCESS";
    if std::env::var_os(FLAG).is_some() {
        let mut f = F::new();
        let w = f.create(0x010551df);
        assert_eq!(f.call(w, 3, [0x800, 0x200, OUT]), 1);
        let label = f.read(OUT);
        assert_eq!(f.call(label, 8, [OUT, 0, 0]), 0);
        let parent = f.read(OUT);
        assert_ne!(parent, w);
        assert_eq!(
            f.call(w, 3, [0x801, 0x200, parent]),
            0,
            "QI do pai interno devolve o proprio widget: rejeitar antes de mudar pais"
        );
        f.call(parent, 1, [0; 3]);
        f.call(label, 1, [0; 3]);
        assert_eq!(f.call(w, 1, [0; 3]), 0);
        return;
    }
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "softkey_resolved_widget_identity_cannot_alias_its_own_private_parent",
            "--nocapture",
        ])
        .env(FLAG, "1")
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(
                status.success(),
                "insercao aliased abortou ou falhou: {status}"
            );
            break;
        }
        if std::time::Instant::now() > deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("insercao aliased entrou em recursao; processo isolado terminado apos5s");
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}
