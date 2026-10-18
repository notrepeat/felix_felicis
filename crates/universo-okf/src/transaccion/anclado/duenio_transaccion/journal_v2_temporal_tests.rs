use super::*;
use crate::transaccion::anclado::{IdentidadArchivo, identidad_archivo};
use crate::transaccion::journal::v2::JournalV2;
use crate::transaccion::journal::{ArtifactFingerprint, DirectoryIdentity, RawTreeDigest};
use crate::transaccion::lock::LockEscritura;
use crate::transaccion::raiz::UbicacionRaiz;
use std::cell::Cell;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use tempfile::tempdir;

#[path = "journal_v2_failure_tests.rs"]
mod failure_mode_tests;
#[path = "journal_v2_native_lock_tests.rs"]
mod native_lock_tests;

const LOCK_CANARY: &[u8] = b"journal-v2-owner-lock-canary";

fn ubicacion_en(temp: &Path) -> UbicacionRaiz {
    let root = temp.join("bundle");
    fs::create_dir(&root).unwrap();
    UbicacionRaiz::nueva(&root).unwrap()
}

fn ruta_temporal(ubicacion: &UbicacionRaiz) -> PathBuf {
    ubicacion
        .ruta()
        .parent()
        .unwrap()
        .join(ubicacion.nombre_journal_temporal())
}

fn ruta_canonica(ubicacion: &UbicacionRaiz) -> PathBuf {
    ubicacion.journal().as_path().to_path_buf()
}

fn identidad_parent_foranea(temp: &Path) -> super::super::IdentidadDirectorio {
    let ruta = temp.join("parent-foraneo");
    fs::create_dir(&ruta).unwrap();
    let parent = super::super::abrir_directorio_absoluto(&ruta, true).unwrap();
    super::metadata_directorio(&parent).unwrap()
}

fn identidad_fingerprint(id: u64) -> DirectoryIdentity {
    #[cfg(unix)]
    {
        DirectoryIdentity::unix(7, id)
    }
    #[cfg(windows)]
    {
        DirectoryIdentity::windows(7, format!("{id:032x}"))
    }
}

fn fingerprint(id: u64, digito: char) -> ArtifactFingerprint {
    ArtifactFingerprint::new(
        identidad_fingerprint(id),
        RawTreeDigest::from_hex(digito.to_string().repeat(64)).unwrap(),
    )
}

fn journal_v2(parent: DirectoryIdentity) -> JournalV2 {
    JournalV2::new(
        "0123456789abcdef0123456789abcdef",
        parent,
        fingerprint(2, 'a'),
        fingerprint(3, 'b'),
    )
    .unwrap()
}

fn identidad_visible(path: &Path) -> IdentidadArchivo {
    identidad_archivo(&File::open(path).unwrap()).unwrap()
}

#[test]
fn owner_crea_temporal_v2_releido_y_conserva_lock_y_journal() {
    let temp = tempdir().unwrap();
    let ubicacion = ubicacion_en(temp.path());
    fs::write(ubicacion.lock().as_path(), LOCK_CANARY).unwrap();
    let temporal = ruta_temporal(&ubicacion);
    let lock_path = ubicacion.lock().as_path().to_path_buf();
    let lock_id;
    let journal_bytes;

    {
        let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
        lock_id = lock.identidad_archivo_lock().unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let journal = journal_v2(owner.identidad_parent.journal_identity().unwrap());
        journal_bytes = journal.encode().unwrap();
        let receipt = owner.crear_temporal_journal_v2(&journal).unwrap();

        assert!(receipt.journal == journal);
        assert_eq!(format!("{receipt:?}"), "ReciboTemporalJournalV2 { .. }");
        assert_eq!(fs::read(&temporal).unwrap(), journal_bytes);
        assert_eq!(
            identidad_visible(&temporal),
            receipt.identidad.identidad_archivo()
        );
        let observado = JournalV2::parse(&fs::read(&temporal).unwrap()).unwrap();
        assert!(observado == journal);
    }

    assert_eq!(identidad_visible(&lock_path), lock_id);
    assert_eq!(fs::read(lock_path).unwrap(), LOCK_CANARY);
    assert_eq!(fs::read(&temporal).unwrap(), journal_bytes);
}

#[test]
fn parent_foreign_preserva_error_o_postcheck_io_domina() {
    for postcheck_falla in [false, true] {
        let temp = tempdir().unwrap();
        let ubicacion = ubicacion_en(temp.path());
        let temporal = ruta_temporal(&ubicacion);
        let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let extranjero = journal_v2(identidad_fingerprint(91));

        let body_finished = Cell::new(false);
        let postcheck_called = Cell::new(false);
        let error = owner
            .crear_temporal_journal_v2_con_prueba(
                &extranjero,
                || {},
                || body_finished.set(true),
                |owner, resultado| {
                    assert!(body_finished.get());
                    assert!(matches!(
                        resultado,
                        Err(
                            super::super::journal_temporal::ErrorJournalAnclado::ParentIdentityMismatch
                        )
                    ));
                    postcheck_called.set(true);
                    if postcheck_falla {
                        Err(super::super::journal_temporal::ErrorJournalAnclado::Io)
                    } else {
                        owner.parent_revalidado()
                    }
                },
            )
            .err()
            .unwrap();
        assert!(postcheck_called.get());
        let esperado = if postcheck_falla {
            super::super::journal_temporal::ErrorJournalAnclado::Io
        } else {
            super::super::journal_temporal::ErrorJournalAnclado::ParentIdentityMismatch
        };
        assert_eq!(error, esperado);
        assert!(!temporal.exists());
    }
}

#[test]
fn probe_final_rechaza_reemplazo_de_bytes_iguales_con_id_nativo_distinto() {
    let temp = tempdir().unwrap();
    let ubicacion = ubicacion_en(temp.path());
    let temporal = ruta_temporal(&ubicacion);
    let moved = temp.path().join("temporal-original");
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let journal = journal_v2(owner.identidad_parent.journal_identity().unwrap());
    let bytes = journal.encode().unwrap();
    let id_original = Cell::new(None);

    let error = owner
        .crear_temporal_journal_v2_con_prueba(
            &journal,
            || {},
            || {
                id_original.set(Some(identidad_visible(&temporal)));
                fs::rename(&temporal, &moved).unwrap();
                fs::write(&temporal, &bytes).unwrap();
            },
            |owner, _| owner.parent_revalidado(),
        )
        .err()
        .unwrap();

    assert_eq!(
        error,
        super::super::journal_temporal::ErrorJournalAnclado::IdentityMismatch
    );
    let id_reemplazo = identidad_visible(&temporal);
    assert_ne!(id_original.get().unwrap(), id_reemplazo);
    assert_eq!(fs::read(&temporal).unwrap(), bytes);
    assert_eq!(fs::read(&moved).unwrap(), bytes);
}

#[test]
fn postcheck_io_domina_candidato_v2_escrito_y_releido() {
    let temp = tempdir().unwrap();
    let ubicacion = ubicacion_en(temp.path());
    let temporal = ruta_temporal(&ubicacion);
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let journal = journal_v2(owner.identidad_parent.journal_identity().unwrap());
    let body_finished = Cell::new(false);
    let postcheck_called = Cell::new(false);

    let error = owner
        .crear_temporal_journal_v2_con_prueba(
            &journal,
            || {},
            || body_finished.set(true),
            |_, resultado| {
                assert!(body_finished.get());
                assert!(resultado.is_ok());
                postcheck_called.set(true);
                Err(super::super::journal_temporal::ErrorJournalAnclado::Io)
            },
        )
        .err()
        .unwrap();

    assert_eq!(
        error,
        super::super::journal_temporal::ErrorJournalAnclado::Io
    );
    assert!(postcheck_called.get());
    assert!(JournalV2::parse(&fs::read(&temporal).unwrap()).unwrap() == journal);
}

#[test]
fn postcheck_io_domina_error_probe_tras_reemplazo_nativo() {
    let temp = tempdir().unwrap();
    let ubicacion = ubicacion_en(temp.path());
    let temporal = ruta_temporal(&ubicacion);
    let moved = temp.path().join("temporal-original");
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let journal = journal_v2(owner.identidad_parent.journal_identity().unwrap());
    let bytes = journal.encode().unwrap();
    let id_original = Cell::new(None);
    let body_finished = Cell::new(false);
    let postcheck_called = Cell::new(false);

    let error = owner
        .crear_temporal_journal_v2_con_prueba(
            &journal,
            || {},
            || {
                id_original.set(Some(identidad_visible(&temporal)));
                fs::rename(&temporal, &moved).unwrap();
                fs::write(&temporal, &bytes).unwrap();
                body_finished.set(true);
            },
            |_, resultado| {
                assert!(body_finished.get());
                assert!(matches!(
                    resultado,
                    Err(super::super::journal_temporal::ErrorJournalAnclado::IdentityMismatch)
                ));
                postcheck_called.set(true);
                Err(super::super::journal_temporal::ErrorJournalAnclado::Io)
            },
        )
        .err()
        .unwrap();

    assert_eq!(
        error,
        super::super::journal_temporal::ErrorJournalAnclado::Io
    );
    assert!(postcheck_called.get());
    assert_ne!(id_original.get().unwrap(), identidad_visible(&temporal));
    assert_eq!(fs::read(&temporal).unwrap(), bytes);
    assert_eq!(fs::read(&moved).unwrap(), bytes);
}

#[test]
fn publicacion_owner_precheck_fallido_no_invoca_cuerpo_y_preserva_temporal() {
    let temp = tempdir().unwrap();
    let ubicacion = ubicacion_en(temp.path());
    let lock_path = ubicacion.lock().as_path().to_path_buf();
    fs::write(&lock_path, LOCK_CANARY).unwrap();
    let canonico = ruta_canonica(&ubicacion);
    let temporal = ruta_temporal(&ubicacion);
    let lock_id;
    let temporal_id;
    let bytes;

    {
        let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
        lock_id = lock.identidad_archivo_lock().unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let journal = journal_v2(owner.identidad_parent.journal_identity().unwrap());
        bytes = journal.encode().unwrap();
        let recibo = owner.crear_temporal_journal_v2(&journal).unwrap();
        temporal_id = recibo.identidad.identidad_archivo();
        recibo._owner.identidad_parent = identidad_parent_foranea(temp.path());

        let cuerpo_llamado = Cell::new(false);
        let postcheck_llamado = Cell::new(false);
        let resultado = recibo.publicar_con_owner_con_prueba(
            |_| cuerpo_llamado.set(true),
            |_, _, _| panic!("el cuerpo no debe correr tras precheck fallido"),
            |_| {
                postcheck_llamado.set(true);
                panic!("no hay postcheck de publicación si el precheck falla")
            },
        );

        assert!(matches!(
            resultado,
            super::ResultadoPublicacionJournalV2ConOwner::PrecheckFallido(
                super::super::journal_temporal::ErrorJournalAnclado::ParentChanged
            )
        ));
        assert!(!cuerpo_llamado.get());
        assert!(!postcheck_llamado.get());
        assert!(!canonico.exists());
        assert_eq!(fs::read(&temporal).unwrap(), bytes);
        assert_eq!(identidad_visible(&temporal), temporal_id);
    }

    assert_eq!(identidad_visible(&lock_path), lock_id);
    assert_eq!(fs::read(lock_path).unwrap(), LOCK_CANARY);
}

#[test]
fn publicacion_owner_nominal_preserva_canonico_id_y_limpia_temporal() {
    let temp = tempdir().unwrap();
    let ubicacion = ubicacion_en(temp.path());
    let lock_path = ubicacion.lock().as_path().to_path_buf();
    fs::write(&lock_path, LOCK_CANARY).unwrap();
    let canonico = ruta_canonica(&ubicacion);
    let temporal = ruta_temporal(&ubicacion);
    let lock_id;

    {
        let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
        lock_id = lock.identidad_archivo_lock().unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let journal = journal_v2(owner.identidad_parent.journal_identity().unwrap());
        let bytes = journal.encode().unwrap();
        let recibo = owner.crear_temporal_journal_v2(&journal).unwrap();
        let identidad_esperada = recibo.identidad.identidad_archivo();

        let resultado = recibo.publicar_con_owner();

        assert!(matches!(
            resultado,
            super::ResultadoPublicacionJournalV2ConOwner::Publicacion(
                super::super::journal_temporal::ResultadoPublicacionJournal::Confirmado { .. }
            )
        ));
        assert_eq!(fs::read(&canonico).unwrap(), bytes);
        assert_eq!(identidad_visible(&canonico), identidad_esperada);
        assert!(!temporal.exists());
    }

    assert_eq!(identidad_visible(&lock_path), lock_id);
    assert_eq!(fs::read(lock_path).unwrap(), LOCK_CANARY);
}

#[test]
fn postcheck_owner_io_domina_confirmacion_c2_y_preserva_canonico() {
    let temp = tempdir().unwrap();
    let ubicacion = ubicacion_en(temp.path());
    let lock_path = ubicacion.lock().as_path().to_path_buf();
    fs::write(&lock_path, LOCK_CANARY).unwrap();
    let canonico = ruta_canonica(&ubicacion);
    let temporal = ruta_temporal(&ubicacion);
    let lock_id;
    let journal_bytes;
    let identidad_esperada;

    {
        let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
        lock_id = lock.identidad_archivo_lock().unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let journal = journal_v2(owner.identidad_parent.journal_identity().unwrap());
        journal_bytes = journal.encode().unwrap();
        let recibo = owner.crear_temporal_journal_v2(&journal).unwrap();
        identidad_esperada = recibo.identidad.identidad_archivo();
        let cuerpo_confirmado = Cell::new(false);
        let postcheck_llamado = Cell::new(false);

        let resultado = recibo.publicar_con_owner_con_prueba(
            |_| {},
            |owner, identidad, journal| {
                let lease = owner.journal_temporal_estricto()?;
                let resultado = lease.publicar_v2_con_prueba(identidad, journal, |_, _| {
                    fs::hard_link(&temporal, &canonico)
                        .map_err(|_| super::super::journal_temporal::ErrorJournalAnclado::Io)
                });
                cuerpo_confirmado.set(matches!(
                    resultado,
                    super::super::journal_temporal::ResultadoPublicacionJournal::Confirmado { .. }
                ));
                Ok(resultado)
            },
            |_| {
                postcheck_llamado.set(true);
                Err(super::super::journal_temporal::ErrorJournalAnclado::Io)
            },
        );

        assert!(matches!(
            resultado,
            super::ResultadoPublicacionJournalV2ConOwner::PostcheckNoVerificado
        ));
        assert!(cuerpo_confirmado.get());
        assert!(postcheck_llamado.get());
        assert_eq!(fs::read(&canonico).unwrap(), journal_bytes);
        assert_eq!(identidad_visible(&canonico), identidad_esperada);
        assert!(!temporal.exists());
    }

    assert_eq!(identidad_visible(&lock_path), lock_id);
    assert_eq!(fs::read(lock_path).unwrap(), LOCK_CANARY);
}

#[test]
fn postcheck_owner_io_domina_error_real_del_lease_y_preserva_temporal() {
    let temp = tempdir().unwrap();
    let ubicacion = ubicacion_en(temp.path());
    let lock_path = ubicacion.lock().as_path().to_path_buf();
    fs::write(&lock_path, LOCK_CANARY).unwrap();
    let canonico = ruta_canonica(&ubicacion);
    let temporal = ruta_temporal(&ubicacion);
    let lock_id;
    let bytes;
    let temporal_id;

    {
        let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
        lock_id = lock.identidad_archivo_lock().unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let journal = journal_v2(owner.identidad_parent.journal_identity().unwrap());
        bytes = journal.encode().unwrap();
        let recibo = owner.crear_temporal_journal_v2(&journal).unwrap();
        temporal_id = recibo.identidad.identidad_archivo();
        let foreign_parent = identidad_parent_foranea(temp.path());
        let error_cuerpo = Cell::new(None);
        let postcheck_llamado = Cell::new(false);

        let resultado = recibo.publicar_con_owner_con_prueba(
            |owner| owner.identidad_parent = foreign_parent,
            |owner, identidad, journal| {
                let resultado = super::publicar_journal_v2_desde_owner(owner, identidad, journal);
                if let Err(error) = resultado {
                    error_cuerpo.set(Some(error));
                }
                resultado
            },
            |_| {
                postcheck_llamado.set(true);
                Err(super::super::journal_temporal::ErrorJournalAnclado::Io)
            },
        );

        assert!(matches!(
            resultado,
            super::ResultadoPublicacionJournalV2ConOwner::PostcheckNoVerificado
        ));
        assert_eq!(
            error_cuerpo.get(),
            Some(super::super::journal_temporal::ErrorJournalAnclado::ParentChanged)
        );
        assert!(postcheck_llamado.get());
        assert!(!canonico.exists());
        assert_eq!(fs::read(&temporal).unwrap(), bytes);
        assert_eq!(identidad_visible(&temporal), temporal_id);
    }

    assert_eq!(identidad_visible(&lock_path), lock_id);
    assert_eq!(fs::read(lock_path).unwrap(), LOCK_CANARY);
}

#[test]
fn postcheck_owner_io_domina_no_aplicado_c2_y_preserva_temporal() {
    let temp = tempdir().unwrap();
    let ubicacion = ubicacion_en(temp.path());
    fs::write(ubicacion.lock().as_path(), LOCK_CANARY).unwrap();
    let lock_path = ubicacion.lock().as_path().to_path_buf();
    let canonico = ruta_canonica(&ubicacion);
    let temporal = ruta_temporal(&ubicacion);
    let lock_id;
    let temporal_id;
    let journal_bytes;

    {
        let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
        lock_id = lock.identidad_archivo_lock().unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let journal = journal_v2(owner.identidad_parent.journal_identity().unwrap());
        journal_bytes = journal.encode().unwrap();
        let receipt = owner.crear_temporal_journal_v2(&journal).unwrap();
        temporal_id = receipt.identidad.identidad_archivo();
        let body_no_aplicado = Cell::new(false);
        let postcheck_called = Cell::new(false);

        let result = receipt.publicar_con_owner_con_prueba(
            |_| {},
            |owner, identidad, journal| {
                let lease = owner.journal_temporal_estricto()?;
                let outcome = lease.publicar_v2_con_prueba(identidad, journal, |_, _| {
                    Err(super::super::journal_temporal::ErrorJournalAnclado::Io)
                });
                body_no_aplicado.set(matches!(
                    outcome,
                    super::super::journal_temporal::ResultadoPublicacionJournal::NoAplicado(
                        super::super::journal_temporal::ErrorJournalAnclado::Io
                    )
                ));
                Ok(outcome)
            },
            |_| {
                postcheck_called.set(true);
                Err(super::super::journal_temporal::ErrorJournalAnclado::Io)
            },
        );

        assert!(matches!(
            result,
            super::ResultadoPublicacionJournalV2ConOwner::PostcheckNoVerificado
        ));
        assert!(body_no_aplicado.get());
        assert!(postcheck_called.get());
        assert!(!canonico.exists());
        assert_eq!(fs::read(&temporal).unwrap(), journal_bytes);
        assert_eq!(identidad_visible(&temporal), temporal_id);
    }

    assert_eq!(identidad_visible(&lock_path), lock_id);
    assert_eq!(fs::read(lock_path).unwrap(), LOCK_CANARY);
}

#[test]
fn postcheck_owner_io_domina_indeterminado_c2_y_preserva_ambos_siblings() {
    let temp = tempdir().unwrap();
    let ubicacion = ubicacion_en(temp.path());
    fs::write(ubicacion.lock().as_path(), LOCK_CANARY).unwrap();
    let lock_path = ubicacion.lock().as_path().to_path_buf();
    let canonico = ruta_canonica(&ubicacion);
    let temporal = ruta_temporal(&ubicacion);
    let lock_id;
    let temporal_id;
    let journal_bytes;
    let canonical_bytes = b"not-a-journal-v2".to_vec();
    let canonical_id = Cell::new(None);

    {
        let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
        lock_id = lock.identidad_archivo_lock().unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let journal = journal_v2(owner.identidad_parent.journal_identity().unwrap());
        journal_bytes = journal.encode().unwrap();
        let receipt = owner.crear_temporal_journal_v2(&journal).unwrap();
        temporal_id = receipt.identidad.identidad_archivo();
        let body_indeterminado = Cell::new(false);
        let postcheck_called = Cell::new(false);

        let result = receipt.publicar_con_owner_con_prueba(
            |_| {},
            |owner, identidad, journal| {
                let lease = owner.journal_temporal_estricto()?;
                let outcome = lease.publicar_v2_con_prueba(identidad, journal, |_, _| {
                    fs::write(&canonico, &canonical_bytes)
                        .map_err(|_| super::super::journal_temporal::ErrorJournalAnclado::Io)?;
                    canonical_id.set(Some(identidad_visible(&canonico)));
                    Err(super::super::journal_temporal::ErrorJournalAnclado::Io)
                });
                body_indeterminado.set(matches!(
                    outcome,
                    super::super::journal_temporal::ResultadoPublicacionJournal::Indeterminado {
                        ..
                    }
                ));
                Ok(outcome)
            },
            |_| {
                postcheck_called.set(true);
                Err(super::super::journal_temporal::ErrorJournalAnclado::Io)
            },
        );

        assert!(matches!(
            result,
            super::ResultadoPublicacionJournalV2ConOwner::PostcheckNoVerificado
        ));
        assert!(body_indeterminado.get());
        assert!(postcheck_called.get());
        assert_eq!(fs::read(&canonico).unwrap(), canonical_bytes);
        assert_eq!(identidad_visible(&canonico), canonical_id.get().unwrap());
        assert_eq!(fs::read(&temporal).unwrap(), journal_bytes);
        assert_eq!(identidad_visible(&temporal), temporal_id);
    }

    assert_eq!(identidad_visible(&lock_path), lock_id);
    assert_eq!(fs::read(lock_path).unwrap(), LOCK_CANARY);
}
