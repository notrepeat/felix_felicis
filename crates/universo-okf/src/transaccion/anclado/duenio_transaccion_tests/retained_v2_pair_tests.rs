use super::*;

use crate::transaccion::admision::envelope::v2::EnvelopeContextoAdmisionV2;
use crate::transaccion::anclado::duenio_transaccion::retained_v2_pair::ErrorObservacionPareadaV2;
use crate::transaccion::journal::JournalState;
use crate::transaccion::journal::v2::JournalV2;

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
