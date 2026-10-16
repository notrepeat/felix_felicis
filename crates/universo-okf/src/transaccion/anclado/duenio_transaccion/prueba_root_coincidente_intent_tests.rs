use super::*;

use std::ffi::OsString;

const INTENT_CANARY: &[u8] = b"unread retirement intent canary";

fn intent_path(fixture: &FixturePublicada) -> PathBuf {
    let basename = fixture.ubicacion.ruta().file_name().unwrap();
    let mut name = OsString::from(".");
    name.push(basename);
    name.push(".felix.admctx-retire");
    fixture.ubicacion.ruta().parent().unwrap().join(name)
}

fn intent_temporal_path(fixture: &FixturePublicada) -> PathBuf {
    let canonical = intent_path(fixture);
    let mut name = canonical.file_name().unwrap().to_os_string();
    name.push(".tmp");
    canonical.with_file_name(name)
}

#[test]
fn intent_canonico_o_temporal_presente_bloquea_emision_y_preserva_storage() {
    for temporal in [false, true] {
        let temp = tempdir().unwrap();
        let fixture = preparar_soportado(&temp.path().join("parent"));
        fijar_estado(&fixture, JournalState::Committed);
        let storage = capturar_storage(&fixture);
        let path = if temporal {
            intent_temporal_path(&fixture)
        } else {
            intent_path(&fixture)
        };
        fs::write(&path, INTENT_CANARY).unwrap();
        let id_intent = id_archivo(&path);
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

        assert!(owner.probar_replay_admitido_root_coincidente().is_err());

        verificar_storage(&fixture, &storage);
        assert_eq!(fs::read(&path).unwrap(), INTENT_CANARY);
        assert!(id_archivo(&path) == id_intent);
        verificar_arbol(&fixture.root, &fixture.entradas);
    }
}

#[test]
fn intent_aparecido_invalida_prueba_previamente_emitida_y_se_preserva() {
    for temporal in [false, true] {
        let temp = tempdir().unwrap();
        let fixture = preparar_soportado(&temp.path().join("parent"));
        fijar_estado(&fixture, JournalState::Committed);
        let storage = capturar_storage(&fixture);
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let prueba = owner.probar_replay_admitido_root_coincidente().unwrap();
        let path = if temporal {
            intent_temporal_path(&fixture)
        } else {
            intent_path(&fixture)
        };
        fs::write(&path, INTENT_CANARY).unwrap();
        let id_intent = id_archivo(&path);

        assert!(prueba.revalidar().is_err());

        verificar_storage(&fixture, &storage);
        assert_eq!(fs::read(&path).unwrap(), INTENT_CANARY);
        assert!(id_archivo(&path) == id_intent);
        verificar_arbol(&fixture.root, &fixture.entradas);
    }
}
