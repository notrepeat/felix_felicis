use super::*;

use crate::transaccion::admision::envelope::selector::ErrorSelectorEnvelopeAdmision;

const TRANSACTION_ID: &str = "0123456789abcdef0123456789abcdef";

#[test]
fn rechaza_v2_malformado_sin_reintentar_v1_y_el_archivo_vacio_no_es_ausencia() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, false);
    let path = contexto_path(&ubicacion);
    let wire = frame(TRANSACTION_ID, parent_identity(&parent));
    let mut malformed_v2 = wire.clone();
    malformed_v2[12..14].copy_from_slice(&2_u16.to_be_bytes());
    fs::write(&path, &malformed_v2).unwrap();

    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        crate::transaccion::anclado::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(
            &mut lock,
        )
        .unwrap();
    let lease = owner.contexto_admision().unwrap();
    let error = super::lector_versionado_tests::leer_versionado(&lease).unwrap_err();
    assert!(matches!(
        error,
        super::super::ErrorLecturaContextoVersionada::Selector(ErrorSelectorEnvelopeAdmision::V2(
            _
        ))
    ));

    fs::write(&path, b"").unwrap();
    let error = super::lector_versionado_tests::leer_versionado(&lease).unwrap_err();
    assert!(matches!(
        error,
        super::super::ErrorLecturaContextoVersionada::Selector(
            ErrorSelectorEnvelopeAdmision::HeaderTruncado
        )
    ));
    assert!(!format!("{error:?}").contains("FELIX-ADMENV"));
}

#[test]
fn rechaza_reemplazo_de_mismos_bytes_con_id_nativo_distinto_y_preserva_ambos() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, false);
    let path = contexto_path(&ubicacion);
    let retained = parent.join("contexto-retained.bin");
    let expected = frame(TRANSACTION_ID, parent_identity(&parent));
    fs::write(&path, &expected).unwrap();

    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        crate::transaccion::anclado::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(
            &mut lock,
        )
        .unwrap();
    let lease = owner.contexto_admision().unwrap();
    let (archivo, _identidad_del_opener) = lease.abrir_canonico().unwrap().unwrap();
    let identidad_original = crate::transaccion::anclado::identidad_archivo(&archivo).unwrap();

    let error = super::super::lector_versionado::leer_archivo_con_limite_y_hook(
        &lease,
        archivo,
        expected.len(),
        || {
            fs::rename(&path, &retained).unwrap();
            fs::write(&path, &expected).unwrap();
        },
    )
    .unwrap_err();

    assert_eq!(
        error,
        super::super::ErrorLecturaContextoVersionada::SourceIdentityMismatch
    );
    assert_eq!(fs::read(&retained).unwrap(), expected);
    assert_eq!(fs::read(&path).unwrap(), expected);

    let retained_file = std::fs::File::open(&retained).unwrap();
    let replacement_file = std::fs::File::open(&path).unwrap();
    let retained_id = crate::transaccion::anclado::identidad_archivo(&retained_file).unwrap();
    let replacement_id = crate::transaccion::anclado::identidad_archivo(&replacement_file).unwrap();
    assert!(retained_id == identidad_original);
    assert!(replacement_id != identidad_original);
    assert!(retained_id != replacement_id);
}
