//! Observes a strict legacy Journal or empty state after the auxiliary guard.

use std::fmt;
use std::fs::File;

use crate::transaccion::journal::selector::JournalPersistido;

use super::super::abrir::abrir_entrada_relativa;
use super::super::journal_temporal::{
    ErrorJournalAnclado, versionado::ObservacionJournalPersistido,
};
use super::super::{
    ErrorDirectorioAnclado, IdentidadArchivo, TipoEntradaAnclada, identidad_archivo,
    scope_filesystem, validar_componente,
};
use super::DuenioTransaccionAnclado;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ErrorObservacionLegacy {
    OwnerUnverified,
    JournalUnverified,
    AuxiliaryUnverified,
    ProbeUnverified,
    JournalChanged,
    JournalIncompatible,
}

impl ErrorObservacionLegacy {
    const fn code(self) -> &'static str {
        match self {
            Self::OwnerUnverified => "legacy_observation_owner_unverified",
            Self::JournalUnverified => "legacy_observation_journal_unverified",
            Self::AuxiliaryUnverified => "legacy_observation_auxiliary_unverified",
            Self::ProbeUnverified => "legacy_observation_probe_unverified",
            Self::JournalChanged => "legacy_observation_journal_changed",
            Self::JournalIncompatible => "legacy_observation_journal_incompatible",
        }
    }
}

impl fmt::Debug for ErrorObservacionLegacy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl fmt::Display for ErrorObservacionLegacy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for ErrorObservacionLegacy {}

impl DuenioTransaccionAnclado<'_> {
    /// Keeps the strict legacy read and reports absence only after a final canonical probe.
    pub(super) fn observar_legacy_o_vacio(
        &mut self,
    ) -> Result<Option<ObservacionJournalPersistido>, ErrorObservacionLegacy> {
        self.observar_con_postcheck(|| {}, |owner| owner.parent_revalidado())
    }

    fn observar_con_postcheck<H, P>(
        &mut self,
        despues_de_lectura: H,
        postcheck: P,
    ) -> Result<Option<ObservacionJournalPersistido>, ErrorObservacionLegacy>
    where
        H: FnOnce(),
        P: FnOnce(&Self) -> Result<File, ErrorJournalAnclado>,
    {
        let parent = self
            .parent_revalidado()
            .map_err(|_| ErrorObservacionLegacy::OwnerUnverified)?;
        drop(parent);

        let lectura = self.leer_legacy_o_vacio(despues_de_lectura);
        let postcheck = postcheck(self);
        finalizar_observacion_legacy(lectura, postcheck)
    }

    fn leer_legacy_o_vacio<H>(
        &mut self,
        despues_de_lectura: H,
    ) -> Result<Option<ObservacionJournalPersistido>, ErrorObservacionLegacy>
    where
        H: FnOnce(),
    {
        let lectura = self
            .observar_journal_versionado()
            .map_err(|_| ErrorObservacionLegacy::JournalUnverified);
        despues_de_lectura();
        let journal = lectura?;

        if journal
            .as_ref()
            .is_some_and(|observacion| !matches!(observacion.journal(), JournalPersistido::V1(_)))
        {
            return Err(ErrorObservacionLegacy::JournalIncompatible);
        }

        self.exigir_auxiliares_legacy_ausentes()
            .map_err(|_| ErrorObservacionLegacy::AuxiliaryUnverified)?;
        self.exigir_auxiliares_legacy_ausentes()
            .map_err(|_| ErrorObservacionLegacy::AuxiliaryUnverified)?;
        self.confirmar_journal_final(journal.as_ref())?;
        Ok(journal)
    }

    fn confirmar_journal_final(
        &self,
        observacion_inicial: Option<&ObservacionJournalPersistido>,
    ) -> Result<(), ErrorObservacionLegacy> {
        let parent = self
            .parent_revalidado()
            .map_err(|_| ErrorObservacionLegacy::OwnerUnverified)?;
        let resultado = (|| {
            let ruta = self._lock.ubicacion().journal();
            let nombre = ruta
                .as_path()
                .file_name()
                .ok_or(ErrorObservacionLegacy::ProbeUnverified)?
                .to_os_string();
            validar_componente(&nombre).map_err(|_| ErrorObservacionLegacy::ProbeUnverified)?;

            let visible =
                match abrir_entrada_relativa(&parent, &nombre, TipoEntradaAnclada::Archivo) {
                    Ok(visible) => visible,
                    Err(ErrorDirectorioAnclado::EntradaAusente)
                        if observacion_inicial.is_none() =>
                    {
                        return Ok(());
                    }
                    Err(ErrorDirectorioAnclado::EntradaAusente) => {
                        return Err(ErrorObservacionLegacy::JournalChanged);
                    }
                    Err(_) => return Err(ErrorObservacionLegacy::ProbeUnverified),
                };
            if scope_filesystem(&visible).map_err(|_| ErrorObservacionLegacy::ProbeUnverified)?
                != self.scope_parent
            {
                return Err(ErrorObservacionLegacy::ProbeUnverified);
            }

            let Some(observacion_inicial) = observacion_inicial else {
                return Err(ErrorObservacionLegacy::JournalChanged);
            };
            let identidad: IdentidadArchivo =
                identidad_archivo(&visible).map_err(|_| ErrorObservacionLegacy::ProbeUnverified)?;
            if identidad != observacion_inicial.identidad().identidad_archivo() {
                return Err(ErrorObservacionLegacy::JournalChanged);
            }
            Ok(())
        })();
        drop(parent);
        resultado
    }
}

fn finalizar_observacion_legacy(
    lectura: Result<Option<ObservacionJournalPersistido>, ErrorObservacionLegacy>,
    postcheck: Result<File, ErrorJournalAnclado>,
) -> Result<Option<ObservacionJournalPersistido>, ErrorObservacionLegacy> {
    let parent = postcheck.map_err(|_| ErrorObservacionLegacy::OwnerUnverified)?;
    drop(parent);
    lectura
}

#[cfg(all(test, any(target_os = "linux", windows)))]
mod tests {
    use std::fs::{self, File};
    use std::path::{Path, PathBuf};

    use crate::transaccion::anclado::duenio_transaccion::fixture_root_replay;
    use crate::transaccion::anclado::{IdentidadArchivo, identidad_archivo};
    use crate::transaccion::journal::{Journal, v2::JournalV2};
    use crate::transaccion::lock::LockEscritura;

    use super::*;

    fn fixture(parent: &Path, name: &str) -> fixture_root_replay::FixturePublicada {
        let fixture = fixture_root_replay::preparar(&parent.join(name));
        fs::remove_file(fixture_root_replay::contexto_path(&fixture.ubicacion)).unwrap();
        fixture
    }

    fn journal_path(fixture: &fixture_root_replay::FixturePublicada) -> PathBuf {
        fixture.ubicacion.journal().as_path().to_path_buf()
    }

    fn identidad(path: &Path) -> IdentidadArchivo {
        identidad_archivo(&File::open(path).unwrap()).unwrap()
    }

    #[test]
    fn journal_v1_presente_ignora_y_preserva_journal_tmp_como_directorio() {
        let temp = tempfile::tempdir().unwrap();
        let fixture = fixture(temp.path(), "legacy-present-tmp-directory");
        let journal = journal_path(&fixture);
        let temporal = journal
            .parent()
            .unwrap()
            .join(fixture.ubicacion.nombre_journal_temporal());
        fs::create_dir(&temporal).unwrap();
        let canario = temporal.join("canary");
        fs::write(&canario, b"journal temporary canary").unwrap();
        let id_canario = identidad(&canario);

        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let observado = owner
            .observar_legacy_o_vacio()
            .unwrap()
            .expect("Journal v1 expected");
        assert!(matches!(observado.journal(), JournalPersistido::V1(_)));
        assert_eq!(
            observado.identidad().identidad_archivo(),
            identidad(&journal)
        );
        drop(observado);
        drop(owner);
        drop(lock);

        assert_eq!(fs::read(&journal).unwrap(), fixture.bytes_journal);
        assert_eq!(fs::read(&canario).unwrap(), b"journal temporary canary");
        assert_eq!(identidad(&canario), id_canario);
    }

    #[test]
    fn auxiliar_presente_rechaza_con_journal_presente_o_ausente_y_preserva_payload() {
        let temp = tempfile::tempdir().unwrap();
        for caso in ["some", "none"] {
            let fixture = fixture(temp.path(), caso);
            let journal = journal_path(&fixture);
            if caso == "none" {
                fs::remove_file(&journal).unwrap();
            }
            let basename = fixture.ubicacion.ruta().file_name().unwrap();
            let nombre =
                crate::transaccion::anclado::contexto_admision::nombre_canonico(basename).unwrap();
            let auxiliar = fixture.ubicacion.ruta().parent().unwrap().join(nombre);
            let bytes = b"opaque auxiliary payload";
            fs::write(&auxiliar, bytes).unwrap();
            let id_auxiliar = identidad(&auxiliar);

            let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
            let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
            assert!(matches!(
                owner.observar_legacy_o_vacio(),
                Err(ErrorObservacionLegacy::AuxiliaryUnverified)
            ));
            drop(owner);
            drop(lock);

            assert_eq!(fs::read(&auxiliar).unwrap(), bytes);
            assert_eq!(identidad(&auxiliar), id_auxiliar);
            if caso == "some" {
                assert_eq!(fs::read(&journal).unwrap(), fixture.bytes_journal);
            } else {
                assert!(!journal.exists());
            }
        }
    }

    #[test]
    fn journal_v2_rechaza_y_ausencia_estricta_devuelve_none() {
        let temp = tempfile::tempdir().unwrap();
        for caso in ["v2", "empty"] {
            let fixture = fixture(temp.path(), caso);
            let journal = journal_path(&fixture);
            let temporal = journal
                .parent()
                .unwrap()
                .join(fixture.ubicacion.nombre_journal_temporal());
            let bytes_esperados = if caso == "v2" {
                let v1 = Journal::parse(&fixture.bytes_journal).unwrap();
                let v2 = JournalV2::new(
                    v1.transaction_id(),
                    v1.parent().clone(),
                    v1.old().clone(),
                    v1.new_fingerprint().clone(),
                )
                .unwrap()
                .encode()
                .unwrap();
                fs::write(&journal, &v2).unwrap();
                Some(v2)
            } else {
                fs::remove_file(&journal).unwrap();
                None
            };

            let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
            let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
            let resultado = owner.observar_legacy_o_vacio();
            if caso == "v2" {
                assert!(matches!(
                    resultado,
                    Err(ErrorObservacionLegacy::JournalIncompatible)
                ));
            } else {
                assert!(matches!(resultado, Ok(None)));
            }
            drop(owner);
            drop(lock);

            if let Some(bytes) = bytes_esperados {
                assert_eq!(fs::read(&journal).unwrap(), bytes);
            } else {
                assert!(!journal.exists());
                assert!(!temporal.exists());
            }
        }
    }

    #[test]
    fn journal_tmp_solo_no_activa_fallback_y_preserva_bytes_e_identidad() {
        let temp = tempfile::tempdir().unwrap();
        let fixture = fixture(temp.path(), "legacy-empty-temporary-only");
        let journal = journal_path(&fixture);
        fs::remove_file(&journal).unwrap();
        let temporal = journal
            .parent()
            .unwrap()
            .join(fixture.ubicacion.nombre_journal_temporal());
        let bytes = b"opaque journal temporary";
        fs::write(&temporal, bytes).unwrap();
        let id_temporal = identidad(&temporal);

        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        assert!(matches!(owner.observar_legacy_o_vacio(), Ok(None)));
        drop(owner);
        drop(lock);

        assert!(!journal.exists());
        assert_eq!(fs::read(&temporal).unwrap(), bytes);
        assert_eq!(identidad(&temporal), id_temporal);
    }

    #[test]
    fn reemplazo_con_mismos_bytes_e_identidad_distinta_rechaza_y_preserva_ambos() {
        let temp = tempfile::tempdir().unwrap();
        let fixture = fixture(temp.path(), "legacy-present-replacement");
        let journal = journal_path(&fixture);
        let retenido = journal.parent().unwrap().join("retained-original-journal");
        let bytes = fixture.bytes_journal.clone();

        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let resultado = owner.observar_con_postcheck(
            || {
                fs::rename(&journal, &retenido).unwrap();
                fs::write(&journal, &bytes).unwrap();
            },
            |owner| owner.parent_revalidado(),
        );
        assert!(matches!(
            resultado,
            Err(ErrorObservacionLegacy::JournalChanged)
        ));
        drop(owner);
        drop(lock);

        let id_retenido = identidad(&retenido);
        let id_visible = identidad(&journal);
        assert_ne!(id_retenido, id_visible);
        assert_eq!(fs::read(&retenido).unwrap(), fixture.bytes_journal);
        assert_eq!(fs::read(&journal).unwrap(), fixture.bytes_journal);
    }

    #[test]
    fn postcheck_fallido_domina_candidato_some_none_y_error_de_lectura() {
        let temp = tempfile::tempdir().unwrap();
        for caso in ["some", "none", "read-error"] {
            let fixture = fixture(temp.path(), caso);
            let journal = journal_path(&fixture);
            let esperado = match caso {
                "none" => {
                    fs::remove_file(&journal).unwrap();
                    None
                }
                "read-error" => {
                    let bytes = b"not-a-canonical-journal".to_vec();
                    fs::write(&journal, &bytes).unwrap();
                    Some(bytes)
                }
                _ => Some(fixture.bytes_journal.clone()),
            };
            let mut postcheck_ejecutado = false;

            let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
            let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
            let resultado = owner.observar_con_postcheck(
                || {},
                |_| {
                    postcheck_ejecutado = true;
                    Err(ErrorJournalAnclado::ParentChanged)
                },
            );
            assert!(postcheck_ejecutado);
            assert!(matches!(
                resultado,
                Err(ErrorObservacionLegacy::OwnerUnverified)
            ));
            drop(owner);
            drop(lock);

            if let Some(bytes) = esperado {
                assert_eq!(fs::read(&journal).unwrap(), bytes);
            } else {
                assert!(!journal.exists());
            }
        }
    }

    #[test]
    fn journal_que_reaparece_despues_de_ausencia_falla_y_preserva_bytes_e_identidad() {
        let temp = tempfile::tempdir().unwrap();
        let fixture = fixture(temp.path(), "legacy-empty-reappeared");
        let journal = journal_path(&fixture);
        fs::remove_file(&journal).unwrap();
        let bytes = fixture.bytes_journal.clone();
        let mut id_reaparecida = None;

        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let resultado = owner.observar_con_postcheck(
            || {
                fs::write(&journal, &bytes).unwrap();
                id_reaparecida = Some(identidad(&journal));
            },
            |owner| owner.parent_revalidado(),
        );
        assert!(matches!(
            resultado,
            Err(ErrorObservacionLegacy::JournalChanged)
        ));
        drop(owner);
        drop(lock);

        assert_eq!(fs::read(&journal).unwrap(), bytes);
        assert_eq!(identidad(&journal), id_reaparecida.unwrap());
    }

    #[test]
    fn tipo_incompatible_que_aparece_despues_de_ausencia_falla_cerrado() {
        let temp = tempfile::tempdir().unwrap();
        let fixture = fixture(temp.path(), "legacy-empty-directory-reappeared");
        let journal = journal_path(&fixture);
        fs::remove_file(&journal).unwrap();
        let canary = journal.join("canary");
        let bytes = b"journal directory canary";
        let mut id_canary = None;

        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let resultado = owner.observar_con_postcheck(
            || {
                fs::create_dir(&journal).unwrap();
                fs::write(&canary, bytes).unwrap();
                id_canary = Some(identidad(&canary));
            },
            |owner| owner.parent_revalidado(),
        );
        assert!(matches!(
            resultado,
            Err(ErrorObservacionLegacy::ProbeUnverified)
        ));
        drop(owner);
        drop(lock);

        assert!(journal.is_dir());
        assert_eq!(fs::read(&canary).unwrap(), bytes);
        assert_eq!(identidad(&canary), id_canary.unwrap());
    }
}
