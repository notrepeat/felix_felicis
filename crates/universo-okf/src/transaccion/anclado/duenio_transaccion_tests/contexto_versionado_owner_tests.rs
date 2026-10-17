use super::*;

use crate::transaccion::admision::envelope::v2::EnvelopeContextoAdmisionV2;
use crate::transaccion::admision::envelope::{self, EnvelopeContextoAdmision, codec as codec_v1};
use crate::transaccion::anclado::contexto_admision::{
    ErrorContextoAdmisionAnclado, ErrorLecturaContextoVersionada,
};
use crate::transaccion::journal::v2::JournalV2;
use crate::transaccion::journal::{DirectoryIdentity, Journal};
use crate::transaccion::lock::LockEscritura;
use crate::transaccion::raiz::UbicacionRaiz;
use std::cell::Cell;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

const TRANSACTION_ID: &str = "0123456789abcdef0123456789abcdef";
const LOCK_CANARY: &[u8] = b"ctx4 owner wrapper lock canary";

struct RutasFixture {
    ubicacion: UbicacionRaiz,
    contexto: PathBuf,
    lock: PathBuf,
}

fn preparar_rutas(temp: &tempfile::TempDir) -> RutasFixture {
    let parent = temp.path().join("parent");
    fs::create_dir_all(&parent).unwrap();
    let ubicacion = UbicacionRaiz::nueva(&parent.join("bundle")).unwrap();
    let mut nombre_contexto = OsString::from(".");
    nombre_contexto.push(ubicacion.ruta().file_name().unwrap());
    nombre_contexto.push(".felix.admctx");
    let contexto = parent.join(nombre_contexto);
    let lock = ubicacion.lock().as_path().to_path_buf();
    fs::write(&lock, LOCK_CANARY).unwrap();
    RutasFixture {
        ubicacion,
        contexto,
        lock,
    }
}

fn frame_v1(parent: DirectoryIdentity) -> Vec<u8> {
    let journal = Journal::new(
        TRANSACTION_ID,
        parent,
        fingerprint_journal_prueba(2, 'a'),
        fingerprint_journal_prueba(3, 'b'),
    )
    .unwrap();
    let envelope = EnvelopeContextoAdmision::ligar(envelope::tests::contexto(b"\n"), &journal);
    codec_v1::codificar(&envelope).unwrap()
}

fn frame_v2(parent: DirectoryIdentity) -> Vec<u8> {
    let journal = JournalV2::new(
        TRANSACTION_ID,
        parent,
        fingerprint_journal_prueba(2, 'a'),
        fingerprint_journal_prueba(3, 'b'),
    )
    .unwrap();
    EnvelopeContextoAdmisionV2::ligar(envelope::tests::contexto(b"\n"), &journal)
        .encode()
        .unwrap()
}

fn verificar_lock_y_contexto(rutas: &RutasFixture, contexto_esperado: Option<&[u8]>) {
    assert_eq!(fs::read(&rutas.lock).unwrap(), LOCK_CANARY);
    match contexto_esperado {
        Some(bytes) => assert_eq!(fs::read(&rutas.contexto).unwrap(), bytes),
        None => assert!(!rutas.contexto.exists()),
    }
}

#[test]
fn owner_observa_frame_v1_y_preserva_lock_y_bytes() {
    let temp = tempdir().unwrap();
    let rutas = preparar_rutas(&temp);
    let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let frame = frame_v1(owner.identidad_parent.journal_identity().unwrap());
    fs::write(&rutas.contexto, &frame).unwrap();

    let observacion = owner.observar_contexto_versionado().unwrap().unwrap();

    assert_eq!(
        format!("{observacion:?}"),
        "ObservacionContextoVersionada { .. }"
    );
    drop(observacion);
    drop(owner);
    drop(lock);
    verificar_lock_y_contexto(&rutas, Some(&frame));
}

#[test]
fn owner_observa_frame_v2_y_preserva_lock_y_bytes() {
    let temp = tempdir().unwrap();
    let rutas = preparar_rutas(&temp);
    let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let frame = frame_v2(owner.identidad_parent.journal_identity().unwrap());
    fs::write(&rutas.contexto, &frame).unwrap();

    let observacion = owner.observar_contexto_versionado().unwrap().unwrap();

    assert_eq!(
        format!("{observacion:?}"),
        "ObservacionContextoVersionada { .. }"
    );
    drop(observacion);
    drop(owner);
    drop(lock);
    verificar_lock_y_contexto(&rutas, Some(&frame));
}

#[test]
fn owner_confirma_ausencia_y_suelta_guardias_antes_de_leer_lock_canary() {
    let temp = tempdir().unwrap();
    let rutas = preparar_rutas(&temp);
    let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

    let observacion = owner.observar_contexto_versionado().unwrap();

    assert!(observacion.is_none());
    drop(owner);
    drop(lock);
    verificar_lock_y_contexto(&rutas, None);
}

#[test]
fn precheck_owner_invalido_no_alcanza_lectura_ni_hook() {
    let temp = tempdir().unwrap();
    let rutas = preparar_rutas(&temp);
    let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let bytes = b"invalid frame must not be read";
    fs::write(&rutas.contexto, bytes).unwrap();
    owner.scope_parent = super::super::super::ScopeFilesystem(owner.scope_parent.0.wrapping_add(1));
    let hook_llamado = Cell::new(false);

    let error = owner
        .observar_contexto_versionado_con_prueba(|| hook_llamado.set(true))
        .unwrap_err();

    assert_eq!(
        error,
        ErrorLecturaContextoVersionada::Lease(ErrorContextoAdmisionAnclado::OwnerUnverified)
    );
    assert!(!hook_llamado.get());
    drop(owner);
    drop(lock);
    verificar_lock_y_contexto(&rutas, Some(bytes));
}

#[derive(Clone, Copy)]
enum EstadoLectura {
    Some,
    None,
    Error,
}

fn verificar_posterror_domina(estado: EstadoLectura) {
    let temp = tempdir().unwrap();
    let rutas = preparar_rutas(&temp);
    let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let contexto = match estado {
        EstadoLectura::Some => Some(frame_v1(owner.identidad_parent.journal_identity().unwrap())),
        EstadoLectura::None => None,
        EstadoLectura::Error => Some(Vec::new()),
    };
    if let Some(bytes) = &contexto {
        fs::write(&rutas.contexto, bytes).unwrap();
    }
    if matches!(estado, EstadoLectura::Error) {
        let lectura = {
            let lease = owner.contexto_admision().unwrap();
            lease.observar_versionado()
        };
        assert!(matches!(
            lectura,
            Err(ErrorLecturaContextoVersionada::Selector(_))
        ));
    }

    let error = owner
        .observar_contexto_versionado_con_posterror_prueba(
            crate::transaccion::anclado::journal_temporal::ErrorJournalAnclado::IdentityMismatch,
        )
        .unwrap_err();

    assert_eq!(
        error,
        ErrorLecturaContextoVersionada::Lease(ErrorContextoAdmisionAnclado::OwnerUnverified)
    );
    drop(owner);
    drop(lock);
    verificar_lock_y_contexto(&rutas, contexto.as_deref());
}

#[test]
fn posterror_sintetico_descarta_some_y_preserva_archivo() {
    verificar_posterror_domina(EstadoLectura::Some);
}

#[test]
fn posterror_sintetico_descarta_none_y_preserva_ausencia() {
    verificar_posterror_domina(EstadoLectura::None);
}

#[test]
fn posterror_sintetico_domina_error_de_lectura_y_preserva_bytes() {
    verificar_posterror_domina(EstadoLectura::Error);
}
