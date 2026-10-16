use crate::transaccion::admision::envelope::{EnvelopeContextoAdmision, codec, tests as fixtures};
use crate::transaccion::anclado::journal_temporal::{
    IdentidadArchivoJournal, ResultadoPublicacionJournal,
};
use crate::transaccion::anclado::{
    abrir_directorio_hijo_lectura, identidad_archivo, metadata_directorio,
};
use crate::transaccion::journal::{ArtifactFingerprint, Journal, JournalState};
use crate::transaccion::lock::LockEscritura;
use crate::transaccion::raiz::UbicacionRaiz;
use crate::transaccion::replay::new_esperado::calcular_new_esperado;
use crate::transaccion::snapshot::{EntradaSnapshot, TipoEntradaSnapshot};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use tempfile::tempdir;

pub(super) struct FixturePublicada {
    pub(super) ubicacion: UbicacionRaiz,
    pub(super) root: PathBuf,
    pub(super) entradas: Vec<EntradaSnapshot>,
    pub(super) esperado: ArtifactFingerprint,
    pub(super) bytes_contexto: Vec<u8>,
    pub(super) bytes_journal: Vec<u8>,
    pub(super) id_journal_publicado: IdentidadArchivoJournal,
}

pub(super) fn contexto_path(ubicacion: &UbicacionRaiz) -> PathBuf {
    let mut nombre = std::ffi::OsString::from(".");
    nombre.push(ubicacion.ruta().file_name().unwrap());
    nombre.push(".felix.admctx");
    ubicacion.ruta().parent().unwrap().join(nombre)
}

pub(super) fn materializar(root: &Path, entradas: &[EntradaSnapshot]) {
    for entrada in entradas {
        let destino = root.join(entrada.ruta());
        match entrada.tipo() {
            TipoEntradaSnapshot::Directorio => fs::create_dir_all(destino).unwrap(),
            TipoEntradaSnapshot::Archivo => {
                fs::create_dir_all(destino.parent().unwrap()).unwrap();
                fs::write(destino, entrada.bytes().unwrap()).unwrap();
            }
        }
    }
}

pub(super) fn preparar(parent: &Path) -> FixturePublicada {
    preparar_con_tags(parent, fixtures::MOTOR, fixtures::POLICY)
}

pub(super) fn preparar_con_tags(
    parent: &Path,
    motor_soportado: &str,
    policy_soportada: &str,
) -> FixturePublicada {
    fs::create_dir_all(parent).unwrap();
    let root = parent.join("bundle");
    fs::create_dir(&root).unwrap();
    let ubicacion = UbicacionRaiz::nueva(&root).unwrap();
    let contexto = fixtures::contexto_con_tags(b"\n", motor_soportado, policy_soportada);
    let fixture_journal =
        fixtures::journal_valido_con_tags(&contexto, motor_soportado, policy_soportada);
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner = super::DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let identidad_parent = owner.identidad_parent.journal_identity().unwrap();
    let journal_provisional = Journal::new(
        fixture_journal.transaction_id(),
        identidad_parent.clone(),
        fixture_journal.old().clone(),
        fixture_journal.new_fingerprint().clone(),
    )
    .unwrap();
    let new = calcular_new_esperado(
        &contexto,
        &journal_provisional,
        motor_soportado,
        policy_soportada,
    )
    .unwrap();
    let entradas = new.entradas().to_vec();
    materializar(&root, &entradas);

    let root_handle =
        abrir_directorio_hijo_lectura(&owner.parent, ubicacion.ruta().file_name().unwrap())
            .unwrap();
    let identidad_root = metadata_directorio(&root_handle)
        .unwrap()
        .journal_identity()
        .unwrap();
    let esperado = ArtifactFingerprint::new(identidad_root, new.digest().clone());
    let journal = Journal::new(
        fixture_journal.transaction_id(),
        identidad_parent,
        fixture_journal.old().clone(),
        esperado.clone(),
    )
    .unwrap();
    let envelope = EnvelopeContextoAdmision::ligar(contexto, &journal);
    let bytes_contexto = codec::codificar(&envelope).unwrap();
    let bytes_journal = journal.encode().unwrap();

    {
        let temporal = owner
            .contexto_admision()
            .unwrap()
            .crear_temporal(&envelope)
            .unwrap();
        let verificado = temporal.verificar().unwrap();
        crate::transaccion::anclado::contexto_admision::publicar_temporal_para_fixture(verificado)
            .unwrap();
    }
    let id_journal_publicado = {
        let lease = owner.journal_temporal().unwrap();
        let identidad = lease.crear_nuevo(&bytes_journal).unwrap();
        assert!(matches!(
            lease.publicar(identidad, &journal),
            ResultadoPublicacionJournal::Confirmado { .. }
        ));
        identidad
    };
    drop(owner);
    drop(lock);

    FixturePublicada {
        ubicacion,
        root,
        entradas,
        esperado,
        bytes_contexto,
        bytes_journal,
        id_journal_publicado,
    }
}

pub(super) fn id_archivo(path: &Path) -> crate::transaccion::anclado::IdentidadArchivo {
    identidad_archivo(&File::open(path).unwrap()).unwrap()
}

pub(super) fn verificar_arbol(root: &Path, entradas: &[EntradaSnapshot]) {
    for entrada in entradas {
        let path = root.join(entrada.ruta());
        match entrada.tipo() {
            TipoEntradaSnapshot::Directorio => assert!(path.is_dir()),
            TipoEntradaSnapshot::Archivo => {
                assert_eq!(fs::read(path).unwrap(), entrada.bytes().unwrap());
            }
        }
    }
}

pub(super) fn assert_storage_unchanged(
    fixture: &FixturePublicada,
    bytes_contexto: &[u8],
    bytes_journal: &[u8],
    id_contexto: crate::transaccion::anclado::IdentidadArchivo,
    id_journal: crate::transaccion::anclado::IdentidadArchivo,
) {
    let path_contexto = contexto_path(&fixture.ubicacion);
    let path_journal = fixture.ubicacion.journal().as_path().to_path_buf();
    assert_eq!(fs::read(path_contexto).unwrap(), bytes_contexto);
    assert_eq!(fs::read(&path_journal).unwrap(), bytes_journal);
    assert!(id_archivo(&contexto_path(&fixture.ubicacion)) == id_contexto);
    assert!(id_archivo(&path_journal) == id_journal);
}

#[test]
fn fixture_prepara_y_reabre_replay_con_root_new_real() {
    let temp = tempdir().unwrap();
    let fixture = preparar(&temp.path().join("parent"));
    verificar_arbol(&fixture.root, &fixture.entradas);

    let path_contexto = contexto_path(&fixture.ubicacion);
    let path_journal = fixture.ubicacion.journal().as_path().to_path_buf();
    let id_contexto = id_archivo(&path_contexto);
    let id_journal = id_archivo(&path_journal);
    let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
    let mut owner = super::DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

    let (id_journal_reabierto, journal_reabierto, bytes_journal) = {
        let lease = owner.journal_temporal().unwrap();
        let (id_bytes, bytes) = lease.abrir_y_leer_canonico().unwrap().unwrap();
        let (id_record, journal) = lease.cargar_canonico().unwrap().unwrap();
        assert!(id_bytes == id_record);
        (id_record, journal, bytes)
    };
    let (envelope, id_contexto_reabierto) = {
        let lease = owner.contexto_admision().unwrap();
        lease.leer_canonico().unwrap().unwrap()
    };
    let replay = envelope
        .validar_replay(&journal_reabierto, fixtures::MOTOR, fixtures::POLICY)
        .unwrap();
    let root_observado = owner.observar_root_raw().unwrap().unwrap();

    let parent = owner.identidad_parent.journal_identity().unwrap();
    assert!(journal_reabierto.parent() == &parent);
    assert_eq!(journal_reabierto.state(), JournalState::Prepared);
    assert!(journal_reabierto.new_fingerprint() == &fixture.esperado);
    assert!(replay.new_fingerprint() == &fixture.esperado);
    assert!(root_observado == fixture.esperado);
    assert!(id_contexto_reabierto == id_contexto);
    assert!(id_journal_reabierto == fixture.id_journal_publicado);
    assert_eq!(bytes_journal, fixture.bytes_journal);
    assert_storage_unchanged(
        &fixture,
        &fixture.bytes_contexto,
        &fixture.bytes_journal,
        id_contexto,
        id_journal,
    );
}
