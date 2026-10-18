use super::*;

use crate::transaccion::anclado::duenio_transaccion::DuenioTransaccionAnclado;
use crate::transaccion::lock::LockEscritura;
use crate::transaccion::raiz::UbicacionRaiz;
use std::fs;
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};

const LOCK_CANARY: &[u8] = b"intent versioned owner lock canary";

struct RutasOwner {
    fixture: IntentFixture,
    ubicacion: UbicacionRaiz,
    intent: std::path::PathBuf,
    lock: std::path::PathBuf,
}

fn preparar_rutas_owner(version: VersionIntentFixture) -> RutasOwner {
    let mut fixture = intent_fixture(version);
    let parent = fixture._directorio.path();
    let ubicacion = UbicacionRaiz::nueva(&parent.join("bundle")).unwrap();
    let basename = ubicacion.ruta().file_name().unwrap();
    let nombre_intent = crate::transaccion::anclado::duenio_transaccion::observar_siblings_nuevo_flujo::nombre_intent_retiro(basename).unwrap();
    let intent = parent.join(nombre_intent);

    drop(fixture.archivo_intent);
    fs::rename(&fixture.ruta_intent, &intent).unwrap();
    fixture.ruta_intent = intent.clone();
    fixture.archivo_intent = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&intent)
        .unwrap();
    assert!(identidad_archivo(&fixture.archivo_intent).unwrap() == fixture.identidad_intent);

    let lock = ubicacion.lock().as_path().to_path_buf();
    fs::write(&lock, LOCK_CANARY).unwrap();
    RutasOwner {
        fixture,
        ubicacion,
        intent,
        lock,
    }
}

fn preparar_rutas_retiro_a(binding_discordante: bool) -> RutasOwner {
    let mut rutas = preparar_rutas_owner(VersionIntentFixture::V2);
    let journal = journal_v2(rutas.fixture.parent_journal.clone());
    let journal_contexto = if binding_discordante {
        journal_v2(another_directory_identity(&rutas.fixture.parent_journal))
    } else {
        journal.clone()
    };
    let bytes_journal = journal.encode().unwrap();
    let bytes_contexto =
        crate::transaccion::admision::envelope::v2::EnvelopeContextoAdmisionV2::ligar(
            crate::transaccion::admision::envelope::tests::contexto(b"\n"),
            &journal_contexto,
        )
        .encode()
        .unwrap();
    let identidad_journal =
        identidad_archivo(&File::open(&rutas.fixture.ruta_journal).unwrap()).unwrap();
    let identidad_contexto =
        identidad_archivo(&File::open(&rutas.fixture.ruta_contexto).unwrap()).unwrap();
    let identidad_intent = rutas.fixture.identidad_intent;
    let longitud_contexto = u64::try_from(bytes_contexto.len()).unwrap();
    let sha256_contexto: [u8; 32] = sha2::Sha256::digest(&bytes_contexto).into();
    let bytes_intent = IntentRetiroPersistidoV2::desde_datos_persistidos(
        journal,
        crate::transaccion::anclado::identidad_archivo::persistida::IdentidadArchivoPersistida::try_from(
            identidad_intent,
        )
        .unwrap(),
        crate::transaccion::anclado::identidad_archivo::persistida::IdentidadArchivoPersistida::try_from(
            identidad_journal,
        )
        .unwrap(),
        crate::transaccion::anclado::identidad_archivo::persistida::IdentidadArchivoPersistida::try_from(
            identidad_contexto,
        )
        .unwrap(),
        rutas.fixture.scope.0,
        longitud_contexto,
        sha256_contexto,
    )
    .unwrap()
    .encode()
    .unwrap();

    fs::write(&rutas.fixture.ruta_journal, &bytes_journal).unwrap();
    fs::write(&rutas.fixture.ruta_contexto, &bytes_contexto).unwrap();
    fs::write(&rutas.intent, &bytes_intent).unwrap();

    let journal_canonico = rutas.ubicacion.journal().as_path().to_path_buf();
    let basename = rutas.ubicacion.ruta().file_name().unwrap();
    let nombre_contexto =
        crate::transaccion::anclado::contexto_admision::nombre_canonico(basename).unwrap();
    let contexto_canonico = rutas.fixture._directorio.path().join(nombre_contexto);
    fs::rename(&rutas.fixture.ruta_journal, &journal_canonico).unwrap();
    fs::rename(&rutas.fixture.ruta_contexto, &contexto_canonico).unwrap();
    rutas.fixture.ruta_journal = journal_canonico;
    rutas.fixture.ruta_contexto = contexto_canonico;
    rutas.fixture.bytes_journal = bytes_journal;
    rutas.fixture.bytes_contexto = bytes_contexto;
    rutas.fixture.bytes_intent = bytes_intent;
    rutas
}

#[test]
fn retiro_a_committed_retiene_los_tres_frames_originales_y_redacta_debug() {
    let rutas = preparar_rutas_retiro_a(false);
    let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

    let observacion = owner
        .observar_retiro_a_con_slots_revalidados_pendiente_identidad_final()
        .unwrap();

    assert_eq!(
        format!("{observacion:?}"),
        "ObservacionRetiroAConSlotsRevalidadosPendienteIdentidadFinal { .. }"
    );
    drop(observacion);
    drop(owner);
    drop(lock);
    assert_owner_files_preserved(&rutas);
}

#[test]
fn binding_discordante_rechaza_retiro_a_y_preserva_los_tres_frames() {
    let rutas = preparar_rutas_retiro_a(true);
    let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

    assert!(matches!(
        owner.observar_retiro_a_con_slots_revalidados_pendiente_identidad_final(),
        Err(crate::transaccion::anclado::duenio_transaccion::retained_v2_pair::ErrorObservacionRetiroA::ObservationUnverified)
    ));
    drop(owner);
    drop(lock);
    assert_owner_files_preserved(&rutas);
}

#[test]
fn slots_retiro_a_invalidos_al_inicio_o_despues_de_leer_fallan_cerrado() {
    for caso in 0_usize..7 {
        let mut rutas = preparar_rutas_retiro_a(false);
        let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let nombres = NombresIntentVersionado::desde_owner(&owner).unwrap();
        let temporales = nombres.temporales_os_str();
        let temporal = rutas
            .intent
            .parent()
            .unwrap()
            .join(temporales[if caso == 6 { 2 } else { caso.saturating_sub(3) }]);
        let faltante = match caso {
            0 => Some(rutas.fixture.ruta_journal.as_path()),
            1 => Some(rutas.fixture.ruta_contexto.as_path()),
            2 => Some(rutas.intent.as_path()),
            _ => None,
        };
        if let Some(path) = faltante {
            if caso == 2 {
                let reemplazo = File::open(&rutas.fixture.ruta_contexto).unwrap();
                drop(std::mem::replace(
                    &mut rutas.fixture.archivo_intent,
                    reemplazo,
                ));
            }
            fs::remove_file(path).unwrap();
        } else if caso < 6 {
            fs::write(&temporal, b"temporary sibling must fail closed").unwrap();
        }

        if caso == 6 {
            let observado = owner
                .observar_retiro_a_con_slots_revalidados_pendiente_identidad_final_con_prueba(
                    |_| fs::write(&temporal, b"late temporary must fail closed").unwrap(),
                    |owner| owner.parent_revalidado(),
                );
            assert!(matches!(
                observado,
                Err(crate::transaccion::anclado::duenio_transaccion::retained_v2_pair::ErrorObservacionRetiroA::ObservationUnverified)
            ));
        } else {
            assert!(matches!(
                owner.observar_retiro_a_con_slots_revalidados_pendiente_identidad_final(),
                Err(crate::transaccion::anclado::duenio_transaccion::retained_v2_pair::ErrorObservacionRetiroA::ObservationUnverified)
            ));
        }

        if faltante.is_none() {
            fs::remove_file(&temporal).unwrap();
        }
        drop(owner);
        drop(lock);
        if let Some(path) = faltante {
            assert!(!path.exists());
            assert_eq!(fs::read(&rutas.lock).unwrap(), LOCK_CANARY);
            let siblings = [
                (
                    rutas.fixture.ruta_journal.as_path(),
                    rutas.fixture.bytes_journal.as_slice(),
                ),
                (
                    rutas.fixture.ruta_contexto.as_path(),
                    rutas.fixture.bytes_contexto.as_slice(),
                ),
                (
                    rutas.intent.as_path(),
                    rutas.fixture.bytes_intent.as_slice(),
                ),
            ];
            for (indice, (path, bytes)) in siblings.iter().enumerate() {
                if indice != caso {
                    assert_eq!(fs::read(path).unwrap().as_slice(), *bytes);
                }
            }
        } else {
            assert_owner_files_preserved(&rutas);
        }
    }
}

#[test]
fn postcheck_exterior_retiro_a_domina_exito_y_error_de_binding() {
    for binding_discordante in [false, true] {
        let rutas = preparar_rutas_retiro_a(binding_discordante);
        let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let cuerpo_llamado = std::cell::Cell::new(false);
        let postcheck_llamado = std::cell::Cell::new(false);

        let resultado = owner
            .observar_retiro_a_con_slots_revalidados_pendiente_identidad_final_con_prueba(
                |_| cuerpo_llamado.set(true),
                |_| {
                    postcheck_llamado.set(true);
                    Err(crate::transaccion::anclado::journal_temporal::ErrorJournalAnclado::Io)
                },
            );

        assert!(matches!(
            resultado,
            Err(crate::transaccion::anclado::duenio_transaccion::retained_v2_pair::ErrorObservacionRetiroA::OwnerUnverified)
        ));
        assert!(postcheck_llamado.get());
        assert_eq!(cuerpo_llamado.get(), !binding_discordante);
        drop(owner);
        drop(lock);
        assert_owner_files_preserved(&rutas);
    }
}

#[test]
fn retiro_a_con_identidades_finales_preserva_los_tres_frames() {
    let rutas = preparar_rutas_retiro_a(false);
    let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

    let observacion = owner.observar_retiro_a_con_identidades_finales().unwrap();

    assert_eq!(
        format!("{observacion:?}"),
        "ObservacionRetiroAConIdentidadesFinales { .. }"
    );
    drop(observacion);
    drop(owner);
    drop(lock);
    assert_owner_files_preserved(&rutas);
}

#[test]
fn identidades_finales_rechazan_reemplazo_de_cada_frame_con_bytes_iguales() {
    for caso in 0..3 {
        let mut rutas = preparar_rutas_retiro_a(false);
        let (canonico, bytes) = canonicos_retiro_a(&rutas)[caso].clone();
        if caso == 2 {
            liberar_handle_intent_fixture(&mut rutas.fixture);
        }
        let retenido = canonico.with_file_name(format!("original-ra2-{caso}"));
        let fuente = canonico.with_file_name(format!("reemplazo-ra2-{caso}"));
        fs::write(&fuente, &bytes).unwrap();
        let identidad_original = identidad_archivo(&File::open(&canonico).unwrap()).unwrap();
        let identidad_reemplazo = identidad_archivo(&File::open(&fuente).unwrap()).unwrap();
        assert_ne!(identidad_original, identidad_reemplazo);

        let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let resultado = owner.observar_retiro_a_con_identidades_finales_con_prueba(
            |_| {
                fs::rename(&canonico, &retenido).unwrap();
                fs::rename(&fuente, &canonico).unwrap();
            },
            |owner| owner.parent_revalidado(),
        );

        assert!(matches!(
            resultado,
            Err(crate::transaccion::anclado::duenio_transaccion::retained_v2_pair::ErrorObservacionRetiroA::ObservationUnverified)
        ));
        drop(owner);
        drop(lock);
        assert_eq!(fs::read(&rutas.lock).unwrap(), LOCK_CANARY);
        assert_eq!(fs::read(&retenido).unwrap(), bytes);
        assert_eq!(fs::read(&canonico).unwrap(), bytes);
        assert_eq!(
            identidad_archivo(&File::open(&retenido).unwrap()).unwrap(),
            identidad_original
        );
        assert_eq!(
            identidad_archivo(&File::open(&canonico).unwrap()).unwrap(),
            identidad_reemplazo
        );
        for (indice, (path, bytes)) in canonicos_retiro_a(&rutas).iter().enumerate() {
            if indice != caso {
                assert_eq!(fs::read(path).unwrap().as_slice(), bytes.as_slice());
            }
        }

        let despues_de_probe = canonico.with_file_name(format!("despues-de-probe-ra2-{caso}"));
        fs::rename(&canonico, &despues_de_probe).unwrap();
        assert_eq!(fs::read(&despues_de_probe).unwrap(), bytes);
    }
}

#[test]
fn identidades_finales_rechazan_ausencia_de_cada_frame_canonico() {
    for caso in 0..3 {
        let mut rutas = preparar_rutas_retiro_a(false);
        let (canonico, bytes) = canonicos_retiro_a(&rutas)[caso].clone();
        if caso == 2 {
            liberar_handle_intent_fixture(&mut rutas.fixture);
        }
        let retenido = canonico.with_file_name(format!("ausente-ra2-{caso}"));
        let identidad_original = identidad_archivo(&File::open(&canonico).unwrap()).unwrap();
        let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

        let resultado = owner.observar_retiro_a_con_identidades_finales_con_prueba(
            |_| fs::rename(&canonico, &retenido).unwrap(),
            |owner| owner.parent_revalidado(),
        );

        assert!(matches!(
            resultado,
            Err(crate::transaccion::anclado::duenio_transaccion::retained_v2_pair::ErrorObservacionRetiroA::ObservationUnverified)
        ));
        drop(owner);
        drop(lock);
        assert_eq!(fs::read(&rutas.lock).unwrap(), LOCK_CANARY);
        assert!(!canonico.exists());
        assert_eq!(fs::read(&retenido).unwrap(), bytes);
        assert_eq!(
            identidad_archivo(&File::open(&retenido).unwrap()).unwrap(),
            identidad_original
        );
        for (indice, (path, bytes)) in canonicos_retiro_a(&rutas).iter().enumerate() {
            if indice != caso {
                assert_eq!(fs::read(path).unwrap().as_slice(), bytes.as_slice());
            }
        }
    }
}

#[test]
fn postcheck_exterior_final_domina_candidato_error_ra1_y_error_de_probe() {
    for caso in 0..3 {
        let rutas = preparar_rutas_retiro_a(caso == 1);
        let journal = rutas.ubicacion.journal().as_path().to_path_buf();
        let retenido = journal.with_file_name("journal-oculto-postcheck-ra2");
        let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let hook_llamado = std::cell::Cell::new(false);
        let postcheck_llamado = std::cell::Cell::new(false);

        let resultado = owner.observar_retiro_a_con_identidades_finales_con_prueba(
            |_| {
                hook_llamado.set(true);
                if caso == 2 {
                    fs::rename(&journal, &retenido).unwrap();
                }
            },
            |_| {
                postcheck_llamado.set(true);
                Err(crate::transaccion::anclado::journal_temporal::ErrorJournalAnclado::Io)
            },
        );

        assert!(matches!(
            resultado,
            Err(crate::transaccion::anclado::duenio_transaccion::retained_v2_pair::ErrorObservacionRetiroA::OwnerUnverified)
        ));
        assert!(postcheck_llamado.get());
        assert_eq!(hook_llamado.get(), caso != 1);
        drop(owner);
        drop(lock);
        assert_eq!(fs::read(&rutas.lock).unwrap(), LOCK_CANARY);
        if caso == 2 {
            assert!(!journal.exists());
            assert_eq!(fs::read(&retenido).unwrap(), rutas.fixture.bytes_journal);
            assert_eq!(
                fs::read(&rutas.fixture.ruta_contexto).unwrap(),
                rutas.fixture.bytes_contexto
            );
            assert_eq!(fs::read(&rutas.intent).unwrap(), rutas.fixture.bytes_intent);
        } else {
            assert_owner_files_preserved(&rutas);
        }
    }
}

fn canonicos_retiro_a(rutas: &RutasOwner) -> [(std::path::PathBuf, Vec<u8>); 3] {
    [
        (
            rutas.ubicacion.journal().as_path().to_path_buf(),
            rutas.fixture.bytes_journal.clone(),
        ),
        (
            rutas.fixture.ruta_contexto.clone(),
            rutas.fixture.bytes_contexto.clone(),
        ),
        (rutas.intent.clone(), rutas.fixture.bytes_intent.clone()),
    ]
}

fn liberar_handle_intent_fixture(fixture: &mut IntentFixture) {
    let reemplazo = fs::File::open(&fixture.ruta_contexto).unwrap();
    drop(std::mem::replace(&mut fixture.archivo_intent, reemplazo));
}

fn assert_owner_files_preserved(rutas: &RutasOwner) {
    assert_eq!(fs::read(&rutas.intent).unwrap(), rutas.fixture.bytes_intent);
    assert_owner_sidecars_preserved(rutas);
}

fn assert_owner_sidecars_preserved(rutas: &RutasOwner) {
    assert_eq!(fs::read(&rutas.lock).unwrap(), LOCK_CANARY);
    assert_eq!(
        fs::read(&rutas.fixture.ruta_journal).unwrap(),
        rutas.fixture.bytes_journal
    );
    assert_eq!(
        fs::read(&rutas.fixture.ruta_contexto).unwrap(),
        rutas.fixture.bytes_contexto
    );
}

fn ocultar_intent_canonico(rutas: &mut RutasOwner) -> std::path::PathBuf {
    let reemplazo = fs::File::open(&rutas.fixture.ruta_contexto).unwrap();
    drop(std::mem::replace(
        &mut rutas.fixture.archivo_intent,
        reemplazo,
    ));

    let retenido = rutas.intent.with_file_name("retained-intent-frame");
    fs::rename(&rutas.intent, &retenido).unwrap();
    retenido
}

fn sustituir_intent_por_frame_invalido(rutas: &mut RutasOwner, bytes: &[u8]) {
    rutas.fixture.archivo_intent.set_len(0).unwrap();
    rutas
        .fixture
        .archivo_intent
        .seek(SeekFrom::Start(0))
        .unwrap();
    rutas.fixture.archivo_intent.write_all(bytes).unwrap();
    rutas.fixture.archivo_intent.flush().unwrap();
    rutas.fixture.bytes_intent = bytes.to_vec();
}

#[test]
fn owner_observa_intent_v1_y_v2_y_preserva_archivos() {
    for version in [VersionIntentFixture::V1, VersionIntentFixture::V2] {
        let rutas = preparar_rutas_owner(version);
        let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let expected =
            intent_persistido::decodificar_versionado(&rutas.fixture.bytes_intent).unwrap();

        let observation = owner
            .observar_intent_versionado()
            .unwrap()
            .expect("a present canonical Intent yields an observation");

        assert_eq!(
            format!("{observation:?}"),
            "ObservacionIntentVersionada { .. }"
        );
        assert_linked_matches(&observation.candidato, &expected, version);
        assert!(observation.candidato.identidad_archivo == rutas.fixture.identidad_intent);
        assert!(observation.candidato.scope == rutas.fixture.scope);
        assert!(observation.candidato.parent_journal == rutas.fixture.parent_journal);
        assert!(
            identidad_archivo(&observation.candidato.archivo_original).unwrap()
                == rutas.fixture.identidad_intent
        );
        drop(observation);
        drop(owner);
        drop(lock);
        assert_owner_files_preserved(&rutas);
    }
}

#[test]
fn owner_rechaza_temporal_preexistente_y_temporal_surgido_tras_lectura() {
    let rutas_pre = preparar_rutas_owner(VersionIntentFixture::V1);
    let mut lock_pre = LockEscritura::adquirir_en(&rutas_pre.ubicacion).unwrap();
    let mut owner_pre = DuenioTransaccionAnclado::desde_lock(&mut lock_pre).unwrap();
    let nombres_pre = NombresIntentVersionado::desde_owner(&owner_pre).unwrap();
    let temporal_pre = rutas_pre
        .intent
        .parent()
        .unwrap()
        .join(&nombres_pre.temporales[0]);
    fs::write(&temporal_pre, b"temporary present before read").unwrap();

    let error = owner_pre.observar_intent_versionado().unwrap_err();
    assert_eq!(error, ErrorObservacionIntentVersionada::OwnerUnverified);
    drop(owner_pre);
    drop(lock_pre);
    assert_owner_files_preserved(&rutas_pre);
    assert_eq!(
        fs::read(&temporal_pre).unwrap(),
        b"temporary present before read"
    );

    let rutas_post = preparar_rutas_owner(VersionIntentFixture::V2);
    let mut lock_post = LockEscritura::adquirir_en(&rutas_post.ubicacion).unwrap();
    let mut owner_post = DuenioTransaccionAnclado::desde_lock(&mut lock_post).unwrap();
    let nombres_post = NombresIntentVersionado::desde_owner(&owner_post).unwrap();
    let temporal_post = rutas_post
        .intent
        .parent()
        .unwrap()
        .join(&nombres_post.temporales[1]);

    let error = owner_post
        .observar_intent_versionado_con_hook_prueba(|| {
            fs::write(&temporal_post, b"temporary after read").unwrap();
        })
        .unwrap_err();

    assert_eq!(error, ErrorObservacionIntentVersionada::OwnerUnverified);
    drop(owner_post);
    drop(lock_post);
    assert_owner_files_preserved(&rutas_post);
    assert_eq!(fs::read(&temporal_post).unwrap(), b"temporary after read");
}

#[test]
fn error_decoder_se_preserva_y_falla_postcheck_lo_domina() {
    let invalid = b"malformed private intent frame";
    let mut rutas_lectura = preparar_rutas_owner(VersionIntentFixture::V1);
    sustituir_intent_por_frame_invalido(&mut rutas_lectura, invalid);
    let mut lock_lectura = LockEscritura::adquirir_en(&rutas_lectura.ubicacion).unwrap();
    let mut owner_lectura = DuenioTransaccionAnclado::desde_lock(&mut lock_lectura).unwrap();

    let error = match owner_lectura.observar_intent_versionado() {
        Ok(_) => panic!("malformed Intent unexpectedly decoded"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        ErrorObservacionIntentVersionada::Candidate(ErrorIntentVersionadoVinculado::Candidato(
            ErrorCandidatoIntentSinVinculo::Selector(_)
        ))
    ));
    drop(owner_lectura);
    drop(lock_lectura);
    assert_owner_files_preserved(&rutas_lectura);

    let mut rutas_postcheck = preparar_rutas_owner(VersionIntentFixture::V2);
    sustituir_intent_por_frame_invalido(&mut rutas_postcheck, invalid);
    let mut lock_postcheck = LockEscritura::adquirir_en(&rutas_postcheck.ubicacion).unwrap();
    let mut owner_postcheck = DuenioTransaccionAnclado::desde_lock(&mut lock_postcheck).unwrap();
    let nombres = NombresIntentVersionado::desde_owner(&owner_postcheck).unwrap();
    let temporal = rutas_postcheck
        .intent
        .parent()
        .unwrap()
        .join(&nombres.temporales[2]);

    let error = owner_postcheck
        .observar_intent_versionado_con_hook_prueba(|| {
            fs::write(&temporal, b"postcheck dominates decoder error").unwrap();
        })
        .unwrap_err();
    assert_eq!(error, ErrorObservacionIntentVersionada::OwnerUnverified);
    drop(owner_postcheck);
    drop(lock_postcheck);
    assert_owner_files_preserved(&rutas_postcheck);
    assert_eq!(
        fs::read(&temporal).unwrap(),
        b"postcheck dominates decoder error"
    );
}

#[test]
fn ausencia_canonica_exacta_en_dos_sondas_devuelve_none_y_preserva_archivos() {
    for version in [VersionIntentFixture::V1, VersionIntentFixture::V2] {
        let mut rutas = preparar_rutas_owner(version);
        let retenido = ocultar_intent_canonico(&mut rutas);
        let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

        let resultado = owner.observar_intent_versionado();

        assert!(matches!(resultado, Ok(None)));
        drop(owner);
        drop(lock);
        assert!(!rutas.intent.exists());
        assert_eq!(fs::read(&retenido).unwrap(), rutas.fixture.bytes_intent);
        assert!(
            identidad_archivo(&fs::File::open(&retenido).unwrap()).unwrap()
                == rutas.fixture.identidad_intent
        );
        assert_owner_sidecars_preserved(&rutas);
    }
}

#[test]
fn reaparicion_tras_primera_ausencia_y_tipo_no_admitido_no_son_none() {
    let mut rutas_reaparece = preparar_rutas_owner(VersionIntentFixture::V1);
    let retenido = ocultar_intent_canonico(&mut rutas_reaparece);
    let mut lock_reaparece = LockEscritura::adquirir_en(&rutas_reaparece.ubicacion).unwrap();
    let mut owner_reaparece = DuenioTransaccionAnclado::desde_lock(&mut lock_reaparece).unwrap();

    let resultado =
        owner_reaparece.observar_intent_versionado_despues_primera_ausencia_prueba(|| {
            fs::rename(&retenido, &rutas_reaparece.intent).unwrap()
        });
    assert!(matches!(
        resultado,
        Err(ErrorObservacionIntentVersionada::CanonicalUnverified)
    ));
    drop(owner_reaparece);
    drop(lock_reaparece);
    assert_owner_files_preserved(&rutas_reaparece);
    assert!(!retenido.exists());
    assert!(
        identidad_archivo(&fs::File::open(&rutas_reaparece.intent).unwrap()).unwrap()
            == rutas_reaparece.fixture.identidad_intent
    );

    let mut rutas_directorio = preparar_rutas_owner(VersionIntentFixture::V2);
    let retenido_directorio = ocultar_intent_canonico(&mut rutas_directorio);
    let mut lock_directorio = LockEscritura::adquirir_en(&rutas_directorio.ubicacion).unwrap();
    let mut owner_directorio = DuenioTransaccionAnclado::desde_lock(&mut lock_directorio).unwrap();

    let resultado =
        owner_directorio.observar_intent_versionado_despues_primera_ausencia_prueba(|| {
            fs::create_dir(&rutas_directorio.intent).unwrap()
        });
    assert!(matches!(
        resultado,
        Err(ErrorObservacionIntentVersionada::CanonicalUnverified)
    ));
    drop(owner_directorio);
    drop(lock_directorio);
    assert!(rutas_directorio.intent.is_dir());
    assert_eq!(
        fs::read(&retenido_directorio).unwrap(),
        rutas_directorio.fixture.bytes_intent
    );
    assert_owner_sidecars_preserved(&rutas_directorio);
}

#[test]
fn postcheck_fallido_domina_none_candidata_y_preserva_objetos() {
    let mut rutas = preparar_rutas_owner(VersionIntentFixture::V1);
    let retenido = ocultar_intent_canonico(&mut rutas);
    let mut lock = LockEscritura::adquirir_en(&rutas.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let nombres = NombresIntentVersionado::desde_owner(&owner).unwrap();
    let temporal = rutas.intent.parent().unwrap().join(&nombres.temporales[0]);

    let resultado = owner.observar_intent_versionado_con_hook_prueba(|| {
        fs::write(&temporal, b"postcheck rejects absent Intent result").unwrap();
    });

    assert!(matches!(
        resultado,
        Err(ErrorObservacionIntentVersionada::OwnerUnverified)
    ));
    drop(owner);
    drop(lock);
    assert!(!rutas.intent.exists());
    assert_eq!(fs::read(&retenido).unwrap(), rutas.fixture.bytes_intent);
    assert_eq!(
        fs::read(&temporal).unwrap(),
        b"postcheck rejects absent Intent result"
    );
    assert_owner_sidecars_preserved(&rutas);
}
