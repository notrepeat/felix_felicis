//! Guards legacy family observation by requiring exact absence of Context and Intent auxiliaries.
//!
//! Journal canonical and temporary siblings are deliberately outside this probe.
//! The result is a sequential absence check, not a snapshot or action authority.

use super::observar_siblings_nuevo_flujo::{PresenciaSiblingNuevoFlujo, observar_sibling};
use super::{DuenioTransaccionAnclado, ErrorDuenioTransaccionAnclado};

impl DuenioTransaccionAnclado<'_> {
    /// Requires the four reserved Context/Intent siblings to be absent.
    pub(super) fn exigir_auxiliares_legacy_ausentes(
        &mut self,
    ) -> Result<(), ErrorDuenioTransaccionAnclado> {
        let parent = self
            .parent_revalidado()
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        let prueba = (|| {
            let ubicacion = self._lock.ubicacion();
            let basename = ubicacion
                .ruta()
                .file_name()
                .ok_or(ErrorDuenioTransaccionAnclado::NoVerificable)?;
            let nombre_contexto = super::super::contexto_admision::nombre_canonico(basename)
                .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
            let nombre_contexto_temporal =
                super::super::contexto_admision::nombre_temporal_sibling(&nombre_contexto)
                    .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
            let nombre_intent_retiro =
                super::observar_siblings_nuevo_flujo::nombre_intent_retiro(basename)?;
            let nombre_intent_retiro_temporal =
                super::super::contexto_admision::nombre_temporal_sibling(&nombre_intent_retiro)
                    .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;

            for nombre in [
                &nombre_contexto,
                &nombre_contexto_temporal,
                &nombre_intent_retiro,
                &nombre_intent_retiro_temporal,
            ] {
                if observar_sibling(&parent, nombre, self.scope_parent)?
                    != PresenciaSiblingNuevoFlujo::Ausente
                {
                    return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
                }
            }
            Ok(())
        })();
        drop(parent);

        self.parent_revalidado()
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)
            .map(drop)?;
        prueba
    }
}

#[cfg(all(test, any(target_os = "linux", windows)))]
mod tests {
    use super::*;
    use crate::transaccion::anclado::contexto_admision::{
        nombre_canonico as nombre_contexto_canonico,
        nombre_temporal_sibling as nombre_temporal_contexto,
    };
    use crate::transaccion::anclado::{IdentidadArchivo, identidad_archivo};
    use crate::transaccion::lock::LockEscritura;
    use crate::transaccion::raiz::UbicacionRaiz;
    use std::fs::{self, File};
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    struct Fixture {
        _temp: TempDir,
        parent: PathBuf,
        ubicacion: UbicacionRaiz,
    }

    fn fixture() -> Fixture {
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join("parent");
        fs::create_dir(&parent).unwrap();
        let root = parent.join("bundle");
        let ubicacion = UbicacionRaiz::nueva(&root).unwrap();
        Fixture {
            _temp: temp,
            parent,
            ubicacion,
        }
    }

    fn auxiliares(ubicacion: &UbicacionRaiz) -> [PathBuf; 4] {
        let parent = ubicacion.ruta().parent().unwrap();
        let basename = ubicacion.ruta().file_name().unwrap();
        let contexto = nombre_contexto_canonico(basename).unwrap();
        let contexto_temporal = nombre_temporal_contexto(&contexto).unwrap();
        let intent =
            super::super::observar_siblings_nuevo_flujo::nombre_intent_retiro(basename).unwrap();
        let intent_temporal = nombre_temporal_contexto(&intent).unwrap();
        [
            parent.join(contexto),
            parent.join(contexto_temporal),
            parent.join(intent),
            parent.join(intent_temporal),
        ]
    }

    fn identidad(path: &Path) -> IdentidadArchivo {
        identidad_archivo(&File::open(path).unwrap()).unwrap()
    }

    #[test]
    fn auxiliares_ausentes_ignoran_journal_canonico_y_temporal_de_cualquier_tipo() {
        for journal_temporal_es_directorio in [false, true] {
            let fixture = fixture();
            let journal = fixture.ubicacion.journal().as_path().to_path_buf();
            let journal_temporal = fixture
                .parent
                .join(fixture.ubicacion.nombre_journal_temporal());
            let canario_journal = journal.join("canary");
            let canario_temporal = journal_temporal.join("canary");

            if journal_temporal_es_directorio {
                fs::create_dir(&journal).unwrap();
                fs::write(&canario_journal, b"journal directory canary").unwrap();
                fs::write(&journal_temporal, b"opaque journal temporary").unwrap();
            } else {
                fs::write(&journal, b"opaque journal canonical").unwrap();
                fs::create_dir(&journal_temporal).unwrap();
                fs::write(&canario_temporal, b"journal temporary directory canary").unwrap();
            }
            let identidad_journal = if journal.is_file() {
                Some(identidad(&journal))
            } else {
                None
            };
            let identidad_temporal = if journal_temporal.is_file() {
                Some(identidad(&journal_temporal))
            } else {
                None
            };
            let id_canario_journal = if journal.is_dir() {
                Some(identidad(&canario_journal))
            } else {
                None
            };
            let id_canario_temporal = if journal_temporal.is_dir() {
                Some(identidad(&canario_temporal))
            } else {
                None
            };

            {
                let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
                let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
                assert_eq!(owner.exigir_auxiliares_legacy_ausentes(), Ok(()));
            }

            if let Some(esperada) = identidad_journal {
                assert_eq!(fs::read(&journal).unwrap(), b"opaque journal canonical");
                assert_eq!(identidad(&journal), esperada);
            } else {
                assert!(journal.is_dir());
                assert_eq!(
                    fs::read(&canario_journal).unwrap(),
                    b"journal directory canary"
                );
                assert_eq!(identidad(&canario_journal), id_canario_journal.unwrap());
            }
            if let Some(esperada) = identidad_temporal {
                assert_eq!(
                    fs::read(&journal_temporal).unwrap(),
                    b"opaque journal temporary"
                );
                assert_eq!(identidad(&journal_temporal), esperada);
            } else {
                assert!(journal_temporal.is_dir());
                assert_eq!(
                    fs::read(&canario_temporal).unwrap(),
                    b"journal temporary directory canary"
                );
                assert_eq!(identidad(&canario_temporal), id_canario_temporal.unwrap());
            }
        }
    }

    #[test]
    fn cada_auxiliar_presente_falla_cerrado_y_preserva_bytes_identidad_y_canario() {
        let payloads: [&[u8]; 4] = [b"opaque context", b"", b"opaque intent", b""];
        for (indice, payload) in payloads.into_iter().enumerate() {
            let fixture = fixture();
            let paths = auxiliares(&fixture.ubicacion);
            let presente = &paths[indice];
            let canario = fixture.parent.join("unrelated-canary");
            fs::write(presente, payload).unwrap();
            fs::write(&canario, b"unrelated sibling canary").unwrap();
            let bytes_esperados = payload.to_vec();
            let id_presente = identidad(presente);
            let id_canario = identidad(&canario);

            {
                let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
                let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
                assert!(matches!(
                    owner.exigir_auxiliares_legacy_ausentes(),
                    Err(ErrorDuenioTransaccionAnclado::NoVerificable)
                ));
            }

            assert_eq!(fs::read(presente).unwrap(), bytes_esperados);
            assert_eq!(identidad(presente), id_presente);
            assert_eq!(fs::read(&canario).unwrap(), b"unrelated sibling canary");
            assert_eq!(identidad(&canario), id_canario);
        }
    }

    #[test]
    fn cada_auxiliar_directorio_falla_cerrado_y_preserva_canario() {
        for indice in 0..4 {
            let fixture = fixture();
            let paths = auxiliares(&fixture.ubicacion);
            let presente = &paths[indice];
            fs::create_dir(presente).unwrap();
            let canario = presente.join("canary");
            fs::write(&canario, b"directory canary").unwrap();
            let id_canario = identidad(&canario);

            {
                let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
                let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
                assert!(matches!(
                    owner.exigir_auxiliares_legacy_ausentes(),
                    Err(ErrorDuenioTransaccionAnclado::NoVerificable)
                ));
            }

            assert!(presente.is_dir());
            assert_eq!(fs::read(&canario).unwrap(), b"directory canary");
            assert_eq!(identidad(&canario), id_canario);
        }
    }

    #[cfg(unix)]
    #[test]
    fn enlace_auxiliar_falla_cerrado_y_preserva_link_y_destino() {
        use std::os::unix::fs::symlink;

        let fixture = fixture();
        let paths = auxiliares(&fixture.ubicacion);
        let destino = fixture.parent.join("symlink-target-canary");
        let enlace = &paths[0];
        fs::write(&destino, b"symlink target canary").unwrap();
        let id_destino = identidad(&destino);
        symlink(&destino, enlace).unwrap();

        {
            let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
            let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
            assert!(matches!(
                owner.exigir_auxiliares_legacy_ausentes(),
                Err(ErrorDuenioTransaccionAnclado::NoVerificable)
            ));
        }

        assert!(
            fs::symlink_metadata(enlace)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(&destino).unwrap(), b"symlink target canary");
        assert_eq!(identidad(&destino), id_destino);
    }
}
