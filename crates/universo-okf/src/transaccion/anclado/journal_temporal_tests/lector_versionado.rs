use super::super::ErrorJournalAnclado;
use super::super::versionado::{
    ErrorLecturaJournalVersionada, EtapaLecturaJournalVersionada, EtapaProbeScopeJournalVersionada,
};
use super::{crear_root, fingerprint_prueba, identidad_distinta, journal_prueba, nombre_journal};
use crate::transaccion::anclado::duenio_transaccion::DuenioTransaccionAnclado;
use crate::transaccion::anclado::{ErrorDirectorioAnclado, ScopeFilesystem, scope_filesystem};
use crate::transaccion::journal::JournalError;
use crate::transaccion::journal::selector::{JournalPersistido, JournalPersistidoError};
use crate::transaccion::journal::v2::JournalV2;
use crate::transaccion::lock::LockEscritura;
use std::cell::Cell;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use tempfile::tempdir;

const TRANSACTION_ID: &str = "0123456789abcdef0123456789abcdef";

fn registrar_y_observar(es_v2: bool) {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let parent = {
        let lease = owner.journal_temporal_estricto().unwrap();
        lease.identidad_parent.journal_identity().unwrap()
    };
    let esperado = if es_v2 {
        JournalPersistido::V2(
            JournalV2::new(
                TRANSACTION_ID,
                parent,
                fingerprint_prueba(2, 'a'),
                fingerprint_prueba(3, 'b'),
            )
            .unwrap(),
        )
    } else {
        JournalPersistido::V1(journal_prueba(parent))
    };
    let bytes = match &esperado {
        JournalPersistido::V1(journal) => journal.encode().unwrap(),
        JournalPersistido::V2(journal) => journal.encode().unwrap(),
    };
    let canonical = nombre_journal(&root);
    fs::write(&canonical, &bytes).unwrap();

    let lease = owner.journal_temporal_estricto().unwrap();
    let observacion = lease.observar_versionado().unwrap().unwrap();
    assert_eq!(observacion.journal(), &esperado);
    assert_eq!(
        observacion.identidad(),
        super::super::identidad_archivo(&File::open(&canonical).unwrap()).unwrap()
    );

    let representacion = format!("{observacion:?}");
    assert!(!representacion.contains(canonical.to_string_lossy().as_ref()));
    assert!(!representacion.contains(TRANSACTION_ID));
    assert!(!representacion.contains(&"a".repeat(64)));
    drop(observacion);
}

#[test]
fn observa_v1_y_v2_sin_proyeccion_y_con_handle_ligado() {
    registrar_y_observar(false);
    registrar_y_observar(true);
}

#[test]
fn ausencia_exacta_del_canonico_devuelve_none() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

    let lease = owner.journal_temporal_estricto().unwrap();
    let observacion = lease.observar_versionado().unwrap();
    assert!(observacion.is_none());
    assert!(!nombre_journal(&root).exists());
}

#[test]
fn metadata_rechaza_sobrecupo_antes_de_parsear_y_preserva_bytes() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let canonical = nombre_journal(&root);
    let bytes = vec![0xff; crate::transaccion::journal::selector::MAX_VERSIONED_JOURNAL_BYTES + 1];
    fs::write(&canonical, &bytes).unwrap();

    let lease = owner.journal_temporal_estricto().unwrap();
    let error = lease.observar_versionado().unwrap_err();
    assert_eq!(
        error,
        ErrorLecturaJournalVersionada::Lease(ErrorJournalAnclado::TooLarge)
    );
    assert_eq!(fs::read(canonical).unwrap(), bytes);
}

#[test]
fn parent_persistido_distinto_falla_cerrado_y_preserva_archivo() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let parent = {
        let lease = owner.journal_temporal_estricto().unwrap();
        lease.identidad_parent.journal_identity().unwrap()
    };
    let journal = journal_prueba(identidad_distinta(&parent));
    let bytes = journal.encode().unwrap();
    let canonical = nombre_journal(&root);
    fs::write(&canonical, &bytes).unwrap();

    let lease = owner.journal_temporal_estricto().unwrap();
    let error = lease.observar_versionado().unwrap_err();
    assert_eq!(
        error,
        ErrorLecturaJournalVersionada::Lease(ErrorJournalAnclado::ParentIdentityMismatch)
    );
    assert_eq!(fs::read(canonical).unwrap(), bytes);
}

#[test]
fn lease_legacy_sin_scope_no_puede_invocar_lector_estricto() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner.journal_temporal().unwrap();

    let error = lease.observar_versionado().unwrap_err();

    assert_eq!(
        error,
        ErrorLecturaJournalVersionada::Lease(ErrorJournalAnclado::ParentScopeUnverified)
    );
}

#[test]
fn ausencia_que_reaparece_antes_de_la_segunda_sonda_es_error() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner.journal_temporal_estricto().unwrap();
    let parent = lease.identidad_parent.journal_identity().unwrap();
    let bytes = journal_prueba(parent).encode().unwrap();
    let canonical = nombre_journal(&root);

    let error = lease
        .observar_versionado_con_prueba(|etapa| {
            if etapa == EtapaLecturaJournalVersionada::DespuesDePrimeraAusencia {
                fs::write(&canonical, &bytes).unwrap();
            }
        })
        .unwrap_err();

    assert_eq!(
        error,
        ErrorLecturaJournalVersionada::Lease(ErrorJournalAnclado::CanonicalConflict)
    );
    assert_eq!(fs::read(canonical).unwrap(), bytes);
}

#[test]
fn reemplazo_de_nombre_con_bytes_iguales_falla_por_identidad() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner.journal_temporal_estricto().unwrap();
    let parent = lease.identidad_parent.journal_identity().unwrap();
    let bytes = journal_prueba(parent).encode().unwrap();
    let canonical = nombre_journal(&root);
    let retained = root.join("journal-retained-after-read");
    fs::write(&canonical, &bytes).unwrap();
    let identidad_original =
        super::super::identidad_archivo(&File::open(&canonical).unwrap()).unwrap();
    let reemplazado = Cell::new(false);

    let error = lease
        .observar_versionado_con_prueba(|etapa| {
            if etapa == EtapaLecturaJournalVersionada::AntesDeSondaVisible {
                fs::rename(&canonical, &retained).unwrap();
                fs::write(&canonical, &bytes).unwrap();
                reemplazado.set(true);
            }
        })
        .unwrap_err();

    assert!(reemplazado.get());
    assert_eq!(
        error,
        ErrorLecturaJournalVersionada::Lease(ErrorJournalAnclado::IdentityMismatch)
    );
    assert_eq!(fs::read(&retained).unwrap(), bytes);
    assert_eq!(fs::read(&canonical).unwrap(), bytes);
    let identidad_retenida =
        super::super::identidad_archivo(&File::open(&retained).unwrap()).unwrap();
    let identidad_visible =
        super::super::identidad_archivo(&File::open(&canonical).unwrap()).unwrap();
    assert_eq!(identidad_retenida, identidad_original);
    assert_ne!(identidad_visible, identidad_original);
}

#[test]
fn crecimiento_despues_de_metadata_se_rechaza_por_centinela() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner.journal_temporal_estricto().unwrap();
    let canonical = nombre_journal(&root);
    let cuota = crate::transaccion::journal::selector::MAX_VERSIONED_JOURNAL_BYTES;
    let bytes = vec![0xff; cuota];
    fs::write(&canonical, &bytes).unwrap();
    let crecio = Cell::new(false);

    let error = lease
        .observar_versionado_con_prueba(|etapa| {
            if etapa == EtapaLecturaJournalVersionada::DespuesDeMetadata {
                OpenOptions::new()
                    .append(true)
                    .open(&canonical)
                    .unwrap()
                    .write_all(&[0xff])
                    .unwrap();
                crecio.set(true);
            }
        })
        .unwrap_err();

    assert!(crecio.get());
    assert_eq!(
        error,
        ErrorLecturaJournalVersionada::Lease(ErrorJournalAnclado::TooLarge)
    );
    let mut esperado = bytes;
    esperado.push(0xff);
    assert_eq!(fs::read(canonical).unwrap(), esperado);
}

#[test]
fn probe_parent_inicial_rechaza_mismatch_y_error_sin_cambiar_archivo() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner.journal_temporal_estricto().unwrap();
    let scope_esperado = lease.scope_parent.unwrap();
    let bytes = journal_prueba(lease.identidad_parent.journal_identity().unwrap())
        .encode()
        .unwrap();
    let canonical = nombre_journal(&root);
    fs::write(&canonical, &bytes).unwrap();

    for devuelve_error in [false, true] {
        let error = lease
            .observar_versionado_con_scope_prueba(|etapa, handle| {
                if etapa == EtapaProbeScopeJournalVersionada::ParentInicial {
                    if devuelve_error {
                        Err(ErrorDirectorioAnclado::Io)
                    } else {
                        Ok(ScopeFilesystem(scope_esperado.0 ^ 1))
                    }
                } else {
                    scope_filesystem(handle)
                }
            })
            .unwrap_err();

        assert_eq!(
            error,
            ErrorLecturaJournalVersionada::Lease(ErrorJournalAnclado::ParentScopeUnverified)
        );
        assert_eq!(fs::read(&canonical).unwrap(), bytes);
    }
}

#[test]
fn probe_source_inicial_rechaza_mismatch_y_error_sin_cambiar_archivo() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner.journal_temporal_estricto().unwrap();
    let scope_esperado = lease.scope_parent.unwrap();
    let bytes = journal_prueba(lease.identidad_parent.journal_identity().unwrap())
        .encode()
        .unwrap();
    let canonical = nombre_journal(&root);
    fs::write(&canonical, &bytes).unwrap();

    for devuelve_error in [false, true] {
        let error = lease
            .observar_versionado_con_scope_prueba(|etapa, handle| {
                if etapa == EtapaProbeScopeJournalVersionada::SourceInicial {
                    if devuelve_error {
                        Err(ErrorDirectorioAnclado::Io)
                    } else {
                        Ok(ScopeFilesystem(scope_esperado.0 ^ 1))
                    }
                } else {
                    scope_filesystem(handle)
                }
            })
            .unwrap_err();

        assert_eq!(
            error,
            ErrorLecturaJournalVersionada::Lease(ErrorJournalAnclado::SourceScopeUnverified)
        );
        assert_eq!(fs::read(&canonical).unwrap(), bytes);
    }
}

#[test]
fn probe_visible_final_rechaza_mismatch_y_error_sin_cambiar_archivo() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner.journal_temporal_estricto().unwrap();
    let scope_esperado = lease.scope_parent.unwrap();
    let bytes = journal_prueba(lease.identidad_parent.journal_identity().unwrap())
        .encode()
        .unwrap();
    let canonical = nombre_journal(&root);
    fs::write(&canonical, &bytes).unwrap();

    for devuelve_error in [false, true] {
        let error = lease
            .observar_versionado_con_scope_prueba(|etapa, handle| {
                if etapa == EtapaProbeScopeJournalVersionada::VisibleFinal {
                    if devuelve_error {
                        Err(ErrorDirectorioAnclado::Io)
                    } else {
                        Ok(ScopeFilesystem(scope_esperado.0 ^ 1))
                    }
                } else {
                    scope_filesystem(handle)
                }
            })
            .unwrap_err();

        assert_eq!(
            error,
            ErrorLecturaJournalVersionada::Lease(ErrorJournalAnclado::SourceScopeUnverified)
        );
        assert_eq!(fs::read(&canonical).unwrap(), bytes);
    }
}

#[test]
fn probe_parent_tras_ausencia_rechaza_mismatch_y_error_sin_crear_archivo() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner.journal_temporal_estricto().unwrap();
    let scope_esperado = lease.scope_parent.unwrap();
    let canonical = nombre_journal(&root);

    for devuelve_error in [false, true] {
        let error = lease
            .observar_versionado_con_scope_prueba(|etapa, handle| {
                if etapa == EtapaProbeScopeJournalVersionada::ParentDespuesDePrimeraAusencia {
                    if devuelve_error {
                        Err(ErrorDirectorioAnclado::Io)
                    } else {
                        Ok(ScopeFilesystem(scope_esperado.0 ^ 1))
                    }
                } else {
                    scope_filesystem(handle)
                }
            })
            .unwrap_err();

        assert_eq!(
            error,
            ErrorLecturaJournalVersionada::Lease(ErrorJournalAnclado::ParentScopeUnverified)
        );
        assert!(!canonical.exists());
    }
}

#[test]
fn schema_desconocido_falla_sin_modificar_archivo() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner.journal_temporal_estricto().unwrap();
    let parent = lease.identidad_parent.journal_identity().unwrap();
    let canonical = nombre_journal(&root);
    let bytes = String::from_utf8(journal_prueba(parent).encode().unwrap())
        .unwrap()
        .replacen("felix-journal-v1", "felix-journal-future", 1)
        .into_bytes();
    fs::write(&canonical, &bytes).unwrap();

    let error = lease.observar_versionado().unwrap_err();

    assert_eq!(
        error,
        ErrorLecturaJournalVersionada::Selector(JournalPersistidoError::SchemaUnsupported)
    );
    assert_eq!(fs::read(canonical).unwrap(), bytes);
}

#[test]
fn esquema_v1_malformado_no_cambia_de_decoder_ni_modifica_bytes() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner.journal_temporal_estricto().unwrap();
    let parent = lease.identidad_parent.journal_identity().unwrap();
    let canonical = nombre_journal(&root);
    let journal_v2 = JournalV2::new(
        TRANSACTION_ID,
        parent,
        fingerprint_prueba(2, 'a'),
        fingerprint_prueba(3, 'b'),
    )
    .unwrap();
    let bytes = String::from_utf8(journal_v2.encode().unwrap())
        .unwrap()
        .replacen("felix-journal-v2", "felix-journal-v1", 1)
        .into_bytes();
    fs::write(&canonical, &bytes).unwrap();

    let error = lease.observar_versionado().unwrap_err();

    assert_eq!(
        error,
        ErrorLecturaJournalVersionada::Selector(JournalPersistidoError::V1(
            JournalError::Malformed
        ))
    );
    assert_eq!(fs::read(canonical).unwrap(), bytes);
}

#[test]
fn directorio_en_nombre_canonico_se_rechaza_y_preserva_su_canario() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner.journal_temporal_estricto().unwrap();
    let canonical = nombre_journal(&root);
    fs::create_dir(&canonical).unwrap();
    let canary = canonical.join("canary");
    fs::write(&canary, b"directory-canary").unwrap();

    let error = lease.observar_versionado().unwrap_err();

    assert!(matches!(
        error,
        ErrorLecturaJournalVersionada::Lease(
            ErrorJournalAnclado::RegularFileRequired | ErrorJournalAnclado::Io
        )
    ));
    assert!(canonical.is_dir());
    assert_eq!(fs::read(canary).unwrap(), b"directory-canary");
}

#[cfg(unix)]
#[test]
fn symlink_canonico_no_se_sigue_y_preserva_el_archivo_destino() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let lease = owner.journal_temporal_estricto().unwrap();
    let parent = lease.identidad_parent.journal_identity().unwrap();
    let target = temp.path().join("outside-journal-canary");
    let bytes = journal_prueba(parent).encode().unwrap();
    fs::write(&target, &bytes).unwrap();
    let canonical = nombre_journal(&root);
    symlink(&target, &canonical).unwrap();

    let error = lease.observar_versionado().unwrap_err();

    assert_eq!(
        error,
        ErrorLecturaJournalVersionada::Lease(ErrorJournalAnclado::ReparseNotAllowed)
    );
    assert!(
        fs::symlink_metadata(&canonical)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(target).unwrap(), bytes);
}
