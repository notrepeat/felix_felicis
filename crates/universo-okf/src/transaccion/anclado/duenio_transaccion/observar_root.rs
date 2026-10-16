use crate::transaccion::inventario::{LimitesInventario, inventariar_root_anclado_con_limites};
use crate::transaccion::journal::ArtifactFingerprint;
use crate::transaccion::snapshot::calcular_digest_raw_tree;

use super::super::{
    DirectorioAnclado, ErrorDirectorioAnclado, abrir_directorio_hijo_lectura, metadata_directorio,
    scope_filesystem, validar_componente,
};
use super::{DuenioTransaccionAnclado, ErrorDuenioTransaccionAnclado, mapear_error};

impl DuenioTransaccionAnclado<'_> {
    /// Observa el fingerprint raw del root desde el parent retenido.
    ///
    /// Devuelve datos de identidad y digest, no una autorización de admisión
    /// ni una capability de recovery o cleanup. El lock protege escritores
    /// cooperantes; el scope detectado no prueba una frontera general de mounts.
    pub(crate) fn observar_root_raw(
        &mut self,
    ) -> Result<Option<ArtifactFingerprint>, ErrorDuenioTransaccionAnclado> {
        let parent = self
            .parent_revalidado()
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        let ruta_root = self._lock.ubicacion().ruta();
        let basename = ruta_root
            .file_name()
            .ok_or(ErrorDuenioTransaccionAnclado::NoVerificable)?;
        let basename = validar_componente(basename).map_err(mapear_error)?;

        let identidad_parent = metadata_directorio(&parent).map_err(mapear_error)?;
        let scope_parent = scope_filesystem(&parent).map_err(mapear_error)?;
        if identidad_parent != self.identidad_parent || scope_parent != self.scope_parent {
            return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
        }

        let raiz = match abrir_directorio_hijo_lectura(&parent, basename) {
            Ok(raiz) => raiz,
            Err(ErrorDirectorioAnclado::EntradaAusente) => return Ok(None),
            Err(error) => return Err(mapear_error(error)),
        };
        let identidad_raiz = metadata_directorio(&raiz).map_err(mapear_error)?;
        if scope_filesystem(&raiz).map_err(mapear_error)? != scope_parent {
            return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
        }

        let ancla = DirectorioAnclado {
            parent,
            basename: basename.to_os_string(),
            raiz: Some(raiz),
            identidad_parent,
            identidad_raiz: Some(identidad_raiz),
            permite_escritura: false,
            puede_crear_stage_sibling: false,
            scope_validado: None,
        };
        let entradas = inventariar_root_anclado_con_limites(ancla, LimitesInventario::CONTRATO)
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        let digest = calcular_digest_raw_tree(&entradas)
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        let identidad = identidad_raiz
            .journal_identity()
            .ok_or(ErrorDuenioTransaccionAnclado::NoVerificable)?;

        Ok(Some(ArtifactFingerprint::new(identidad, digest)))
    }
}

#[cfg(all(test, any(target_os = "linux", windows)))]
mod tests {
    use super::*;
    use crate::transaccion::anclado::{abrir_directorio_absoluto, abrir_directorio_hijo_lectura};
    use crate::transaccion::lock::LockEscritura;
    use crate::transaccion::raiz::UbicacionRaiz;
    use crate::transaccion::snapshot::{EntradaSnapshot, calcular_digest_raw_tree};
    use std::ffi::OsStr;
    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;

    const PAYLOAD: &[u8] = b"root observer raw payload";
    const LOCK_BYTES: &[u8] = b"root observer same lock bytes";

    fn crear_fixture_root(root: &Path) {
        fs::create_dir(root).unwrap();
        fs::create_dir(root.join("empty")).unwrap();
        fs::write(root.join("payload.bin"), PAYLOAD).unwrap();
    }

    fn abrir_contexto(parent: &Path, root: bool) -> (UbicacionRaiz, PathBuf, LockEscritura) {
        fs::create_dir_all(parent).unwrap();
        let path_root = parent.join("bundle");
        if root {
            crear_fixture_root(&path_root);
        }
        let ubicacion = UbicacionRaiz::nueva(&path_root).unwrap();
        fs::write(ubicacion.lock().as_path(), LOCK_BYTES).unwrap();
        let lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
        (ubicacion, path_root, lock)
    }

    fn digest_fixture() -> crate::transaccion::journal::RawTreeDigest {
        calcular_digest_raw_tree(&[
            EntradaSnapshot::directorio("empty"),
            EntradaSnapshot::archivo("payload.bin", PAYLOAD),
        ])
        .unwrap()
    }

    fn fingerprint_desde_parent(parent: &std::fs::File, nombre: &OsStr) -> ArtifactFingerprint {
        let root = abrir_directorio_hijo_lectura(parent, nombre).unwrap();
        let identidad = metadata_directorio(&root)
            .unwrap()
            .journal_identity()
            .unwrap();
        ArtifactFingerprint::new(identidad, digest_fixture())
    }

    #[test]
    fn observar_root_ausente_devuelve_none_sin_crearlo() {
        let temp = tempdir().unwrap();
        let parent = temp.path().join("parent");
        let (_ubicacion, root, mut lock) = abrir_contexto(&parent, false);
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

        assert!(matches!(owner.observar_root_raw(), Ok(None)));
        assert!(!root.exists());
    }

    #[test]
    fn observar_root_devuelve_identidad_y_digest_raw_completos() {
        let temp = tempdir().unwrap();
        let parent = temp.path().join("parent");
        let (ubicacion, _root, mut lock) = abrir_contexto(&parent, true);
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let esperado =
            fingerprint_desde_parent(&owner.parent, ubicacion.ruta().file_name().unwrap());

        let observado = owner.observar_root_raw().unwrap().unwrap();

        assert!(observado == esperado);
    }

    #[test]
    fn contenido_igual_en_roots_distintos_conserva_ids_distintos() {
        let primera_temp = tempdir().unwrap();
        let segunda_temp = tempdir().unwrap();
        let primera_parent = primera_temp.path().join("parent");
        let segunda_parent = segunda_temp.path().join("parent");
        let (primera_ubicacion, _, mut primer_lock) = abrir_contexto(&primera_parent, true);
        let (segunda_ubicacion, _, mut segundo_lock) = abrir_contexto(&segunda_parent, true);
        let mut primer_owner = DuenioTransaccionAnclado::desde_lock(&mut primer_lock).unwrap();
        let mut segundo_owner = DuenioTransaccionAnclado::desde_lock(&mut segundo_lock).unwrap();

        let primero = primer_owner.observar_root_raw().unwrap().unwrap();
        let segundo = segundo_owner.observar_root_raw().unwrap().unwrap();
        let primero_esperado = fingerprint_desde_parent(
            &primer_owner.parent,
            primera_ubicacion.ruta().file_name().unwrap(),
        );
        let segundo_esperado = fingerprint_desde_parent(
            &segundo_owner.parent,
            segunda_ubicacion.ruta().file_name().unwrap(),
        );

        assert!(primero == primero_esperado);
        assert!(segundo == segundo_esperado);
        assert!(primero.identity() != segundo.identity());
        assert!(primero != segundo);
    }

    #[test]
    fn parent_movido_no_redirige_observacion_a_root_decoy() {
        let temp = tempdir().unwrap();
        let parent = temp.path().join("parent");
        let moved_parent = temp.path().join("parent-moved");
        let (ubicacion, _root, mut lock) = abrir_contexto(&parent, true);
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let esperado = owner.observar_root_raw().unwrap().unwrap();

        if let Err(error) = fs::rename(&parent, &moved_parent) {
            #[cfg(windows)]
            {
                assert!(matches!(
                    error.raw_os_error(),
                    Some(5) | Some(32) | Some(33)
                ));
                assert!(_root.is_dir());
                assert!(!moved_parent.exists());
                assert!(owner.observar_root_raw().unwrap().unwrap() == esperado);
                eprintln!(
                    "Windows impidió mover el parent retenido; el reemplazo por decoy no se ejercitó."
                );
                return;
            }
            #[cfg(not(windows))]
            panic!("no se pudo mover el parent de prueba: {error}");
        }

        fs::create_dir(&parent).unwrap();
        let decoy_root = parent.join("bundle");
        crear_fixture_root(&decoy_root);
        let decoy_lock = parent.join(ubicacion.lock().as_path().file_name().unwrap());
        fs::write(&decoy_lock, LOCK_BYTES).unwrap();
        assert_eq!(fs::read(&decoy_lock).unwrap(), LOCK_BYTES);

        let decoy_parent_handle = abrir_directorio_absoluto(&parent, true).unwrap();
        let decoy =
            fingerprint_desde_parent(&decoy_parent_handle, ubicacion.ruta().file_name().unwrap());
        let observado = owner.observar_root_raw().unwrap().unwrap();

        assert!(esperado.identity() != decoy.identity());
        assert!(observado == esperado);
        assert!(observado != decoy);
        assert_eq!(fs::read(decoy_root.join("payload.bin")).unwrap(), PAYLOAD);
        assert!(moved_parent.join("bundle").is_dir());
    }

    #[test]
    fn root_regular_file_se_rechaza_y_se_preserva() {
        let temp = tempdir().unwrap();
        let parent = temp.path().join("parent");
        let (_ubicacion, root, mut lock) = abrir_contexto(&parent, true);
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let original = temp.path().join("original-root");
        fs::rename(&root, &original).unwrap();
        fs::write(&root, b"root file canary").unwrap();

        let resultado = owner.observar_root_raw();

        assert!(matches!(
            resultado,
            Err(ErrorDuenioTransaccionAnclado::NoVerificable)
        ));
        assert_eq!(fs::read(&root).unwrap(), b"root file canary");
        assert_eq!(fs::read(original.join("payload.bin")).unwrap(), PAYLOAD);
    }

    #[cfg(unix)]
    #[test]
    fn root_symlink_se_rechaza_sin_tocar_el_target() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().unwrap();
        let parent = temp.path().join("parent");
        let (_ubicacion, root, mut lock) = abrir_contexto(&parent, true);
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let original = temp.path().join("original-root");
        let target = temp.path().join("target-root");
        fs::rename(&root, &original).unwrap();
        crear_fixture_root(&target);
        symlink(&target, &root).unwrap();

        let resultado = owner.observar_root_raw();

        assert!(matches!(
            resultado,
            Err(ErrorDuenioTransaccionAnclado::NoVerificable)
        ));
        assert!(
            fs::symlink_metadata(&root)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(target.join("payload.bin")).unwrap(), PAYLOAD);
        assert_eq!(fs::read(original.join("payload.bin")).unwrap(), PAYLOAD);
    }
}
