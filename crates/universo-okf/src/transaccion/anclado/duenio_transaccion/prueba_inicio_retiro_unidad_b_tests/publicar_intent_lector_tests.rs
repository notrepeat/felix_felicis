use super::*;
use crate::transaccion::anclado::contexto_admision::nombre_temporal_sibling;
use crate::transaccion::anclado::intent_persistido::MAX_INTENT_BYTES;
use std::path::Path;

fn fixture_publicada_con_intent(parent: &Path) -> FixtureFilaConservarNew {
    let mut fixture = preparar_fixture(parent, false);
    avanzar_a(&mut fixture, JournalState::Committed);

    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let publicado = escribir_intent(&mut owner)
        .verificar_temporal()
        .unwrap()
        .revalidar_completo()
        .unwrap()
        .publicar_intent()
        .unwrap();
    drop(publicado);
    drop(owner);
    drop(lock);
    fixture
}

fn observar(fixture: &FixtureFilaConservarNew) -> Result<String, (String, String)> {
    let mut lock = LockEscritura::adquirir_en(&fixture.publicada.ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    match owner.observar_intent_canonico() {
        Ok(observacion) => Ok(format!("{observacion:?}")),
        Err(error) => Err((error.to_string(), format!("{error:?}"))),
    }
}

fn assert_no_verificable(fixture: &FixtureFilaConservarNew) {
    let (display, debug) = observar(fixture).unwrap_err();
    assert_eq!(display, "transaction_owner_unverified");
    assert_eq!(
        debug,
        "ErrorDuenioTransaccionAnclado(\"transaction_owner_unverified\")"
    );
}

#[test]
fn owner_fresco_observa_publicacion_canónica_y_revalida_id_y_scope() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));

    assert_eq!(
        observar(&fixture).unwrap(),
        "ObservacionIntentCanonico { .. }"
    );
}

#[test]
fn ausencia_de_intent_canonico_falla_cerrado() {
    let temp = tempdir().unwrap();
    let mut fixture = preparar_fixture(&temp.path().join("parent"), false);
    avanzar_a(&mut fixture, JournalState::Committed);
    let (canonico, temporal) = intent_paths(&fixture);

    assert!(!canonico.exists());
    assert!(!temporal.exists());
    assert_no_verificable(&fixture);
    assert!(!canonico.exists());
    assert!(!temporal.exists());
}

#[test]
fn intent_temporal_presente_bloquea_y_preserva_bytes_e_identidad() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (_, temporal) = intent_paths(&fixture);
    let canary = b"intent temporary canary";
    fs::write(&temporal, canary).unwrap();
    let identidad = id_archivo(&temporal);

    assert_no_verificable(&fixture);

    assert_eq!(fs::read(&temporal).unwrap(), canary);
    assert!(id_archivo(&temporal) == identidad);
}

#[test]
fn temporales_de_journal_y_contexto_bloquean_y_se_preservan() {
    for es_journal in [true, false] {
        let temp = tempdir().unwrap();
        let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
        let ubicacion = &fixture.publicada.ubicacion;
        let parent = ubicacion.ruta().parent().unwrap();
        let ruta_contexto = contexto_path(ubicacion);
        let nombre_contexto = ruta_contexto.file_name().unwrap();
        let temporal_contexto = parent.join(nombre_temporal_sibling(nombre_contexto).unwrap());
        let temporal_journal = parent.join(ubicacion.nombre_journal_temporal());
        let temporal = if es_journal {
            temporal_journal
        } else {
            temporal_contexto
        };
        let canary = b"reserved temporary canary";
        fs::write(&temporal, canary).unwrap();
        let identidad = id_archivo(&temporal);

        assert_no_verificable(&fixture);

        assert_eq!(fs::read(&temporal).unwrap(), canary);
        assert!(id_archivo(&temporal) == identidad);
    }
}

#[test]
fn mismo_frame_en_otro_id_canonico_falla_y_preserva_ambos_archivos() {
    let temp = tempdir().unwrap();
    let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
    let (canonico, _) = intent_paths(&fixture);
    let original = canonico.with_file_name("retained-original-intent");
    let bytes = fs::read(&canonico).unwrap();
    let identidad_original = id_archivo(&canonico);
    fs::rename(&canonico, &original).unwrap();
    fs::write(&canonico, &bytes).unwrap();
    let identidad_reemplazo = id_archivo(&canonico);
    assert!(identidad_original != identidad_reemplazo);

    assert_no_verificable(&fixture);

    assert_eq!(fs::read(&original).unwrap(), bytes);
    assert_eq!(fs::read(&canonico).unwrap(), bytes);
    assert!(id_archivo(&original) == identidad_original);
    assert!(id_archivo(&canonico) == identidad_reemplazo);
}

#[test]
fn frame_truncado_extra_y_oversize_fallan_sin_alterar_identidad() {
    for tipo in ["truncado", "extra", "oversize"] {
        let temp = tempdir().unwrap();
        let fixture = fixture_publicada_con_intent(&temp.path().join("parent"));
        let (canonico, _) = intent_paths(&fixture);
        let original = fs::read(&canonico).unwrap();
        let identidad = id_archivo(&canonico);
        let mut corrupto = original.clone();
        match tipo {
            "truncado" => {
                corrupto.pop().unwrap();
            }
            "extra" => corrupto.push(0xa5),
            "oversize" => corrupto.resize(MAX_INTENT_BYTES + 1, 0xa5),
            _ => unreachable!(),
        }
        fs::write(&canonico, &corrupto).unwrap();
        assert!(id_archivo(&canonico) == identidad);

        assert_no_verificable(&fixture);

        assert_eq!(fs::read(&canonico).unwrap(), corrupto);
        assert!(id_archivo(&canonico) == identidad);
    }
}

#[path = "publicar_intent_lector_revalidar_tests.rs"]
mod revalidar_tests;

#[path = "publicar_intent_lector_parent_tests.rs"]
mod parent_binding_tests;

#[path = "publicar_intent_lector_reconciliar_a_tests.rs"]
mod reconciliar_a_tests;
