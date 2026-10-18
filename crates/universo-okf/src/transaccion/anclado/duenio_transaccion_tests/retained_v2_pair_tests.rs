use super::*;

use crate::transaccion::admision::envelope::v2::EnvelopeContextoAdmisionV2;
use crate::transaccion::anclado::duenio_transaccion::retained_v2_pair::ErrorObservacionPareadaV2;
use crate::transaccion::anclado::{IdentidadArchivo, identidad_archivo as identidad_nativa};
use crate::transaccion::journal::JournalState;
use crate::transaccion::journal::v2::JournalV2;
use std::fs::File;
use std::path::Path;

#[derive(Clone, Copy)]
enum DiferenciaBinding {
    Ninguna,
    TransactionId,
    Parent,
    Old,
    New,
}

fn journal_v2(
    parent: DirectoryIdentity,
    transaction_id: &str,
    old: (u64, char),
    new: (u64, char),
) -> JournalV2 {
    JournalV2::new(
        transaction_id,
        parent,
        fingerprint_journal_prueba(old.0, old.1),
        fingerprint_journal_prueba(new.0, new.1),
    )
    .unwrap()
}

fn parent_distinto() -> DirectoryIdentity {
    #[cfg(unix)]
    {
        DirectoryIdentity::unix(u64::MAX, u64::MAX - 1)
    }
    #[cfg(windows)]
    {
        DirectoryIdentity::windows(u64::MAX, "ffffffffffffffffffffffffffffffff".to_owned())
    }
}

fn frame_contexto_v2(journal: &JournalV2) -> Vec<u8> {
    EnvelopeContextoAdmisionV2::ligar(envelope::tests::contexto(b"\n"), journal)
        .encode()
        .unwrap()
}

fn escribir_pareja(
    owner: &DuenioTransaccionAnclado<'_>,
    rutas: &RutasFixture,
    diferencia: DiferenciaBinding,
) -> (Vec<u8>, Vec<u8>) {
    let parent = owner.identidad_parent.journal_identity().unwrap();
    let journal = journal_v2(parent.clone(), TRANSACTION_ID, (2, 'a'), (3, 'b'));
    let binding = match diferencia {
        DiferenciaBinding::Ninguna => journal.clone(),
        DiferenciaBinding::TransactionId => journal_v2(
            parent.clone(),
            "fedcba9876543210fedcba9876543210",
            (2, 'a'),
            (3, 'b'),
        ),
        DiferenciaBinding::Parent => {
            journal_v2(parent_distinto(), TRANSACTION_ID, (2, 'a'), (3, 'b'))
        }
        DiferenciaBinding::Old => journal_v2(parent.clone(), TRANSACTION_ID, (4, 'c'), (3, 'b')),
        DiferenciaBinding::New => journal_v2(parent.clone(), TRANSACTION_ID, (2, 'a'), (5, 'd')),
    };
    let bytes_contexto = frame_contexto_v2(&binding);
    let mut persistido = journal;
    persistido.advance(JournalState::BackupReady).unwrap();
    let bytes_journal = persistido.encode().unwrap();
    fs::write(rutas.ubicacion.journal().as_path(), &bytes_journal).unwrap();
    fs::write(&rutas.contexto, &bytes_contexto).unwrap();
    (bytes_journal, bytes_contexto)
}

#[test]
fn pareja_v2_retenida_acepta_binding_prepared_y_journal_backup_ready() {
    let temp = tempdir().unwrap();
    let rutas = preparar_rutas(&temp);
    let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let (bytes_journal, bytes_contexto) =
        escribir_pareja(&owner, &rutas, DiferenciaBinding::Ninguna);

    let pareja = owner.leer_pareja_v2_ligada().unwrap();

    assert_eq!(
        fs::read(rutas.ubicacion.journal().as_path()).unwrap(),
        bytes_journal
    );
    assert_eq!(fs::read(&rutas.contexto).unwrap(), bytes_contexto);

    drop(pareja);
    drop(owner);
    drop(lock);
    verificar_lock_y_contexto(&rutas, Some(&bytes_contexto));
}

#[test]
fn cada_campo_binding_v2_discordante_falla_y_preserva_ambos_frames() {
    for diferencia in [
        DiferenciaBinding::TransactionId,
        DiferenciaBinding::Parent,
        DiferenciaBinding::Old,
        DiferenciaBinding::New,
    ] {
        let temp = tempdir().unwrap();
        let rutas = preparar_rutas(&temp);
        let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let (bytes_journal, bytes_contexto) = escribir_pareja(&owner, &rutas, diferencia);

        assert!(matches!(
            owner.leer_pareja_v2_ligada(),
            Err(ErrorObservacionPareadaV2::PairUnverified)
        ));
        assert_eq!(
            fs::read(rutas.ubicacion.journal().as_path()).unwrap(),
            bytes_journal
        );
        assert_eq!(fs::read(&rutas.contexto).unwrap(), bytes_contexto);

        drop(owner);
        drop(lock);
        verificar_lock_y_contexto(&rutas, Some(&bytes_contexto));
    }
}

#[test]
fn ausencia_de_journal_o_contexto_falla_cerrado_y_no_toca_el_sibling_presente() {
    for falta_journal in [true, false] {
        let temp = tempdir().unwrap();
        let rutas = preparar_rutas(&temp);
        let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let (bytes_journal, bytes_contexto) =
            escribir_pareja(&owner, &rutas, DiferenciaBinding::Ninguna);
        let ausente = if falta_journal {
            rutas.ubicacion.journal().as_path().to_path_buf()
        } else {
            rutas.contexto.clone()
        };
        let presente = if falta_journal {
            (rutas.contexto.clone(), bytes_contexto.clone())
        } else {
            (
                rutas.ubicacion.journal().as_path().to_path_buf(),
                bytes_journal.clone(),
            )
        };
        fs::remove_file(&ausente).unwrap();

        assert!(matches!(
            owner.leer_pareja_v2_ligada(),
            Err(ErrorObservacionPareadaV2::PairUnverified)
        ));
        assert!(!ausente.exists());
        assert_eq!(fs::read(&presente.0).unwrap(), presente.1);

        drop(owner);
        drop(lock);
        verificar_lock_y_contexto(
            &rutas,
            if falta_journal {
                Some(&bytes_contexto)
            } else {
                None
            },
        );
    }
}

#[test]
fn rechaza_journal_v1_y_contexto_v1_y_preserva_los_archivos() {
    for contexto_v1 in [false, true] {
        let temp = tempdir().unwrap();
        let rutas = preparar_rutas(&temp);
        let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let (bytes_journal_v2, bytes_contexto_v2) =
            escribir_pareja(&owner, &rutas, DiferenciaBinding::Ninguna);
        let parent = owner.identidad_parent.journal_identity().unwrap();
        let bytes_contexto = if contexto_v1 {
            frame_v1(parent.clone())
        } else {
            bytes_contexto_v2
        };
        let bytes_journal = if contexto_v1 {
            bytes_journal_v2
        } else {
            Journal::new(
                TRANSACTION_ID,
                parent,
                fingerprint_journal_prueba(2, 'a'),
                fingerprint_journal_prueba(3, 'b'),
            )
            .unwrap()
            .encode()
            .unwrap()
        };
        if contexto_v1 {
            fs::write(&rutas.contexto, &bytes_contexto).unwrap();
        } else {
            fs::write(rutas.ubicacion.journal().as_path(), &bytes_journal).unwrap();
        }

        assert!(matches!(
            owner.leer_pareja_v2_ligada(),
            Err(ErrorObservacionPareadaV2::PairUnverified)
        ));
        assert_eq!(
            fs::read(rutas.ubicacion.journal().as_path()).unwrap(),
            bytes_journal
        );
        assert_eq!(fs::read(&rutas.contexto).unwrap(), bytes_contexto);

        drop(owner);
        drop(lock);
        verificar_lock_y_contexto(&rutas, Some(&bytes_contexto));
    }
}

#[test]
fn postcheck_exterior_domina_tanto_un_candidato_como_un_error_de_observacion() {
    for falta_contexto in [false, true] {
        let temp = tempdir().unwrap();
        let rutas = preparar_rutas(&temp);
        let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let (bytes_journal, bytes_contexto) =
            escribir_pareja(&owner, &rutas, DiferenciaBinding::Ninguna);
        if falta_contexto {
            fs::remove_file(&rutas.contexto).unwrap();
        }

        assert!(matches!(
            owner.leer_pareja_v2_ligada_con_prueba(
                |_| {},
                |_| Err(crate::transaccion::anclado::journal_temporal::ErrorJournalAnclado::Io),
            ),
            Err(ErrorObservacionPareadaV2::OwnerUnverified)
        ));
        assert_eq!(
            fs::read(rutas.ubicacion.journal().as_path()).unwrap(),
            bytes_journal
        );

        drop(owner);
        drop(lock);
        verificar_lock_y_contexto(
            &rutas,
            if falta_contexto {
                None
            } else {
                Some(&bytes_contexto)
            },
        );
    }
}

fn identidad_real(path: &Path) -> IdentidadArchivo {
    identidad_nativa(&File::open(path).unwrap()).unwrap()
}

#[test]
fn revalidar_pareja_v2_acepta_los_mismos_objetos_y_preserva_frames() {
    let temp = tempdir().unwrap();
    let rutas = preparar_rutas(&temp);
    let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let (bytes_journal, bytes_contexto) =
        escribir_pareja(&owner, &rutas, DiferenciaBinding::Ninguna);

    let pareja = owner.revalidar_pareja_v2().unwrap();

    assert_eq!(
        fs::read(rutas.ubicacion.journal().as_path()).unwrap(),
        bytes_journal
    );
    assert_eq!(fs::read(&rutas.contexto).unwrap(), bytes_contexto);
    drop(pareja);
    drop(owner);
    drop(lock);
    verificar_lock_y_contexto(&rutas, Some(&bytes_contexto));
}

#[test]
fn reemplazo_journal_con_bytes_iguales_e_id_distinto_falla_y_preserva_ambos_objetos() {
    let temp = tempdir().unwrap();
    let rutas = preparar_rutas(&temp);
    let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let (bytes_journal, bytes_contexto) =
        escribir_pareja(&owner, &rutas, DiferenciaBinding::Ninguna);
    let canonico = rutas.ubicacion.journal().as_path().to_path_buf();
    let retenido = rutas
        .contexto
        .parent()
        .unwrap()
        .join("journal-original-retained");
    let fuente = rutas
        .contexto
        .parent()
        .unwrap()
        .join("journal-replacement-source");
    fs::write(&fuente, &bytes_journal).unwrap();
    let id_original = identidad_real(&canonico);
    let id_reemplazo = identidad_real(&fuente);
    assert_ne!(id_original, id_reemplazo);

    let resultado = owner.revalidar_pareja_v2_con_prueba(
        |_| {
            fs::rename(&canonico, &retenido).unwrap();
            fs::rename(&fuente, &canonico).unwrap();
        },
        |owner| owner.parent_revalidado(),
    );

    assert!(matches!(
        resultado,
        Err(ErrorObservacionPareadaV2::PairUnverified)
    ));
    assert_eq!(fs::read(&retenido).unwrap(), bytes_journal);
    assert_eq!(fs::read(&canonico).unwrap(), bytes_journal);
    assert_eq!(identidad_real(&retenido), id_original);
    assert_eq!(identidad_real(&canonico), id_reemplazo);
    assert_eq!(fs::read(&rutas.contexto).unwrap(), bytes_contexto);

    drop(owner);
    drop(lock);
    verificar_lock_y_contexto(&rutas, Some(&bytes_contexto));
}

#[test]
fn reemplazo_contexto_con_bytes_iguales_e_id_distinto_falla_y_preserva_ambos_objetos() {
    let temp = tempdir().unwrap();
    let rutas = preparar_rutas(&temp);
    let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let (bytes_journal, bytes_contexto) =
        escribir_pareja(&owner, &rutas, DiferenciaBinding::Ninguna);
    let canonico = rutas.contexto.clone();
    let retenido = rutas
        .contexto
        .parent()
        .unwrap()
        .join("context-original-retained");
    let fuente = rutas
        .contexto
        .parent()
        .unwrap()
        .join("context-replacement-source");
    fs::write(&fuente, &bytes_contexto).unwrap();
    let id_original = identidad_real(&canonico);
    let id_reemplazo = identidad_real(&fuente);
    assert_ne!(id_original, id_reemplazo);

    let resultado = owner.revalidar_pareja_v2_con_prueba(
        |_| {
            fs::rename(&canonico, &retenido).unwrap();
            fs::rename(&fuente, &canonico).unwrap();
        },
        |owner| owner.parent_revalidado(),
    );

    assert!(matches!(
        resultado,
        Err(ErrorObservacionPareadaV2::PairUnverified)
    ));
    assert_eq!(fs::read(&retenido).unwrap(), bytes_contexto);
    assert_eq!(fs::read(&canonico).unwrap(), bytes_contexto);
    assert_eq!(identidad_real(&retenido), id_original);
    assert_eq!(identidad_real(&canonico), id_reemplazo);
    assert_eq!(
        fs::read(rutas.ubicacion.journal().as_path()).unwrap(),
        bytes_journal
    );

    drop(owner);
    drop(lock);
    verificar_lock_y_contexto(&rutas, Some(&bytes_contexto));
}

#[test]
fn ausencia_final_de_journal_o_contexto_falla_y_preserva_el_original() {
    for falta_journal in [true, false] {
        let temp = tempdir().unwrap();
        let rutas = preparar_rutas(&temp);
        let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let (bytes_journal, bytes_contexto) =
            escribir_pareja(&owner, &rutas, DiferenciaBinding::Ninguna);
        let (canonico, retenido, bytes) = if falta_journal {
            (
                rutas.ubicacion.journal().as_path().to_path_buf(),
                rutas
                    .contexto
                    .parent()
                    .unwrap()
                    .join("journal-original-retained"),
                bytes_journal.clone(),
            )
        } else {
            (
                rutas.contexto.clone(),
                rutas
                    .contexto
                    .parent()
                    .unwrap()
                    .join("context-original-retained"),
                bytes_contexto.clone(),
            )
        };
        let id_original = identidad_real(&canonico);

        let resultado = owner.revalidar_pareja_v2_con_prueba(
            |_| fs::rename(&canonico, &retenido).unwrap(),
            |owner| owner.parent_revalidado(),
        );

        assert!(matches!(
            resultado,
            Err(ErrorObservacionPareadaV2::PairUnverified)
        ));
        assert!(!canonico.exists());
        assert_eq!(fs::read(&retenido).unwrap(), bytes);
        assert_eq!(identidad_real(&retenido), id_original);
        assert_eq!(
            fs::read(rutas.ubicacion.journal().as_path()).ok(),
            if falta_journal {
                None
            } else {
                Some(bytes_journal)
            }
        );
        drop(owner);
        drop(lock);
        verificar_lock_y_contexto(
            &rutas,
            if falta_journal {
                Some(&bytes_contexto)
            } else {
                None
            },
        );
    }
}

#[test]
fn postcheck_de_revalidacion_domina_candidato_error_de_lectura_y_error_de_probe() {
    for caso in 0..3 {
        let temp = tempdir().unwrap();
        let rutas = preparar_rutas(&temp);
        let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let (bytes_journal, bytes_contexto) =
            escribir_pareja(&owner, &rutas, DiferenciaBinding::Ninguna);
        if caso == 1 {
            fs::remove_file(rutas.ubicacion.journal().as_path()).unwrap();
        }

        let resultado = owner.revalidar_pareja_v2_con_prueba(
            |_| {
                if caso == 2 {
                    fs::remove_file(&rutas.contexto).unwrap();
                }
            },
            |_| Err(crate::transaccion::anclado::journal_temporal::ErrorJournalAnclado::Io),
        );

        assert!(matches!(
            resultado,
            Err(ErrorObservacionPareadaV2::OwnerUnverified)
        ));
        assert_eq!(!rutas.ubicacion.journal().as_path().exists(), caso == 1);
        assert_eq!(!rutas.contexto.exists(), caso == 2);
        if caso != 1 {
            assert_eq!(
                fs::read(rutas.ubicacion.journal().as_path()).unwrap(),
                bytes_journal
            );
        }
        if caso != 2 {
            assert_eq!(fs::read(&rutas.contexto).unwrap(), bytes_contexto);
        }

        drop(owner);
        drop(lock);
        verificar_lock_y_contexto(
            &rutas,
            if caso == 2 {
                None
            } else {
                Some(&bytes_contexto)
            },
        );
    }
}

fn nombres_siblings_b2a(rutas: &RutasFixture) -> [std::path::PathBuf; 6] {
    let parent = rutas.contexto.parent().unwrap();
    let nombre_contexto = rutas.contexto.file_name().unwrap();
    let nombre_temporal_contexto =
        crate::transaccion::anclado::contexto_admision::nombre_temporal_sibling(nombre_contexto)
            .unwrap();
    let basename = rutas.ubicacion.ruta().file_name().unwrap();
    let nombre_intent =
        crate::transaccion::anclado::duenio_transaccion::observar_siblings_nuevo_flujo::nombre_intent_retiro(basename)
            .unwrap();
    let nombre_temporal_intent =
        crate::transaccion::anclado::contexto_admision::nombre_temporal_sibling(&nombre_intent)
            .unwrap();

    [
        rutas.ubicacion.journal().as_path().to_path_buf(),
        parent.join(rutas.ubicacion.nombre_journal_temporal()),
        rutas.contexto.clone(),
        parent.join(nombre_temporal_contexto),
        parent.join(&nombre_intent),
        parent.join(nombre_temporal_intent),
    ]
}

#[test]
fn b2a_rechaza_slots_iniciales_invalidos_antes_de_invocar_a2() {
    for slot in [1, 3, 4, 5] {
        let temp = tempdir().unwrap();
        let rutas = preparar_rutas(&temp);
        let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let (bytes_journal, bytes_contexto) =
            escribir_pareja(&owner, &rutas, DiferenciaBinding::Ninguna);
        let siblings = nombres_siblings_b2a(&rutas);
        fs::write(&siblings[slot], b"auxiliary present initially").unwrap();
        let despues_de_a2 = Cell::new(false);

        let resultado = owner
            .observar_pareja_v2_con_slots_verificados_pendiente_identidad_con_prueba(
                |_| despues_de_a2.set(true),
                |owner| owner.parent_revalidado(),
            );

        assert!(matches!(
            resultado,
            Err(ErrorObservacionPareadaV2::PairUnverified)
        ));
        assert!(!despues_de_a2.get());
        assert_eq!(
            fs::read(&siblings[slot]).unwrap(),
            b"auxiliary present initially"
        );
        assert_eq!(
            fs::read(rutas.ubicacion.journal().as_path()).unwrap(),
            bytes_journal
        );
        assert_eq!(fs::read(&rutas.contexto).unwrap(), bytes_contexto);
        drop(owner);
        drop(lock);
        verificar_lock_y_contexto(&rutas, Some(&bytes_contexto));
    }

    for slot in [0, 2] {
        let temp = tempdir().unwrap();
        let rutas = preparar_rutas(&temp);
        let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let (bytes_journal, bytes_contexto) =
            escribir_pareja(&owner, &rutas, DiferenciaBinding::Ninguna);
        let siblings = nombres_siblings_b2a(&rutas);
        fs::remove_file(&siblings[slot]).unwrap();
        let despues_de_a2 = Cell::new(false);

        let resultado = owner
            .observar_pareja_v2_con_slots_verificados_pendiente_identidad_con_prueba(
                |_| despues_de_a2.set(true),
                |owner| owner.parent_revalidado(),
            );

        assert!(matches!(
            resultado,
            Err(ErrorObservacionPareadaV2::PairUnverified)
        ));
        assert!(!despues_de_a2.get());
        assert!(!siblings[slot].exists());
        if slot == 0 {
            assert_eq!(fs::read(&rutas.contexto).unwrap(), bytes_contexto);
        } else {
            assert_eq!(
                fs::read(rutas.ubicacion.journal().as_path()).unwrap(),
                bytes_journal
            );
        }
        drop(owner);
        drop(lock);
        verificar_lock_y_contexto(
            &rutas,
            if slot == 0 {
                Some(&bytes_contexto)
            } else {
                None
            },
        );
    }
}

#[test]
fn b2a_rechaza_cada_auxiliar_que_reaparece_despues_de_a2() {
    for slot in [1, 3, 4, 5] {
        let temp = tempdir().unwrap();
        let rutas = preparar_rutas(&temp);
        let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let (bytes_journal, bytes_contexto) =
            escribir_pareja(&owner, &rutas, DiferenciaBinding::Ninguna);
        let siblings = nombres_siblings_b2a(&rutas);

        let resultado = owner
            .observar_pareja_v2_con_slots_verificados_pendiente_identidad_con_prueba(
                |_| fs::write(&siblings[slot], b"auxiliary reappeared after A2").unwrap(),
                |owner| owner.parent_revalidado(),
            );

        assert!(matches!(
            resultado,
            Err(ErrorObservacionPareadaV2::PairUnverified)
        ));
        assert_eq!(
            fs::read(&siblings[slot]).unwrap(),
            b"auxiliary reappeared after A2"
        );
        assert_eq!(
            fs::read(rutas.ubicacion.journal().as_path()).unwrap(),
            bytes_journal
        );
        assert_eq!(fs::read(&rutas.contexto).unwrap(), bytes_contexto);
        drop(owner);
        drop(lock);
        verificar_lock_y_contexto(&rutas, Some(&bytes_contexto));
    }
}

#[test]
fn b2a_rechaza_ausencia_final_de_journal_o_contexto_y_preserva_original() {
    for slot in [0, 2] {
        let temp = tempdir().unwrap();
        let rutas = preparar_rutas(&temp);
        let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let (bytes_journal, bytes_contexto) =
            escribir_pareja(&owner, &rutas, DiferenciaBinding::Ninguna);
        let siblings = nombres_siblings_b2a(&rutas);
        let retenido = rutas.contexto.parent().unwrap().join(if slot == 0 {
            "journal-original-after-b2a"
        } else {
            "context-original-after-b2a"
        });
        let bytes = if slot == 0 {
            bytes_journal.clone()
        } else {
            bytes_contexto.clone()
        };
        let id_original = identidad_real(&siblings[slot]);

        let resultado = owner
            .observar_pareja_v2_con_slots_verificados_pendiente_identidad_con_prueba(
                |_| fs::rename(&siblings[slot], &retenido).unwrap(),
                |owner| owner.parent_revalidado(),
            );

        assert!(matches!(
            resultado,
            Err(ErrorObservacionPareadaV2::PairUnverified)
        ));
        assert!(!siblings[slot].exists());
        assert_eq!(fs::read(&retenido).unwrap(), bytes);
        assert_eq!(identidad_real(&retenido), id_original);
        if slot == 0 {
            assert_eq!(fs::read(&rutas.contexto).unwrap(), bytes_contexto);
        } else {
            assert_eq!(
                fs::read(rutas.ubicacion.journal().as_path()).unwrap(),
                bytes_journal
            );
        }
        drop(owner);
        drop(lock);
        verificar_lock_y_contexto(
            &rutas,
            if slot == 0 {
                Some(&bytes_contexto)
            } else {
                None
            },
        );
    }
}

#[test]
fn b2a_postcheck_exterior_domina_candidato_y_errores_del_cuerpo() {
    for caso in 0..4 {
        let temp = tempdir().unwrap();
        let rutas = preparar_rutas(&temp);
        let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let diferencia = if caso == 1 {
            DiferenciaBinding::TransactionId
        } else {
            DiferenciaBinding::Ninguna
        };
        let (bytes_journal, bytes_contexto) = escribir_pareja(&owner, &rutas, diferencia);
        let siblings = nombres_siblings_b2a(&rutas);
        if caso == 3 {
            fs::write(&siblings[4], b"initial intent").unwrap();
        }
        let postcheck_llamado = Cell::new(false);

        let resultado = owner
            .observar_pareja_v2_con_slots_verificados_pendiente_identidad_con_prueba(
                |_| {
                    if caso == 2 {
                        fs::write(&siblings[4], b"intent after A2").unwrap();
                    }
                },
                |_| {
                    postcheck_llamado.set(true);
                    Err(crate::transaccion::anclado::journal_temporal::ErrorJournalAnclado::Io)
                },
            );

        assert!(matches!(
            resultado,
            Err(ErrorObservacionPareadaV2::OwnerUnverified)
        ));
        assert!(postcheck_llamado.get());
        if caso == 2 {
            assert_eq!(fs::read(&siblings[4]).unwrap(), b"intent after A2");
        }
        if caso == 3 {
            assert_eq!(fs::read(&siblings[4]).unwrap(), b"initial intent");
        }
        assert_eq!(
            fs::read(rutas.ubicacion.journal().as_path()).unwrap(),
            bytes_journal
        );
        assert_eq!(fs::read(&rutas.contexto).unwrap(), bytes_contexto);
        drop(owner);
        drop(lock);
        verificar_lock_y_contexto(&rutas, Some(&bytes_contexto));
    }
}

#[test]
fn b2a_conserva_pareja_y_deja_identidad_final_pendiente() {
    let temp = tempdir().unwrap();
    let rutas = preparar_rutas(&temp);
    let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let (bytes_journal, bytes_contexto) =
        escribir_pareja(&owner, &rutas, DiferenciaBinding::Ninguna);

    let observado = owner
        .observar_pareja_v2_con_slots_verificados_pendiente_identidad()
        .unwrap();

    assert_eq!(
        format!("{observado:?}"),
        "ObservacionParejaV2ConSlotsVerificadosPendienteIdentidad { .. }"
    );
    assert_eq!(
        fs::read(rutas.ubicacion.journal().as_path()).unwrap(),
        bytes_journal
    );
    assert_eq!(fs::read(&rutas.contexto).unwrap(), bytes_contexto);
    drop(observado);
    drop(owner);
    drop(lock);
    verificar_lock_y_contexto(&rutas, Some(&bytes_contexto));
}

#[test]
fn b2b_confirma_identidades_despues_de_b2a_y_conserva_frames() {
    let temp = tempdir().unwrap();
    let rutas = preparar_rutas(&temp);
    let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let (bytes_journal, bytes_contexto) =
        escribir_pareja(&owner, &rutas, DiferenciaBinding::Ninguna);

    let observado = owner.observar_pareja_v2_con_identidades_finales().unwrap();

    assert_eq!(
        format!("{observado:?}"),
        "ObservacionParejaV2ConIdentidadesFinales { .. }"
    );
    assert_eq!(
        fs::read(rutas.ubicacion.journal().as_path()).unwrap(),
        bytes_journal
    );
    assert_eq!(fs::read(&rutas.contexto).unwrap(), bytes_contexto);
    drop(observado);
    drop(owner);
    drop(lock);
    verificar_lock_y_contexto(&rutas, Some(&bytes_contexto));
}

#[test]
fn b2b_rechaza_journal_y_contexto_reemplazados_despues_del_guard_b2a() {
    for reemplazar_journal in [true, false] {
        let temp = tempdir().unwrap();
        let rutas = preparar_rutas(&temp);
        let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let (bytes_journal, bytes_contexto) =
            escribir_pareja(&owner, &rutas, DiferenciaBinding::Ninguna);
        let siblings = nombres_siblings_b2a(&rutas);
        let indice = if reemplazar_journal { 0 } else { 2 };
        let parent = rutas.contexto.parent().unwrap();
        let retenido = parent.join(if reemplazar_journal {
            "journal-original-retained-b2b"
        } else {
            "context-original-retained-b2b"
        });
        let fuente = parent.join(if reemplazar_journal {
            "journal-replacement-source-b2b"
        } else {
            "context-replacement-source-b2b"
        });
        let bytes_reemplazo = if reemplazar_journal {
            bytes_journal.clone()
        } else {
            bytes_contexto.clone()
        };
        fs::write(&fuente, &bytes_reemplazo).unwrap();
        let id_original = identidad_real(&siblings[indice]);
        let id_reemplazo = identidad_real(&fuente);
        assert_ne!(id_original, id_reemplazo);
        let hook_llamado = Cell::new(false);

        let resultado = owner.observar_pareja_v2_con_identidades_finales_con_prueba(
            |_| {
                hook_llamado.set(true);
                fs::rename(&siblings[indice], &retenido).unwrap();
                fs::rename(&fuente, &siblings[indice]).unwrap();
            },
            |owner| owner.parent_revalidado(),
        );

        assert!(hook_llamado.get());
        assert!(matches!(
            resultado,
            Err(ErrorObservacionPareadaV2::PairUnverified)
        ));
        assert_eq!(fs::read(&retenido).unwrap(), bytes_reemplazo);
        assert_eq!(fs::read(&siblings[indice]).unwrap(), bytes_reemplazo);
        assert_eq!(identidad_real(&retenido), id_original);
        assert_eq!(identidad_real(&siblings[indice]), id_reemplazo);

        let probe_liberado = parent.join(if reemplazar_journal {
            "journal-probe-released-b2b"
        } else {
            "context-probe-released-b2b"
        });
        fs::rename(&siblings[indice], &probe_liberado).unwrap();
        assert_eq!(identidad_real(&probe_liberado), id_reemplazo);
        fs::rename(&probe_liberado, &siblings[indice]).unwrap();
        if reemplazar_journal {
            assert_eq!(fs::read(&rutas.contexto).unwrap(), bytes_contexto);
        } else {
            assert_eq!(
                fs::read(rutas.ubicacion.journal().as_path()).unwrap(),
                bytes_journal
            );
        }
        drop(owner);
        drop(lock);
        verificar_lock_y_contexto(&rutas, Some(&bytes_contexto));
    }
}

#[test]
fn b2b_rechaza_ausencia_final_de_journal_o_contexto_y_preserva_original() {
    for indice in [0, 2] {
        let temp = tempdir().unwrap();
        let rutas = preparar_rutas(&temp);
        let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let (bytes_journal, bytes_contexto) =
            escribir_pareja(&owner, &rutas, DiferenciaBinding::Ninguna);
        let siblings = nombres_siblings_b2a(&rutas);
        let retenido = rutas.contexto.parent().unwrap().join(if indice == 0 {
            "journal-original-absent-b2b"
        } else {
            "context-original-absent-b2b"
        });
        let bytes = if indice == 0 {
            bytes_journal.clone()
        } else {
            bytes_contexto.clone()
        };
        let id_original = identidad_real(&siblings[indice]);

        let resultado = owner.observar_pareja_v2_con_identidades_finales_con_prueba(
            |_| fs::rename(&siblings[indice], &retenido).unwrap(),
            |owner| owner.parent_revalidado(),
        );

        assert!(matches!(
            resultado,
            Err(ErrorObservacionPareadaV2::PairUnverified)
        ));
        assert!(!siblings[indice].exists());
        assert_eq!(fs::read(&retenido).unwrap(), bytes);
        assert_eq!(identidad_real(&retenido), id_original);
        if indice == 0 {
            assert_eq!(fs::read(&rutas.contexto).unwrap(), bytes_contexto);
        } else {
            assert_eq!(
                fs::read(rutas.ubicacion.journal().as_path()).unwrap(),
                bytes_journal
            );
        }
        drop(owner);
        drop(lock);
        verificar_lock_y_contexto(
            &rutas,
            if indice == 0 {
                Some(&bytes_contexto)
            } else {
                None
            },
        );
    }
}

#[test]
fn b2b_postcheck_exterior_domina_candidato_error_b2a_y_error_de_probe() {
    for caso in 0..3 {
        let temp = tempdir().unwrap();
        let rutas = preparar_rutas(&temp);
        let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let diferencia = if caso == 1 {
            DiferenciaBinding::TransactionId
        } else {
            DiferenciaBinding::Ninguna
        };
        let (bytes_journal, bytes_contexto) = escribir_pareja(&owner, &rutas, diferencia);
        let siblings = nombres_siblings_b2a(&rutas);
        let retenido = rutas
            .contexto
            .parent()
            .unwrap()
            .join("journal-original-postcheck-b2b");
        let id_original = if caso == 2 {
            Some(identidad_real(&siblings[0]))
        } else {
            None
        };
        let hook_llamado = Cell::new(false);
        let postcheck_llamado = Cell::new(false);

        let resultado = owner.observar_pareja_v2_con_identidades_finales_con_prueba(
            |_| {
                hook_llamado.set(true);
                if caso == 2 {
                    fs::rename(&siblings[0], &retenido).unwrap();
                }
            },
            |_| {
                postcheck_llamado.set(true);
                Err(crate::transaccion::anclado::journal_temporal::ErrorJournalAnclado::Io)
            },
        );

        assert!(matches!(
            resultado,
            Err(ErrorObservacionPareadaV2::OwnerUnverified)
        ));
        assert!(postcheck_llamado.get());
        assert_eq!(hook_llamado.get(), caso != 1);
        if caso == 2 {
            assert!(!siblings[0].exists());
            assert_eq!(fs::read(&retenido).unwrap(), bytes_journal);
            assert_eq!(identidad_real(&retenido), id_original.unwrap());
            assert_eq!(fs::read(&rutas.contexto).unwrap(), bytes_contexto);
        } else {
            assert_eq!(
                fs::read(rutas.ubicacion.journal().as_path()).unwrap(),
                bytes_journal
            );
            assert_eq!(fs::read(&rutas.contexto).unwrap(), bytes_contexto);
        }
        drop(owner);
        drop(lock);
        verificar_lock_y_contexto(&rutas, Some(&bytes_contexto));
    }
}
