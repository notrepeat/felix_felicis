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
    let error = aplicar_archivo_con_hooks(
        &f.root,
        &f.manifest,
        &f.invariants,
        &f.delta,
        token(&f),
        &crate::transaccion::historial::Procedencia::default(),
        || fs::write(f.root.join("payload.bin"), b"changed").unwrap(),
        || {},
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

#[test]
fn apply_exitoso_sella_el_estado_y_un_rechazo_conserva_el_sello() {
    use crate::transaccion::sello::{EstadoSello, evaluar_guardia};
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    let f = fixture();
    let viejo = token(&f);
    let result = aplicar_archivo(&f.root, &f.manifest, &f.invariants, &f.delta, viejo).unwrap();
    assert!(result.sellado);
    let guardia = evaluar_guardia(&f.root, &f.manifest, &f.invariants).unwrap();
    assert!(guardia.aprobada(), "{guardia:?}");
    // Un apply stale no publica ni toca el sello; la edición manual se detecta.
    fs::write(f.root.join("payload.bin"), b"x").unwrap();
    assert_eq!(
        aplicar_archivo(&f.root, &f.manifest, &f.invariants, &f.delta, viejo)
            .unwrap_err()
            .codigo(),
        "stale"
    );
    let guardia = evaluar_guardia(&f.root, &f.manifest, &f.invariants).unwrap();
    assert_eq!(guardia.sello, EstadoSello::Difiere);
}

#[test]
fn edicion_ajena_entre_instalar_y_sellar_no_queda_sellada() {
    use crate::transaccion::sello::{EstadoSello, evaluar_guardia};
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    let f = fixture();
    let result = aplicar_archivo_con_hooks(
        &f.root,
        &f.manifest,
        &f.invariants,
        &f.delta,
        token(&f),
        &crate::transaccion::historial::Procedencia::default(),
        || {},
        || fs::write(f.root.join("payload.bin"), b"ajeno").unwrap(),
    )
    .unwrap();
    assert_eq!(result.estado, EstadoAplicacion::Aplicada);
    assert!(!result.sellado);
    let guardia = evaluar_guardia(&f.root, &f.manifest, &f.invariants).unwrap();
    assert_eq!(guardia.sello, EstadoSello::Ausente);
}

#[test]
fn apply_registra_procedencia_y_un_rechazo_no() {
    use crate::transaccion::historial::{Procedencia, leer};
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    let f = fixture();
    let procedencia = Procedencia {
        autor: Some("Romeo".into()),
        motivo: Some("Cambia el tipo a b".into()),
    };
    let viejo = token(&f);
    let result = aplicar_archivo_con_procedencia(
        &f.root,
        &f.manifest,
        &f.invariants,
        &f.delta,
        viejo,
        &procedencia,
    )
    .unwrap();
    assert!(result.registrado);
    let entradas = leer(&f.root).unwrap().entradas;
    assert_eq!(entradas.len(), 1);
    assert_eq!(entradas[0].autor.as_deref(), Some("Romeo"));
    assert_eq!(entradas[0].motivo.as_deref(), Some("Cambia el tipo a b"));
    assert!(entradas[0].sellado);
    // Un apply stale no publica y no deja entrada.
    assert!(aplicar_archivo(&f.root, &f.manifest, &f.invariants, &f.delta, viejo).is_err());
    assert_eq!(leer(&f.root).unwrap().entradas.len(), 1);
    // Procedencia inválida se rechaza antes de tocar nada.
    let mala = Procedencia {
        autor: Some("dos\nlíneas".into()),
        motivo: None,
    };
    assert_eq!(
        aplicar_archivo_con_procedencia(
            &f.root,
            &f.manifest,
            &f.invariants,
            &f.delta,
            viejo,
            &mala
        )
        .unwrap_err()
        .codigo(),
        "provenance_invalid"
    );
}

#[test]
fn apply_rechazado_por_invariantes_no_registra_ni_sella() {
    use crate::transaccion::historial::leer;
    let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
    let f = fixture();
    fs::write(&f.invariants, "(Type @n/node a)\n").unwrap();
    let result = aplicar_archivo(&f.root, &f.manifest, &f.invariants, &f.delta, token(&f)).unwrap();
    assert_eq!(result.estado, EstadoAplicacion::Rechazada);
    assert!(!result.registrado && !result.sellado);
    assert_eq!(leer(&f.root).unwrap(), Default::default());
}
