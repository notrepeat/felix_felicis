//! Vincula el frame al handle recibido; no prueba el nombre canónico ni el owner.
use std::fs::File;
use std::io::{Seek, SeekFrom};

use super::{ErrorLecturaIntentRetiroV3, IntentRetiroPersistidoV3};
use crate::transaccion::anclado::identidad_archivo::persistida::IdentidadArchivoPersistida;
use crate::transaccion::anclado::{
    ScopeFilesystem, identidad_archivo, metadata_es_reparse, scope_filesystem,
};
use crate::transaccion::journal::DirectoryIdentity;

#[derive(Debug, PartialEq, Eq)]
pub(in crate::transaccion::anclado) enum ErrorArchivoIntentV3 {
    HandleUnverified,
    BindingMismatch,
    Read(ErrorLecturaIntentRetiroV3),
}

/// Retiene el mismo handle observado. No constituye una capability de mutación.
pub(in crate::transaccion::anclado) struct IntentV3EnArchivo {
    pub(in crate::transaccion::anclado) archivo: File,
    pub(in crate::transaccion::anclado) intent: IntentRetiroPersistidoV3,
}

pub(in crate::transaccion::anclado) fn leer_archivo(
    mut archivo: File,
    scope_esperado: ScopeFilesystem,
    parent_esperado: &DirectoryIdentity,
) -> Result<IntentV3EnArchivo, ErrorArchivoIntentV3> {
    let antes = observar(&archivo, scope_esperado)?;
    archivo
        .seek(SeekFrom::Start(0))
        .map_err(|_| ErrorArchivoIntentV3::HandleUnverified)?;
    let lectura = IntentRetiroPersistidoV3::leer(&mut archivo);
    // Una discrepancia posterior domina incluso un error del decoder.
    let despues = observar(&archivo, scope_esperado)?;
    if antes != despues {
        return Err(ErrorArchivoIntentV3::BindingMismatch);
    }
    let intent = lectura.map_err(ErrorArchivoIntentV3::Read)?;
    intent
        .metadata()
        .validar_vinculo_archivo_intent_datos(&antes, scope_esperado.0)
        .map_err(|_| ErrorArchivoIntentV3::BindingMismatch)?;
    if intent.metadata().journal().parent() != parent_esperado {
        return Err(ErrorArchivoIntentV3::BindingMismatch);
    }
    Ok(IntentV3EnArchivo { archivo, intent })
}

fn observar(
    archivo: &File,
    esperado: ScopeFilesystem,
) -> Result<IdentidadArchivoPersistida, ErrorArchivoIntentV3> {
    let metadata = archivo
        .metadata()
        .map_err(|_| ErrorArchivoIntentV3::HandleUnverified)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata_es_reparse(&metadata) {
        return Err(ErrorArchivoIntentV3::HandleUnverified);
    }
    let scope = scope_filesystem(archivo).map_err(|_| ErrorArchivoIntentV3::HandleUnverified)?;
    if scope != esperado {
        return Err(ErrorArchivoIntentV3::BindingMismatch);
    }
    let identidad =
        identidad_archivo(archivo).map_err(|_| ErrorArchivoIntentV3::HandleUnverified)?;
    IdentidadArchivoPersistida::try_from(identidad)
        .map_err(|_| ErrorArchivoIntentV3::HandleUnverified)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaccion::anclado::intent_persistido_v2::IntentRetiroPersistidoV2;
    use sha2::{Digest, Sha256};
    use std::io::Write;

    fn fixture() -> (File, ScopeFilesystem, DirectoryIdentity, Vec<u8>) {
        let mut file = tempfile::tempfile().unwrap();
        let scope = scope_filesystem(&file).unwrap();
        let id = observar(&file, scope).unwrap();
        let (journal, context, _) = super::super::tests::committed_fixture();
        let parent = journal.parent().clone();
        let metadata = IntentRetiroPersistidoV2::desde_datos_persistidos(
            journal,
            id.clone(),
            id.clone(),
            id,
            scope.0,
            context.len() as u64,
            Sha256::digest(&context).into(),
        )
        .unwrap();
        let frame = IntentRetiroPersistidoV3::encode(&metadata, &context).unwrap();
        file.write_all(&frame).unwrap();
        (file, scope, parent, frame)
    }

    #[test]
    fn conserva_handle_y_rebobina_antes_de_leer() {
        let (file, scope, parent, _) = fixture();
        let original = identidad_archivo(&file).unwrap();
        let result = leer_archivo(file, scope, &parent).unwrap();
        assert_eq!(identidad_archivo(&result.archivo).unwrap(), original);
        assert!(result.intent.metadata().journal().parent() == &parent);
    }

    #[test]
    fn copia_del_frame_en_otro_archivo_no_hereda_identidad() {
        let (_original, scope, parent, frame) = fixture();
        let mut replacement = tempfile::tempfile().unwrap();
        replacement.write_all(&frame).unwrap();
        assert!(matches!(
            leer_archivo(replacement, scope, &parent),
            Err(ErrorArchivoIntentV3::BindingMismatch)
        ));
    }

    #[test]
    fn scope_parent_y_truncacion_se_rechazan() {
        let (file, scope, parent, _) = fixture();
        assert!(matches!(
            leer_archivo(file, ScopeFilesystem(scope.0.wrapping_add(1)), &parent),
            Err(ErrorArchivoIntentV3::BindingMismatch)
        ));
        let (file, scope, _, _) = fixture();
        let (other_parent, _, _) = super::super::tests::committed_fixture();
        // Una identidad de root no coincide con el parent de la fixture.
        assert!(matches!(
            leer_archivo(file, scope, other_parent.new_fingerprint().identity()),
            Err(ErrorArchivoIntentV3::BindingMismatch)
        ));
        let (file, scope, parent, _) = fixture();
        file.set_len(3).unwrap();
        assert!(matches!(
            leer_archivo(file, scope, &parent),
            Err(ErrorArchivoIntentV3::Read(_))
        ));
    }
}
