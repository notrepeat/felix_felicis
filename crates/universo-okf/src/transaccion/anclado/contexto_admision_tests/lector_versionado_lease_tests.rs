use super::*;

use crate::transaccion::admision::envelope::selector::{
    EnvelopeAdmisionVersionada, ErrorSelectorEnvelopeAdmision,
};
use crate::transaccion::admision::envelope::v2::EnvelopeContextoAdmisionV2;
use crate::transaccion::journal::v2::JournalV2;
use std::cell::Cell;

const TRANSACTION_ID: &str = "0123456789abcdef0123456789abcdef";

fn frame_v2(parent: DirectoryIdentity) -> Vec<u8> {
    let old = crate::transaccion::journal::ArtifactFingerprint::new(
        super::identity(10),
        crate::transaccion::journal::RawTreeDigest::from_hex("a".repeat(64)).unwrap(),
    );
    let new = crate::transaccion::journal::ArtifactFingerprint::new(
        super::identity(11),
        crate::transaccion::journal::RawTreeDigest::from_hex("b".repeat(64)).unwrap(),
    );
    let journal = JournalV2::new(TRANSACTION_ID, parent, old, new).unwrap();
    EnvelopeContextoAdmisionV2::ligar(super::envelope_fixtures::contexto(b"\n"), &journal)
        .encode()
        .unwrap()
}

fn verificar_variante_v1_o_v2(es_v2: bool) {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = super::ubicacion(&parent, false);
    let path = super::contexto_path(&ubicacion);
    let expected = if es_v2 {
        frame_v2(super::parent_identity(&parent))
    } else {
        super::frame(TRANSACTION_ID, super::parent_identity(&parent))
    };
    fs::write(&path, &expected).unwrap();

    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        crate::transaccion::anclado::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(
            &mut lock,
        )
        .unwrap();
    let lease = owner.contexto_admision().unwrap();
    let observacion = lease.observar_versionado().unwrap().unwrap();

    match (&observacion.envelope, es_v2) {
        (EnvelopeAdmisionVersionada::V1(_), false) | (EnvelopeAdmisionVersionada::V2(_), true) => {}
        _ => panic!("la entrada del lease debe conservar la variante exterior"),
    }
    assert_eq!(
        observacion.evidencia.longitud,
        u64::try_from(expected.len()).unwrap()
    );
    let digest: [u8; 32] = Sha256::digest(&expected).into();
    assert_eq!(observacion.evidencia.sha256, digest);
    assert_eq!(fs::read(&path).unwrap(), expected);
}

#[test]
fn entrada_lease_conserva_v1_y_v2_con_evidencia_exacta() {
    verificar_variante_v1_o_v2(false);
    verificar_variante_v1_o_v2(true);
}

#[test]
fn ausencia_se_confirma_tras_dos_sondas_parent_y_hook_por_llamada() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = super::ubicacion(&parent, false);
    let path = super::contexto_path(&ubicacion);

    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        crate::transaccion::anclado::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(
            &mut lock,
        )
        .unwrap();
    let lease = owner.contexto_admision().unwrap();
    let llamadas = Cell::new(0_u8);
    let observado = lease
        .observar_versionado_con_hook(|| llamadas.set(llamadas.get() + 1))
        .unwrap();

    assert!(observado.is_none());
    assert_eq!(llamadas.get(), 1);
    assert!(!path.exists());
}

#[test]
fn la_reaparicion_entre_sondas_es_error_y_preserva_el_canario() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = super::ubicacion(&parent, false);
    let path = super::contexto_path(&ubicacion);
    let canary = super::frame(TRANSACTION_ID, super::parent_identity(&parent));

    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        crate::transaccion::anclado::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(
            &mut lock,
        )
        .unwrap();
    let lease = owner.contexto_admision().unwrap();
    let resultado = lease.observar_versionado_con_hook(|| fs::write(&path, &canary).unwrap());

    assert!(matches!(
        resultado,
        Err(super::super::ErrorLecturaContextoVersionada::Lease(
            super::super::ErrorContextoAdmisionAnclado::CanonicalUnverified
        ))
    ));
    assert_eq!(fs::read(&path).unwrap(), canary);
}

#[test]
fn error_en_la_segunda_sonda_no_se_convierte_en_ausencia() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = super::ubicacion(&parent, false);
    let path = super::contexto_path(&ubicacion);

    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        crate::transaccion::anclado::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(
            &mut lock,
        )
        .unwrap();
    let lease = owner.contexto_admision().unwrap();
    let resultado = lease.observar_versionado_con_hook(|| fs::create_dir(&path).unwrap());

    // Windows puede devolver Io al abrir un directorio antes de clasificar su tipo.
    assert!(matches!(
        resultado,
        Err(super::super::ErrorLecturaContextoVersionada::Lease(
            super::super::ErrorContextoAdmisionAnclado::EntryRejected
                | super::super::ErrorContextoAdmisionAnclado::Io
        ))
    ));
    assert!(path.is_dir());
}

#[test]
fn archivo_regular_vacio_es_error_y_no_ausencia() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = super::ubicacion(&parent, false);
    let path = super::contexto_path(&ubicacion);
    fs::write(&path, b"").unwrap();

    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        crate::transaccion::anclado::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(
            &mut lock,
        )
        .unwrap();
    let lease = owner.contexto_admision().unwrap();
    let resultado = lease.observar_versionado();

    assert!(matches!(
        resultado,
        Err(super::super::ErrorLecturaContextoVersionada::Selector(
            ErrorSelectorEnvelopeAdmision::HeaderTruncado
        ))
    ));
    assert!(path.is_file());
}
