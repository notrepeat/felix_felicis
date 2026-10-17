//! Observes only the strict legacy Journal-present candidate after the auxiliary guard.

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
pub(super) enum ErrorObservacionLegacyPresente {
    OwnerUnverified,
    JournalUnverified,
    JournalAbsent,
    AuxiliaryUnverified,
    ProbeUnverified,
    JournalChanged,
    JournalIncompatible,
}

impl ErrorObservacionLegacyPresente {
    const fn code(self) -> &'static str {
        match self {
            Self::OwnerUnverified => "legacy_observation_owner_unverified",
            Self::JournalUnverified => "legacy_observation_journal_unverified",
            Self::JournalAbsent => "legacy_observation_journal_absent",
            Self::AuxiliaryUnverified => "legacy_observation_auxiliary_unverified",
            Self::ProbeUnverified => "legacy_observation_probe_unverified",
            Self::JournalChanged => "legacy_observation_journal_changed",
            Self::JournalIncompatible => "legacy_observation_journal_incompatible",
        }
    }
}

impl fmt::Debug for ErrorObservacionLegacyPresente {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl fmt::Display for ErrorObservacionLegacyPresente {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for ErrorObservacionLegacyPresente {}

impl DuenioTransaccionAnclado<'_> {
    /// Keeps the original strict Journal observation only for a v1 canonical Journal.
    pub(super) fn observar_journal_legacy_presente(
        &mut self,
    ) -> Result<ObservacionJournalPersistido, ErrorObservacionLegacyPresente> {
        self.observar_con_postcheck(|| {}, |owner| owner.parent_revalidado())
    }

    fn observar_con_postcheck<H, P>(
        &mut self,
        despues_de_lectura: H,
        postcheck: P,
    ) -> Result<ObservacionJournalPersistido, ErrorObservacionLegacyPresente>
    where
        H: FnOnce(),
        P: FnOnce(&Self) -> Result<File, ErrorJournalAnclado>,
    {
        let parent = self
            .parent_revalidado()
            .map_err(|_| ErrorObservacionLegacyPresente::OwnerUnverified)?;
        drop(parent);

        let lectura = self.leer_journal_legacy_presente(despues_de_lectura);
        let postcheck = postcheck(self);
        finalizar_observacion_legacy_presente(lectura, postcheck)
    }

    fn leer_journal_legacy_presente<H>(
        &mut self,
        despues_de_lectura: H,
    ) -> Result<ObservacionJournalPersistido, ErrorObservacionLegacyPresente>
    where
        H: FnOnce(),
    {
        let lectura = self
            .observar_journal_versionado()
            .map_err(|_| ErrorObservacionLegacyPresente::JournalUnverified);
        despues_de_lectura();
        let journal = lectura?.ok_or(ErrorObservacionLegacyPresente::JournalAbsent)?;

        if !matches!(journal.journal(), JournalPersistido::V1(_)) {
            return Err(ErrorObservacionLegacyPresente::JournalIncompatible);
        }

        self.exigir_auxiliares_legacy_ausentes()
            .map_err(|_| ErrorObservacionLegacyPresente::AuxiliaryUnverified)?;
        self.exigir_auxiliares_legacy_ausentes()
            .map_err(|_| ErrorObservacionLegacyPresente::AuxiliaryUnverified)?;
        self.confirmar_journal_final(&journal)?;
        Ok(journal)
    }

    fn confirmar_journal_final(
        &self,
        observacion_inicial: &ObservacionJournalPersistido,
    ) -> Result<(), ErrorObservacionLegacyPresente> {
        let parent = self
            .parent_revalidado()
            .map_err(|_| ErrorObservacionLegacyPresente::OwnerUnverified)?;
        let resultado = (|| {
            let ruta = self._lock.ubicacion().journal();
            let nombre = ruta
                .as_path()
                .file_name()
                .ok_or(ErrorObservacionLegacyPresente::ProbeUnverified)?
                .to_os_string();
            validar_componente(&nombre)
                .map_err(|_| ErrorObservacionLegacyPresente::ProbeUnverified)?;

            let visible =
                match abrir_entrada_relativa(&parent, &nombre, TipoEntradaAnclada::Archivo) {
                    Ok(visible) => visible,
                    Err(ErrorDirectorioAnclado::EntradaAusente) => {
                        return Err(ErrorObservacionLegacyPresente::JournalChanged);
                    }
                    Err(_) => return Err(ErrorObservacionLegacyPresente::ProbeUnverified),
                };
            if scope_filesystem(&visible)
                .map_err(|_| ErrorObservacionLegacyPresente::ProbeUnverified)?
                != self.scope_parent
            {
                return Err(ErrorObservacionLegacyPresente::ProbeUnverified);
            }
            let identidad: IdentidadArchivo = identidad_archivo(&visible)
                .map_err(|_| ErrorObservacionLegacyPresente::ProbeUnverified)?;
            if identidad != observacion_inicial.identidad().identidad_archivo() {
                return Err(ErrorObservacionLegacyPresente::JournalChanged);
            }
            Ok(())
        })();
        drop(parent);
        resultado
    }
}

fn finalizar_observacion_legacy_presente(
    lectura: Result<ObservacionJournalPersistido, ErrorObservacionLegacyPresente>,
    postcheck: Result<File, ErrorJournalAnclado>,
) -> Result<ObservacionJournalPersistido, ErrorObservacionLegacyPresente> {
    let parent = postcheck.map_err(|_| ErrorObservacionLegacyPresente::OwnerUnverified)?;
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
        let observado = owner.observar_journal_legacy_presente().unwrap();
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
    fn auxiliar_presente_rechaza_sin_leer_ni_alterar_su_payload() {
        let temp = tempfile::tempdir().unwrap();
        let fixture = fixture(temp.path(), "legacy-present-auxiliary");
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
            owner.observar_journal_legacy_presente(),
            Err(ErrorObservacionLegacyPresente::AuxiliaryUnverified)
        ));
        drop(owner);
        drop(lock);

        assert_eq!(fs::read(&auxiliar).unwrap(), bytes);
        assert_eq!(identidad(&auxiliar), id_auxiliar);
    }

    #[test]
    fn journal_v2_y_canonicamente_ausente_rechazan_sin_fallback() {
        let temp = tempfile::tempdir().unwrap();
        for caso in ["v2", "absent"] {
            let fixture = fixture(temp.path(), caso);
            let journal = journal_path(&fixture);
            let temporal = journal
                .parent()
                .unwrap()
                .join(fixture.ubicacion.nombre_journal_temporal());
            let esperado = if caso == "v2" {
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
                v2
            } else {
                fs::rename(&journal, &temporal).unwrap();
                fs::read(&temporal).unwrap()
            };
            let error_esperado = if caso == "v2" {
                ErrorObservacionLegacyPresente::JournalIncompatible
            } else {
                ErrorObservacionLegacyPresente::JournalAbsent
            };

            let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
            let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
            assert!(matches!(
                owner.observar_journal_legacy_presente(),
                Err(error) if error == error_esperado
            ));
            drop(owner);
            drop(lock);

            if caso == "v2" {
                assert_eq!(fs::read(&journal).unwrap(), esperado);
            } else {
                assert!(!journal.exists());
                assert_eq!(fs::read(&temporal).unwrap(), esperado);
            }
        }
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
            Err(ErrorObservacionLegacyPresente::JournalChanged)
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
    fn postcheck_fallido_domina_candidato_some_y_error_de_lectura() {
        let temp = tempfile::tempdir().unwrap();
        for caso in ["some", "read-error"] {
            let fixture = fixture(temp.path(), caso);
            let journal = journal_path(&fixture);
            let esperado = if caso == "read-error" {
                let bytes = b"not-a-canonical-journal";
                fs::write(&journal, bytes).unwrap();
                bytes.as_slice()
            } else {
                fixture.bytes_journal.as_slice()
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
                Err(ErrorObservacionLegacyPresente::OwnerUnverified)
            ));
            drop(owner);
            drop(lock);

            assert_eq!(fs::read(&journal).unwrap(), esperado);
        }
    }
}
