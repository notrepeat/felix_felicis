use super::*;
use crate::transaccion::anclado::intent_persistido::{
    IntentRetiroPersistido, MAX_INTENT_BYTES, codificar_nuevo,
};
use crate::transaccion::anclado::{abrir_directorio_absoluto, scope_filesystem};
use crate::transaccion::journal::Journal;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::path::Path;

fn assert_lease_rejected<T, E>(result: Result<T, E>)
where
    T: std::fmt::Debug,
    E: std::fmt::Debug + std::fmt::Display,
{
    let error = result.unwrap_err();
    assert_eq!(error.to_string(), "transaction_owner_unverified");
    assert_eq!(
        format!("{error:?}"),
        "ErrorDuenioTransaccionAnclado(\"transaction_owner_unverified\")"
    );
}

fn frame_with_changed_context_digest(
    fixture: &super::FixtureFilaConservarNew,
    intent_path: &Path,
) -> Vec<u8> {
    let journal = Journal::parse(&fixture.publicada.bytes_journal).unwrap();
    let location = &fixture.publicada.ubicacion;
    let parent_path = location.ruta().parent().unwrap();
    let parent = abrir_directorio_absoluto(parent_path, true).unwrap();
    let scope = scope_filesystem(&parent).unwrap();
    let journal_location = location.journal();
    let journal_path = journal_location.as_path();
    let context_path = contexto_path(location);
    let context_bytes = fs::read(&context_path).unwrap();
    let mut context_digest: [u8; 32] = Sha256::digest(&context_bytes).into();
    context_digest[0] ^= 1;

    codificar_nuevo(
        &journal,
        id_archivo(intent_path),
        id_archivo(journal_path),
        id_archivo(&context_path),
        scope.0,
        u64::try_from(context_bytes.len()).unwrap(),
        context_digest,
    )
    .unwrap()
}

#[test]
fn mismo_id_y_longitud_con_otro_registro_valido_falla_y_preserva_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (canonico, _) = intent_paths(&fixture);
    let bytes_originales = fs::read(&canonico).unwrap();
    let registro_original = IntentRetiroPersistido::decode(&bytes_originales).unwrap();
    let identidad_original = id_archivo(&canonico);
    let ids_storage = capturar_storage(&fixture);
    let bytes_modificados = frame_with_changed_context_digest(&fixture, &canonico);
    let registro_modificado = IntentRetiroPersistido::decode(&bytes_modificados).unwrap();
    assert_ne!(registro_original, registro_modificado);
    assert_eq!(bytes_modificados.len(), bytes_originales.len());

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let observacion = owner.observar_intent_canonico().unwrap();
    fs::write(&canonico, &bytes_modificados).unwrap();
    assert!(id_archivo(&canonico) == identidad_original);

    assert_lease_rejected(observacion.abrir_lease_retiro());

    assert_eq!(fs::read(&canonico).unwrap(), bytes_modificados);
    assert!(id_archivo(&canonico) == identidad_original);
    verificar_storage(&fixture, ids_storage);
}

#[test]
fn mismos_bytes_en_otro_id_falla_y_conserva_el_original_retenido() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (canonico, _) = intent_paths(&fixture);
    let original_retenido = canonico.with_file_name("retained-original-intent");
    let bytes = fs::read(&canonico).unwrap();
    let _archivo_original = File::open(&canonico).unwrap();
    let identidad_original = id_archivo(&canonico);
    let ids_storage = capturar_storage(&fixture);
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let observacion = owner.observar_intent_canonico().unwrap();

    fs::rename(&canonico, &original_retenido).unwrap();
    fs::write(&canonico, &bytes).unwrap();
    let identidad_reemplazo = id_archivo(&canonico);
    assert!(identidad_original != identidad_reemplazo);

    assert_lease_rejected(observacion.abrir_lease_retiro());

    assert_eq!(fs::read(&original_retenido).unwrap(), bytes);
    assert_eq!(fs::read(&canonico).unwrap(), bytes);
    assert!(id_archivo(&original_retenido) == identidad_original);
    assert!(id_archivo(&canonico) == identidad_reemplazo);
    verificar_storage(&fixture, ids_storage);
}

#[test]
fn frames_malformados_y_oversize_fallan_sin_alterar_id_ni_contenido() {
    for tipo in ["truncado", "extra", "oversize"] {
        let temp = tempfile::tempdir().unwrap();
        let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
        let (canonico, _) = intent_paths(&fixture);
        let bytes_originales = fs::read(&canonico).unwrap();
        assert!(IntentRetiroPersistido::decode(&bytes_originales).is_ok());
        let identidad_original = id_archivo(&canonico);
        let ids_storage = capturar_storage(&fixture);
        let mut bytes_prueba = bytes_originales.clone();
        match tipo {
            "truncado" => {
                bytes_prueba.pop().unwrap();
            }
            "extra" => bytes_prueba.push(0xa5),
            "oversize" => bytes_prueba.resize(MAX_INTENT_BYTES + 1, 0xa5),
            _ => unreachable!(),
        }
        if tipo == "oversize" {
            assert_eq!(bytes_prueba.len(), 1537);
        }

        let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let observacion = owner.observar_intent_canonico().unwrap();
        fs::write(&canonico, &bytes_prueba).unwrap();
        assert!(id_archivo(&canonico) == identidad_original);

        assert_lease_rejected(observacion.abrir_lease_retiro());

        assert_eq!(fs::read(&canonico).unwrap(), bytes_prueba);
        assert!(id_archivo(&canonico) == identidad_original);
        verificar_storage(&fixture, ids_storage);
    }
}
