fn main() {
    // **O ícone do `.exe` não mora aqui.** Um recurso do Windows só chega ao executável se for
    // compilado no pacote que o produz: o `frontends/qt-standalone` e o `frontends/cli`.
    println!("cargo:rerun-if-changed=build.rs");
    // O `frontends/switch/compilar.sh` passa `--cfg zeebx_switch`. Sem este aviso o rustc
    // trata o nome como desconhecido.
    println!("cargo::rustc-check-cfg=cfg(zeebx_switch)");
    println!("cargo:rerun-if-changed=third-party/qcelp/qcelp.c");
    println!("cargo:rerun-if-changed=third-party/qcelp/qcelpdata.h");
    println!("cargo:rerun-if-changed=third-party/qcelp/support.h");
    cc::Build::new().file("third-party/qcelp/qcelp.c")
        .include("third-party/qcelp").flag_if_supported("-std=c11")
        .flag_if_supported("-ffp-contract=off").compile("brew_qcelp");
}
