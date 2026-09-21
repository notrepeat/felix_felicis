//! El núcleo no arrastra tokio ni rmcp (plan 08, «SDK y transporte»).
use std::process::Command;

#[test]
fn universo_core_no_depende_de_tokio_ni_rmcp() {
    let raiz = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let salida = Command::new(env!("CARGO"))
        .args([
            "tree",
            "-p",
            "universo-core",
            "-e",
            "normal",
            "--prefix",
            "none",
            "--locked",
        ])
        .current_dir(raiz)
        .output()
        .expect("cargo tree debe poder ejecutarse");
    assert!(
        salida.status.success(),
        "cargo tree falló: {}",
        String::from_utf8_lossy(&salida.stderr)
    );
    let arbol = String::from_utf8_lossy(&salida.stdout);
    for prohibido in ["tokio", "rmcp"] {
        let hay = arbol
            .lines()
            .any(|l| l.split_whitespace().next() == Some(prohibido));
        assert!(!hay, "universo-core arrastra `{prohibido}`:\n{arbol}");
    }
}
