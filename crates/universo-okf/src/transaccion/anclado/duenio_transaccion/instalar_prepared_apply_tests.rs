//! El mismo caller público debe recuperar root ausente antes de leer config interna.
use super::super::preparar_apply::{
    ResultadoPreparacionApply,
    tests::{config, delta, fixture},
};
use super::*;
use crate::transaccion::{
    captura::capturar_snapshot_soportado_anclado, lock::LockEscritura, snapshot::SnapshotToken,
};
use std::fs;
#[test]
#[ignore = "writer exits after moving Old to backup"]
fn proceso_apply_config_interna_corte() {
    let root = std::env::var_os("FELIX_TEST_INTERNAL_CONFIG_ROOT").unwrap();
    let expected =
        SnapshotToken::from_hex(&std::env::var("FELIX_TEST_INTERNAL_CONFIG_TOKEN").unwrap())
            .unwrap();
    let mut lock = LockEscritura::adquirir(std::path::Path::new(&root)).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let snapshot = capturar_snapshot_soportado_anclado(
        &owner.abrir_root_lectura().unwrap().unwrap(),
        config(),
    )
    .unwrap();
    assert!(snapshot.token() == expected);
    let ResultadoPreparacionApply::Preparada(prepared) =
        owner.preparar_apply(config(), expected, &delta()).unwrap()
    else {
        panic!("reject");
    };
    let result = prepared
        .publicar_prepared()
        .unwrap()
        .instalar_con_hook(|step| {
            if step == 1 {
                std::process::exit(105);
            }
        });
    assert!(result.is_err(), "cut not reached");
}
#[test]
fn aplicar_mismo_comando_recupera_root_ausente_y_config_interna_antes_cas() {
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    let (dir, location) = fixture();
    let manifest = location.ruta().join("manifest.toml");
    let invariants = location.ruta().join("invariants.sx");
    fs::write(&manifest, config().manifiesto_raw()).unwrap();
    fs::write(&invariants, config().invariantes_raw()).unwrap();
    let delta_path = dir.path().join("delta.json");
    fs::write(
        &delta_path,
        r#"{"schema":"felix-delta-v1","add":["Type(n/node, b)"],"remove":["Type(n/node, a)"]}"#,
    )
    .unwrap();
    let expected = crate::transaccion::simulacion::simular_archivo(
        location.ruta(),
        &manifest,
        &invariants,
        &delta_path,
    )
    .unwrap()
    .snapshot_token();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "transaccion::anclado::duenio_transaccion::instalar_prepared::apply_recovery_tests::proceso_apply_config_interna_corte", "--ignored", "--nocapture"])
        .env("FELIX_TEST_INTERNAL_CONFIG_ROOT", location.ruta())
        .env("FELIX_TEST_INTERNAL_CONFIG_TOKEN", expected.to_hex())
        .output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(105),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!location.ruta().exists());
    assert!(location.backup().as_path().exists());
    let result = crate::transaccion::aplicar::aplicar_archivo(
        location.ruta(),
        &manifest,
        &invariants,
        &delta_path,
        expected,
    )
    .unwrap();
    assert_eq!(
        result.estado,
        crate::transaccion::aplicar::EstadoAplicacion::Aplicada
    );
    assert!(
        fs::read_to_string(location.ruta().join("n/node.md"))
            .unwrap()
            .contains("type: b")
    );
    assert_eq!(fs::read(&manifest).unwrap(), config().manifiesto_raw());
    assert_eq!(fs::read(&invariants).unwrap(), config().invariantes_raw());
    assert!(!location.backup().as_path().exists());
    assert!(!location.journal().as_path().exists());
}
