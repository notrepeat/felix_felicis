use super::*;
use crate::transaccion::simulacion::simular_archivo;
use std::fs;

struct Fixture {
    dir: tempfile::TempDir,
    root: std::path::PathBuf,
    manifest: std::path::PathBuf,
    invariants: std::path::PathBuf,
    delta: std::path::PathBuf,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("bundle");
    fs::create_dir_all(root.join("n")).unwrap();
    fs::write(
        root.join("n/node.md"),
        "---\ntype: a\ntitle: Node\n---\nbody\n",
    )
    .unwrap();
    fs::write(root.join("payload.bin"), b"original").unwrap();
    let manifest = dir.path().join("manifest.toml");
    fs::write(
        &manifest,
        "version = \"t/1\"\n[nodos]\nsortes = [\"a\", \"b\", \"Retirado\", \"Patrón\"]\n[layout]\ndirectorios = [\"n\"]\n",
    )
    .unwrap();
    let invariants = dir.path().join("invariants.sx");
    fs::write(&invariants, "(Type @n/node b)\n").unwrap();
    let delta = dir.path().join("delta.json");
    fs::write(
        &delta,
        r#"{"schema":"felix-delta-v1","add":["Type(n/node, b)"],"remove":["Type(n/node, a)"]}"#,
    )
    .unwrap();
    Fixture {
        dir,
        root,
        manifest,
        invariants,
        delta,
    }
}

fn token(f: &Fixture) -> SnapshotToken {
    simular_archivo(&f.root, &f.manifest, &f.invariants, &f.delta)
        .unwrap()
        .snapshot_token()
}

fn stage_siblings(f: &Fixture) -> Vec<String> {
    fs::read_dir(f.dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".felix.stage."))
        .collect()
}

#[test]
fn fallo_de_preparacion_tras_crear_stage_lo_limpia_y_no_bloquea_el_siguiente_apply() {
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    let f = fixture();
    let error = aplicar_archivo_con_hook(
        &f.root,
        &f.manifest,
        &f.invariants,
        &f.delta,
        token(&f),
        || fs::write(f.root.join("payload.bin"), b"changed").unwrap(),
    )
    .unwrap_err();
    assert_eq!(error.codigo(), "stale");
    assert_eq!(stage_siblings(&f), Vec::<String>::new());
    let result = aplicar_archivo(&f.root, &f.manifest, &f.invariants, &f.delta, token(&f))
        .expect("el stage de la preparación fallida no debe bloquear el apply siguiente");
    assert_eq!(result.estado, EstadoAplicacion::Aplicada);
    assert!(
        fs::read_to_string(f.root.join("n/node.md"))
            .unwrap()
            .contains("type: b")
    );
}

#[test]
fn apply_sincroniza_stage_antes_de_instalar_y_cada_cambio_de_nombres() {
    use crate::transaccion::durabilidad::{Punto, registro};
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    let f = fixture();
    let esperado = token(&f);
    registro::tomar();
    let result = aplicar_archivo(&f.root, &f.manifest, &f.invariants, &f.delta, esperado).unwrap();
    assert_eq!(result.estado, EstadoAplicacion::Aplicada);
    let eventos = registro::tomar();
    let posicion = |punto: Punto| eventos.iter().position(|e| *e == punto);
    let ultima = |punto: Punto| eventos.iter().rposition(|e| *e == punto);
    let contar = |punto: Punto| eventos.iter().filter(|e| **e == punto).count();
    // n/node.md y payload.bin; directorios: raíz del stage y n/.
    assert_eq!(contar(Punto::StageArchivo), 2, "{eventos:?}");
    assert_eq!(contar(Punto::StageDirectorio), 2, "{eventos:?}");
    // root -> backup y stage -> root.
    assert_eq!(contar(Punto::RenombrarDirectorio), 2, "{eventos:?}");
    let contenido_listo = ultima(Punto::StageDirectorio).unwrap();
    assert!(ultima(Punto::StageArchivo).unwrap() < contenido_listo);
    assert!(
        contenido_listo < posicion(Punto::JournalPublicado).unwrap(),
        "{eventos:?}"
    );
    assert!(
        posicion(Punto::JournalPublicado).unwrap() < posicion(Punto::RenombrarDirectorio).unwrap()
    );
    // stage -> root (segundo rename) es durable antes del sucesor Committed.
    let renombrados: Vec<_> = eventos
        .iter()
        .enumerate()
        .filter(|(_, e)| **e == Punto::RenombrarDirectorio)
        .map(|(i, _)| i)
        .collect();
    assert!(
        renombrados[1] < ultima(Punto::JournalSucesor).unwrap(),
        "{eventos:?}"
    );
    // El backup se borra solo después de que el sucesor Committed sea durable.
    assert!(
        ultima(Punto::JournalSucesor).unwrap() < posicion(Punto::BackupEliminado).unwrap(),
        "{eventos:?}"
    );
    for punto in [
        Punto::ContextoPublicado,
        Punto::IntentPublicado,
        Punto::NombreRetirado,
    ] {
        assert!(contar(punto) >= 1, "falta {punto:?} en {eventos:?}");
    }
}

#[test]
fn fsync_fallido_del_sucesor_no_borra_backup_y_recover_completa() {
    use crate::transaccion::durabilidad::{Punto, registro};
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    let f = fixture();
    let esperado = token(&f);
    // BackupReady, Installed y Committed publican sucesor; falla el primero.
    registro::fallar_una_vez(Punto::JournalSucesor);
    let error = aplicar_archivo(&f.root, &f.manifest, &f.invariants, &f.delta, esperado)
        .expect_err("un sucesor no durable no puede darse por confirmado");
    assert!(error.conserva_evidencia(), "{}", error.codigo());
    drop(error);
    registro::tomar();
    assert!(
        !crate::transaccion::estado::inspeccionar(&f.root)
            .unwrap()
            .limpio()
    );
    crate::transaccion::estado::recuperar(&f.root).unwrap();
    assert!(
        crate::transaccion::estado::inspeccionar(&f.root)
            .unwrap()
            .limpio()
    );
    // Recover revierte o completa; en ambos casos el root es un estado válido.
    let nodo = fs::read_to_string(f.root.join("n/node.md")).unwrap();
    assert!(
        nodo.contains("type: a") || nodo.contains("type: b"),
        "{nodo}"
    );
}
