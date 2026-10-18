use super::*;

use crate::transaccion::admision::envelope::selector::EnvelopeAdmisionVersionada;
use crate::transaccion::admision::envelope::v2::{
    EnvelopeContextoAdmisionV2, ErrorCodecEnvelopeContextoAdmisionV2,
};
use crate::transaccion::journal::v2::JournalV2;

const TRANSACTION_ID: &str = "0123456789abcdef0123456789abcdef";

fn envelope_v2(parent: DirectoryIdentity) -> (EnvelopeContextoAdmisionV2, JournalV2) {
    let old = ArtifactFingerprint::new(
        identity(2),
        RawTreeDigest::from_hex("a".repeat(64)).unwrap(),
    );
    let new = ArtifactFingerprint::new(
        identity(3),
        RawTreeDigest::from_hex("b".repeat(64)).unwrap(),
    );
    let journal = JournalV2::new(TRANSACTION_ID, parent, old, new).unwrap();
    let envelope = EnvelopeContextoAdmisionV2::ligar(envelope_fixtures::contexto(b"\n"), &journal);
    (envelope, journal)
}

#[test]
fn publica_contexto_v2_tipado_y_relee_binding_bytes_identidad_y_lock_canary() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let (envelope, journal) = envelope_v2(parent_identity(&parent));
    let esperado = envelope.encode().unwrap();
    let temporal = temporal_contexto_path(&ubicacion);
    let canonico = contexto_path(&ubicacion);
    let lock_path = ubicacion.lock().as_path().to_path_buf();
    let lock_canary = b"preserve typed context lock";
    let digest_esperado: [u8; 32] = Sha256::digest(&esperado).into();
    fs::write(&lock_path, lock_canary).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock)
            .unwrap();

    let recibo = owner
        .contexto_admision()
        .unwrap()
        .crear_temporal_v2(&envelope)
        .unwrap();
    let id_temporal = recibo.datos.identidad_archivo;
    assert_eq!(fs::read(&temporal).unwrap(), esperado);
    assert!(!canonico.exists());
    assert_eq!(recibo.datos.longitud, esperado.len());
    assert_eq!(recibo.datos.digest, digest_esperado);

    let debug_publicado = format!("{:?}", recibo.verificar().unwrap().publicar().unwrap());
    assert_eq!(debug_publicado, "ReciboContextoPublicado { .. }");
    assert_eq!(fs::read(&canonico).unwrap(), esperado);
    assert!(!temporal.exists());

    let (id_canonico, longitud, sha256, binding_v2) = {
        let lease = owner.contexto_admision().unwrap();
        let observacion = lease.observar_versionado().unwrap().unwrap();
        let (id, longitud, sha256) = observacion.evidencia_raw_retiro();
        let binding_v2 = match observacion.envelope_versionado() {
            EnvelopeAdmisionVersionada::V2(parsed) => {
                parsed.coincide_estructuralmente(&journal).is_ok()
            }
            EnvelopeAdmisionVersionada::V1(_) => false,
        };
        (id, longitud, sha256, binding_v2)
    };
    assert_eq!(id_canonico, id_temporal);
    assert_eq!(
        id_canonico,
        identidad_archivo(&File::open(&canonico).unwrap()).unwrap()
    );
    assert_eq!(longitud, u64::try_from(esperado.len()).unwrap());
    assert_eq!(sha256, digest_esperado);
    assert!(binding_v2);

    drop(owner);
    drop(lock);
    assert_eq!(
        fs::read(&lock_path).unwrap().as_slice(),
        lock_canary.as_slice()
    );
}

#[test]
fn codec_v2_fallido_no_crea_ni_modifica_temporal_o_canonico() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let (envelope, _) = envelope_v2(parent_identity(&parent));
    let temporal = temporal_contexto_path(&ubicacion);
    let canonico = contexto_path(&ubicacion);
    let canary = b"preserve existing temporary";
    fs::write(&temporal, canary).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock)
            .unwrap();

    let error = owner
        .contexto_admision()
        .unwrap()
        .crear_temporal_v2_con(&envelope, |_| {
            assert_eq!(fs::read(&temporal).unwrap(), canary);
            Err(ErrorCodecEnvelopeContextoAdmisionV2::ReservationFailed)
        })
        .unwrap_err();

    assert_eq!(
        error,
        ErrorContextoAdmisionAnclado::CodecV2(
            ErrorCodecEnvelopeContextoAdmisionV2::ReservationFailed
        )
    );
    assert_eq!(error.codigo(), "admission_envelope_v2_reservation_failed");
    assert_eq!(
        format!("{error:?}"),
        "ErrorContextoAdmisionAnclado(\"admission_envelope_v2_reservation_failed\")"
    );
    assert_eq!(fs::read(&temporal).unwrap().as_slice(), canary.as_slice());
    assert!(!canonico.exists());
}

#[test]
fn temporal_v2_ocupado_falla_cerrado_y_preserva_canary() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, false);
    let (envelope, _) = envelope_v2(parent_identity(&parent));
    let temporal = temporal_contexto_path(&ubicacion);
    let canary = b"preserve occupied temporal";
    fs::write(&temporal, canary).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock)
            .unwrap();

    let error = owner
        .contexto_admision()
        .unwrap()
        .crear_temporal_v2(&envelope)
        .unwrap_err();

    assert_eq!(error, ErrorContextoAdmisionAnclado::TemporaryExists);
    assert_eq!(fs::read(&temporal).unwrap().as_slice(), canary.as_slice());
    assert!(!contexto_path(&ubicacion).exists());
}

#[test]
fn canonic_v2_conflictivo_no_reemplaza_ni_limpia_el_temporal() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let (envelope, _) = envelope_v2(parent_identity(&parent));
    let esperado = envelope.encode().unwrap();
    let temporal = temporal_contexto_path(&ubicacion);
    let canonico = contexto_path(&ubicacion);
    let canary = b"preserve canonical v2 canary";
    fs::write(&canonico, canary).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock)
            .unwrap();

    let error = owner
        .contexto_admision()
        .unwrap()
        .crear_temporal_v2(&envelope)
        .unwrap()
        .verificar()
        .unwrap()
        .publicar()
        .unwrap_err();

    assert_eq!(error, ErrorContextoAdmisionAnclado::PublicationUnverified);
    assert_eq!(fs::read(&canonico).unwrap().as_slice(), canary.as_slice());
    assert_eq!(fs::read(&temporal).unwrap(), esperado);
}

#[test]
fn guard_recibo_v2_exige_mismo_token_y_misma_identidad_aunque_bytes_coincidan() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let (envelope, journal) = envelope_v2(parent_identity(&parent));
    let esperado = envelope.encode().unwrap();
    let canonico = contexto_path(&ubicacion);
    let lock_path = ubicacion.lock().as_path().to_path_buf();
    let lock_canary = b"preserve context receipt lock";
    fs::write(&lock_path, lock_canary).unwrap();

    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock)
            .unwrap();
    let recibo = owner
        .contexto_admision()
        .unwrap()
        .crear_temporal_v2(&envelope)
        .unwrap()
        .verificar()
        .unwrap()
        .publicar()
        .unwrap();
    let id_original = identidad_archivo(&File::open(&canonico).unwrap()).unwrap();

    let observacion_original = {
        let lease = owner.contexto_admision().unwrap();
        let observacion = lease.observar_versionado().unwrap().unwrap();
        assert_eq!(
            recibo.validar_observacion_v2(&lease, &observacion, &journal),
            Ok(())
        );
        let journal_ajeno = JournalV2::new(
            "ffffffffffffffffffffffffffffffff",
            journal.parent().clone(),
            journal.old().clone(),
            journal.new_fingerprint().clone(),
        )
        .unwrap();
        assert_eq!(
            recibo.validar_observacion_v2(&lease, &observacion, &journal_ajeno),
            Err(ErrorContextoAdmisionAnclado::CanonicalUnverified)
        );
        observacion
    };
    assert_eq!(observacion_original.evidencia_raw_retiro().0, id_original);

    let original_guardado = parent.join("contexto-original-preservado");
    fs::rename(&canonico, &original_guardado).unwrap();
    fs::write(&canonico, &esperado).unwrap();
    let id_reemplazo = identidad_archivo(&File::open(&canonico).unwrap()).unwrap();
    assert_ne!(id_original, id_reemplazo);
    assert_eq!(fs::read(&original_guardado).unwrap(), esperado);
    assert_eq!(fs::read(&canonico).unwrap(), esperado);

    {
        let lease = owner.contexto_admision().unwrap();
        let observacion = lease.observar_versionado().unwrap().unwrap();
        assert_ne!(
            observacion.evidencia_raw_retiro().0,
            observacion_original.evidencia_raw_retiro().0
        );
        assert_eq!(
            recibo.validar_observacion_v2(&lease, &observacion, &journal),
            Err(ErrorContextoAdmisionAnclado::CanonicalUnverified)
        );
    }
    drop(observacion_original);

    drop(owner);
    drop(lock);
    assert_eq!(
        fs::read(&lock_path).unwrap().as_slice(),
        lock_canary.as_slice()
    );

    let mut lock_ajeno = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner_ajeno =
        super::super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(
            &mut lock_ajeno,
        )
        .unwrap();
    {
        let lease = owner_ajeno.contexto_admision().unwrap();
        let observacion = lease.observar_versionado().unwrap().unwrap();
        assert_eq!(
            recibo.validar_observacion_v2(&lease, &observacion, &journal),
            Err(ErrorContextoAdmisionAnclado::OwnerUnverified)
        );
    }
    drop(owner_ajeno);
    drop(lock_ajeno);
    assert_eq!(
        fs::read(&lock_path).unwrap().as_slice(),
        lock_canary.as_slice()
    );
}

#[test]
fn temporal_v2_sustituido_por_bytes_iguales_y_otro_id_falla_verificacion() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, false);
    let (envelope, _) = envelope_v2(parent_identity(&parent));
    let esperado = envelope.encode().unwrap();
    let temporal = temporal_contexto_path(&ubicacion);
    let original = temp.path().join("original-v2-temporary");
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock)
            .unwrap();
    let recibo = owner
        .contexto_admision()
        .unwrap()
        .crear_temporal_v2(&envelope)
        .unwrap();
    let identidad_original = recibo.datos.identidad_archivo;

    fs::rename(&temporal, &original).unwrap();
    fs::write(&temporal, &esperado).unwrap();
    let reemplazo = File::open(&temporal).unwrap();
    let identidad_reemplazo = identidad_archivo(&reemplazo).unwrap();
    assert_ne!(identidad_reemplazo, identidad_original);
    assert_eq!(fs::read(&temporal).unwrap(), esperado);
    drop(reemplazo);
    let error = recibo.verificar().unwrap_err();

    assert_eq!(error, ErrorContextoAdmisionAnclado::TemporaryUnverified);
    assert_eq!(fs::read(&original).unwrap(), esperado);
    assert_eq!(fs::read(&temporal).unwrap(), esperado);
    assert!(!contexto_path(&ubicacion).exists());
}
