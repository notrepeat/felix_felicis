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

fn assert_owner_files_preserved(rutas: &RutasOwner) {
    assert_eq!(fs::read(&rutas.lock).unwrap(), LOCK_CANARY);
    assert_eq!(fs::read(&rutas.intent).unwrap(), rutas.fixture.bytes_intent);
    assert_eq!(
        fs::read(&rutas.fixture.ruta_journal).unwrap(),
        rutas.fixture.bytes_journal
    );
    assert_eq!(
        fs::read(&rutas.fixture.ruta_contexto).unwrap(),
        rutas.fixture.bytes_contexto
    );
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

        let observation = owner.observar_intent_versionado().unwrap();

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
