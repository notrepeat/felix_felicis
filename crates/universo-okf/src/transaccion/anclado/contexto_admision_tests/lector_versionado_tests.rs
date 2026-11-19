use super::*;

use crate::transaccion::admision::envelope::selector::EnvelopeAdmisionVersionada;
use crate::transaccion::admision::envelope::v2::EnvelopeContextoAdmisionV2;
use crate::transaccion::journal::v2::JournalV2;
use crate::transaccion::journal::{ArtifactFingerprint, RawTreeDigest};
use std::cell::Cell;
use std::fs::File;
use std::io::Write;

const TRANSACTION_ID: &str = "0123456789abcdef0123456789abcdef";

fn abrir_archivo(lease: &super::super::ContextoAdmisionAnclado<'_>) -> File {
    let Some((archivo, _identidad_del_opener)) = lease.abrir_canonico().unwrap() else {
        panic!("la fixture presente debe producir un handle");
    };
    archivo
}

pub(super) fn leer_versionado(
    lease: &super::super::ContextoAdmisionAnclado<'_>,
) -> Result<super::super::ObservacionContextoVersionada, super::super::ErrorLecturaContextoVersionada>
{
    let archivo = abrir_archivo(lease);
    super::super::lector_versionado::leer_archivo_versionado(lease, archivo)
}

fn frame_v2(parent: DirectoryIdentity) -> Vec<u8> {
    let old = ArtifactFingerprint::new(
        identity(10),
        RawTreeDigest::from_hex("a".repeat(64)).unwrap(),
    );
    let new = ArtifactFingerprint::new(
        identity(11),
        RawTreeDigest::from_hex("b".repeat(64)).unwrap(),
    );
    let journal = JournalV2::new(TRANSACTION_ID, parent, old, new).unwrap();
    EnvelopeContextoAdmisionV2::ligar(envelope_fixtures::contexto(b"\n"), &journal)
        .encode()
        .unwrap()
}

fn verificar_observacion(es_v2: bool) {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, false);
    let parent_id = parent_identity(&parent);
    let expected = if es_v2 {
        frame_v2(parent_id)
    } else {
        frame(TRANSACTION_ID, parent_id)
    };
    fs::write(contexto_path(&ubicacion), &expected).unwrap();

    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        crate::transaccion::anclado::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(
            &mut lock,
        )
        .unwrap();
    let lease = owner.contexto_admision().unwrap();
    let observacion = leer_versionado(&lease).unwrap();

    match (&observacion.envelope, es_v2) {
        (EnvelopeAdmisionVersionada::V1(_), false) | (EnvelopeAdmisionVersionada::V2(_), true) => {}
        _ => panic!("el selector debe conservar la variante exterior"),
    }
    assert_eq!(
        observacion.evidencia.longitud,
        u64::try_from(expected.len()).unwrap()
    );
    let digest: [u8; 32] = Sha256::digest(&expected).into();
    assert_eq!(observacion.evidencia.sha256, digest);
    assert_eq!(
        observacion.evidencia.identidad_archivo,
        crate::transaccion::anclado::identidad_archivo(&observacion.archivo_original).unwrap()
    );
    assert_eq!(fs::read(contexto_path(&ubicacion)).unwrap(), expected);
    assert_eq!(
        format!("{observacion:?}"),
        "ObservacionContextoVersionada { .. }"
    );
}

#[test]
fn lee_v1_y_v2_en_el_mismo_handle_con_evidencia_exacta() {
    verificar_observacion(false);
    verificar_observacion(true);
}

#[test]
fn binding_de_parent_distinto_se_rechaza_antes_de_devolver_datos() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, false);
    fs::write(
        contexto_path(&ubicacion),
        frame(TRANSACTION_ID, identity(900)),
    )
    .unwrap();

    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        crate::transaccion::anclado::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(
            &mut lock,
        )
        .unwrap();
    let lease = owner.contexto_admision().unwrap();
    let error = leer_versionado(&lease).unwrap_err();

    assert_eq!(
        error,
        super::super::ErrorLecturaContextoVersionada::ParentBindingMismatch
    );
    assert_eq!(error.to_string(), error.codigo());
    assert!(!format!("{error:?}").contains("0123456789abcdef"));
}

#[test]
fn metadata_early_y_byte_sentinela_rechazan_sobrecuota() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, false);
    let path = contexto_path(&ubicacion);
    let expected = frame(TRANSACTION_ID, parent_identity(&parent));
    fs::write(&path, &expected).unwrap();

    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        crate::transaccion::anclado::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(
            &mut lock,
        )
        .unwrap();
    let lease = owner.contexto_admision().unwrap();
    let hook_called = Cell::new(false);
    let error = super::super::lector_versionado::leer_archivo_con_limite_y_hook(
        &lease,
        abrir_archivo(&lease),
        expected.len() - 1,
        || hook_called.set(true),
    )
    .unwrap_err();
    assert_eq!(
        error,
        super::super::ErrorLecturaContextoVersionada::FrameTooLarge
    );
    assert!(!hook_called.get());

    let error = super::super::lector_versionado::leer_archivo_con_limite_y_hook(
        &lease,
        abrir_archivo(&lease),
        expected.len(),
        || {
            std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap()
                .write_all(b"x")
                .unwrap();
        },
    )
    .unwrap_err();
    assert_eq!(
        error,
        super::super::ErrorLecturaContextoVersionada::FrameTooLarge
    );
}
