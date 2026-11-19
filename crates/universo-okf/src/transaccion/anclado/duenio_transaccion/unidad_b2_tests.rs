use super::*;

#[derive(Clone, Copy)]
enum MutacionIntent {
    MismoLargo,
    Truncado,
    BytesExtra,
}

fn cambia_intent_y_rechaza(mutacion: MutacionIntent) {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
    avanzar_a(&mut fixture, JournalState::Committed);
    let ruta_journal = fixture
        .publicada
        .ubicacion
        .journal()
        .as_path()
        .to_path_buf();
    let ruta_contexto = contexto_path(&fixture.publicada.ubicacion);
    let (_, ruta_intent) = intent_paths(&fixture);
    let bytes_journal = fs::read(&ruta_journal).unwrap();
    let bytes_contexto = fs::read(&ruta_contexto).unwrap();
    let id_journal = id_archivo(&ruta_journal);
    let id_contexto = id_archivo(&ruta_contexto);
    let id_root = identidad_directorio(&fixture.publicada.root);

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let verificado = escribir_intent(&mut owner).verificar_temporal().unwrap();
    let bytes_originales = fs::read(&ruta_intent).unwrap();
    let id_original = id_archivo(&ruta_intent);
    let mut bytes_mutados = bytes_originales.clone();
    match mutacion {
        MutacionIntent::MismoLargo => {
            let indice = bytes_mutados.len() - 1;
            bytes_mutados[indice] ^= 1;
        }
        MutacionIntent::Truncado => {
            bytes_mutados.pop().expect("intent frame no vacío");
        }
        MutacionIntent::BytesExtra => bytes_mutados.push(0xa5),
    }

    if let Err(error) = fs::write(&ruta_intent, &bytes_mutados) {
        #[cfg(windows)]
        {
            assert!(matches!(
                error.raw_os_error(),
                Some(5) | Some(32) | Some(33)
            ));
            drop(verificado);
            assert_eq!(fs::read(&ruta_intent).unwrap(), bytes_originales);
            assert!(id_archivo(&ruta_intent) == id_original);
            drop(owner);
            drop(lock);
            eprintln!(
                "Windows bloqueó la mutación del intent abierto; esta variante no se ejercitó."
            );
            return;
        }
        #[cfg(not(windows))]
        panic!("no se pudo mutar el intent temporal: {error}");
    }

    assert!(id_archivo(&ruta_intent) == id_original);
    assert_eq!(fs::read(&ruta_intent).unwrap(), bytes_mutados);

    assert_rechazo(verificado.revalidar_completo());

    assert_eq!(fs::read(&ruta_journal).unwrap(), bytes_journal);
    assert_eq!(fs::read(&ruta_contexto).unwrap(), bytes_contexto);
    assert!(id_archivo(&ruta_journal) == id_journal);
    assert!(id_archivo(&ruta_contexto) == id_contexto);
    assert!(identidad_directorio(&fixture.publicada.root) == id_root);
    verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
    assert_eq!(fs::read(&ruta_intent).unwrap(), bytes_mutados);
    assert!(id_archivo(&ruta_intent) == id_original);
}

#[test]
fn revalidacion_completa_rechaza_intent_mutado_mismo_id_truncado_o_con_extra() {
    for mutacion in [
        MutacionIntent::MismoLargo,
        MutacionIntent::Truncado,
        MutacionIntent::BytesExtra,
    ] {
        cambia_intent_y_rechaza(mutacion);
    }
}

#[test]
fn revalidacion_completa_rechaza_journal_o_contexto_ausentes_y_preserva_el_resto() {
    #[derive(Clone, Copy)]
    enum NombreAusente {
        Journal,
        Contexto,
    }

    for nombre_ausente in [NombreAusente::Journal, NombreAusente::Contexto] {
        let temp = tempdir().unwrap();
        let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
        avanzar_a(&mut fixture, JournalState::Committed);
        let ruta_journal = fixture
            .publicada
            .ubicacion
            .journal()
            .as_path()
            .to_path_buf();
        let ruta_contexto = contexto_path(&fixture.publicada.ubicacion);
        let (_, ruta_intent) = intent_paths(&fixture);
        let bytes_journal = fs::read(&ruta_journal).unwrap();
        let bytes_contexto = fs::read(&ruta_contexto).unwrap();
        let id_journal = id_archivo(&ruta_journal);
        let id_contexto = id_archivo(&ruta_contexto);
        let id_root = identidad_directorio(&fixture.publicada.root);

        let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let verificado = escribir_intent(&mut owner).verificar_temporal().unwrap();
        let bytes_intent = fs::read(&ruta_intent).unwrap();
        let id_intent = id_archivo(&ruta_intent);
        let ruta_eliminada = match nombre_ausente {
            NombreAusente::Journal => &ruta_journal,
            NombreAusente::Contexto => &ruta_contexto,
        };
        fs::remove_file(ruta_eliminada).unwrap();

        assert_rechazo(verificado.revalidar_completo());

        assert!(!ruta_eliminada.exists());
        match nombre_ausente {
            NombreAusente::Journal => {
                assert_eq!(fs::read(&ruta_contexto).unwrap(), bytes_contexto);
                assert!(id_archivo(&ruta_contexto) == id_contexto);
            }
            NombreAusente::Contexto => {
                assert_eq!(fs::read(&ruta_journal).unwrap(), bytes_journal);
                assert!(id_archivo(&ruta_journal) == id_journal);
            }
        }
        assert_eq!(fs::read(&ruta_intent).unwrap(), bytes_intent);
        assert!(id_archivo(&ruta_intent) == id_intent);
        assert!(identidad_directorio(&fixture.publicada.root) == id_root);
        verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
    }
}

#[test]
fn revalidacion_completa_rechaza_contexto_valido_mismo_id_y_longitud_con_sha_distinto() {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
    avanzar_a(&mut fixture, JournalState::Committed);
    let ruta_journal = fixture
        .publicada
        .ubicacion
        .journal()
        .as_path()
        .to_path_buf();
    let ruta_contexto = contexto_path(&fixture.publicada.ubicacion);
    let (_, ruta_intent) = intent_paths(&fixture);
    let bytes_journal = fs::read(&ruta_journal).unwrap();
    let bytes_contexto = fs::read(&ruta_contexto).unwrap();
    let id_journal = id_archivo(&ruta_journal);
    let id_contexto = id_archivo(&ruta_contexto);
    let id_root = identidad_directorio(&fixture.publicada.root);
    let journal = crate::transaccion::journal::Journal::parse(&bytes_journal).unwrap();
    let contexto_equivalente = crate::transaccion::admision::envelope::tests::contexto_con_tags(
        b" ",
        crate::transaccion::replay::semantica_archivada::VERSION_MOTOR_V1,
        crate::transaccion::replay::semantica_archivada::POLICY_ID_V1,
    );
    let envelope = crate::transaccion::admision::envelope::EnvelopeContextoAdmision::ligar(
        contexto_equivalente,
        &journal,
    );
    let replay = envelope.validar_replay_soportado(&journal).unwrap();
    assert!(replay.new_fingerprint() == journal.new_fingerprint());
    let bytes_contexto_equivalente =
        crate::transaccion::admision::envelope::codec::codificar(&envelope).unwrap();
    assert_ne!(bytes_contexto_equivalente, bytes_contexto);
    assert_eq!(bytes_contexto_equivalente.len(), bytes_contexto.len());
    let sha_original: [u8; 32] = <sha2::Sha256 as sha2::Digest>::digest(&bytes_contexto).into();
    let sha_equivalente: [u8; 32] =
        <sha2::Sha256 as sha2::Digest>::digest(&bytes_contexto_equivalente).into();
    assert_ne!(sha_equivalente, sha_original);

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let verificado = escribir_intent(&mut owner).verificar_temporal().unwrap();
    let bytes_intent = fs::read(&ruta_intent).unwrap();
    let id_intent = id_archivo(&ruta_intent);
    fs::write(&ruta_contexto, &bytes_contexto_equivalente).unwrap();
    assert!(id_archivo(&ruta_contexto) == id_contexto);
    assert_eq!(
        fs::read(&ruta_contexto).unwrap(),
        bytes_contexto_equivalente
    );

    assert_rechazo(verificado.revalidar_completo());

    assert_eq!(fs::read(&ruta_journal).unwrap(), bytes_journal);
    assert!(id_archivo(&ruta_journal) == id_journal);
    assert_eq!(
        fs::read(&ruta_contexto).unwrap(),
        bytes_contexto_equivalente
    );
    assert!(id_archivo(&ruta_contexto) == id_contexto);
    assert_eq!(fs::read(&ruta_intent).unwrap(), bytes_intent);
    assert!(id_archivo(&ruta_intent) == id_intent);
    assert!(identidad_directorio(&fixture.publicada.root) == id_root);
    verificar_arbol(&fixture.publicada.root, &fixture.publicada.entradas);
}
