use super::*;
use crate::transaccion::anclado::duenio_transaccion::DuenioTransaccionAnclado;
use crate::transaccion::anclado::{IdentidadArchivo, identidad_archivo};
use crate::transaccion::journal::v2::JournalV2;
use crate::transaccion::journal::{ArtifactFingerprint, DirectoryIdentity, RawTreeDigest};
use crate::transaccion::lock::LockEscritura;
use crate::transaccion::raiz::UbicacionRaiz;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use tempfile::tempdir;

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

fn identidad_parent(temp: &Path) -> DirectoryIdentity {
    let parent = super::super::abrir_directorio_absoluto(temp, true).unwrap();
    metadata_directorio(&parent)
        .unwrap()
        .journal_identity()
        .unwrap()
}

#[test]
fn lease_estricto_crea_y_relee_el_journal_v2_completo() {
    let temp = tempdir().unwrap();
    let ubicacion = ubicacion_en(temp.path());
    let temporal = ruta_temporal(&ubicacion);
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let journal = journal_v2(identidad_parent(temp.path()));

    let identidad_creada = {
        let lease = owner.journal_temporal_estricto().unwrap();
        lease.crear_temporal_v2(&journal).unwrap()
    };

    let bytes = fs::read(&temporal).unwrap();
    let observado = JournalV2::parse(&bytes).unwrap();
    assert!(observado == journal);
    assert_eq!(
        identidad_visible(&temporal),
        identidad_creada.identidad_archivo()
    );
}

#[test]
fn lease_legacy_sin_scope_rechaza_v2_antes_de_crear() {
    let temp = tempdir().unwrap();
    let ubicacion = ubicacion_en(temp.path());
    let temporal = ruta_temporal(&ubicacion);
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let journal = journal_v2(identidad_parent(temp.path()));

    let error = {
        let lease = owner.journal_temporal().unwrap();
        lease.crear_temporal_v2(&journal).err().unwrap()
    };

    assert_eq!(error, ErrorJournalAnclado::ParentScopeUnverified);
    assert!(!temporal.exists());
}

#[test]
fn lease_v2_preserva_temporal_ocupado_y_su_identidad() {
    let temp = tempdir().unwrap();
    let ubicacion = ubicacion_en(temp.path());
    let temporal = ruta_temporal(&ubicacion);
    let canary = b"occupied-temporal-canary";
    fs::write(&temporal, canary).unwrap();
    let identidad_antes = identidad_visible(&temporal);
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let journal = journal_v2(identidad_parent(temp.path()));

    let error = {
        let lease = owner.journal_temporal_estricto().unwrap();
        lease.crear_temporal_v2(&journal).err().unwrap()
    };

    assert_eq!(error, ErrorJournalAnclado::Exists);
    assert_eq!(fs::read(&temporal).unwrap(), canary);
    assert_eq!(identidad_visible(&temporal), identidad_antes);
}
