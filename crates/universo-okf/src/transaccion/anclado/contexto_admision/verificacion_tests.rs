use super::*;
use crate::transaccion::anclado::{
    archivo_temporal::DatosArchivoTemporal, duenio_transaccion::DuenioTransaccionAnclado,
    identidad_archivo,
};
use crate::transaccion::lock::LockEscritura;
use crate::transaccion::raiz::UbicacionRaiz;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use tempfile::{TempDir, tempdir};

fn ubicacion_temporal() -> (TempDir, UbicacionRaiz) {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    fs::create_dir(&parent).unwrap();
    let root = parent.join("bundle");
    let ubicacion = UbicacionRaiz::nueva(&root).unwrap();
    (temp, ubicacion)
}

fn ruta_canonica(ubicacion: &UbicacionRaiz) -> std::path::PathBuf {
    let parent = ubicacion.ruta().parent().unwrap();
    let basename = ubicacion.ruta().file_name().unwrap();
    parent.join(super::super::nombre_canonico(basename).unwrap())
}

fn evidencia(file: &File, bytes: &[u8]) -> EvidenciaLecturaContexto {
    EvidenciaLecturaContexto {
        identidad_archivo: identidad_archivo(file).unwrap(),
        longitud: u64::try_from(bytes.len()).unwrap(),
        sha256: Sha256::digest(bytes).into(),
    }
}

fn owner<'lock>(lock: &'lock mut LockEscritura) -> DuenioTransaccionAnclado<'lock> {
    DuenioTransaccionAnclado::desde_lock(lock).unwrap()
}

#[test]
fn ausencia_raw_es_none_y_los_wrappers_conservan_sus_errores() {
    let (_temp, ubicacion) = ubicacion_temporal();
    let canonical = ruta_canonica(&ubicacion);
    let bytes = b"raw canonical frame";
    fs::write(&canonical, bytes).unwrap();
    let evidencia = evidencia(&File::open(&canonical).unwrap(), bytes);
    fs::remove_file(&canonical).unwrap();

    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner = owner(&mut lock);
    let lease = owner.contexto_admision().unwrap();
    assert!(
        verificar_canonico_opcional(&lease, &evidencia)
            .unwrap()
            .is_none()
    );
    let error = lease
        .verificar_canonico_con_evidencia(&evidencia)
        .unwrap_err();
    assert_eq!(error, ErrorContextoAdmisionAnclado::CanonicalUnverified);
    assert_eq!(error.codigo(), "admission_context_canonical_unverified");

    let datos_temporales = DatosArchivoTemporal {
        identidad_archivo: evidencia.identidad_archivo,
        longitud: bytes.len(),
        digest: evidencia.sha256,
    };
    assert_eq!(
        verificar_archivo(
            &lease,
            std::ffi::OsStr::new("missing.tmp"),
            &datos_temporales
        )
        .unwrap_err(),
        ErrorContextoAdmisionAnclado::TemporaryUnverified
    );
}

#[test]
fn discrepancia_de_bytes_sigue_siendo_error_y_no_ausencia() {
    let (_temp, ubicacion) = ubicacion_temporal();
    let canonical = ruta_canonica(&ubicacion);
    let original = b"frame A";
    let modificado = b"frame B";
    assert_eq!(original.len(), modificado.len());
    fs::write(&canonical, original).unwrap();
    let evidencia = evidencia(&File::open(&canonical).unwrap(), original);

    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner = owner(&mut lock);
    let lease = owner.contexto_admision().unwrap();
    fs::write(&canonical, modificado).unwrap();
    assert_eq!(
        identidad_archivo(&File::open(&canonical).unwrap()).unwrap(),
        evidencia.identidad_archivo
    );

    let error = verificar_canonico_opcional(&lease, &evidencia).unwrap_err();
    assert_eq!(error, ErrorContextoAdmisionAnclado::CanonicalUnverified);
    assert_eq!(error.codigo(), "admission_context_canonical_unverified");
    assert_eq!(fs::read(&canonical).unwrap(), modificado);
}
