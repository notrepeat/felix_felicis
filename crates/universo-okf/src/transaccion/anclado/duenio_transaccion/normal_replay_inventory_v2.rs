//! Une replay normal v2 e inventario raw bajo un solo owner, solo como datos.

use std::fmt;
use std::fs::File;

use super::super::journal_temporal::ErrorJournalAnclado;
use super::DuenioTransaccionAnclado;
use super::comparar_root_normal_v2::RelacionRootNormalV2;
use super::familia_persistida::FaseFamiliaPersistida;
use super::observer::SiblingsRecoveryV2Raw;
use super::retained_v2_pair::NormalV2;
use crate::transaccion::journal::recovery::{ArtifactState, RecoveryInventory};
use crate::transaccion::journal::v2::JournalV2;
use crate::transaccion::journal::{ArtifactFingerprint, JournalState};

/// Error fijo para el coordinador privado de replay e inventario normal v2.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ErrorNormalRecoveryV2 {
    OwnerUnverified,
    ObservationUnverified,
}

impl ErrorNormalRecoveryV2 {
    const fn codigo(self) -> &'static str {
        match self {
            Self::OwnerUnverified => "normal_recovery_v2_owner_unverified",
            Self::ObservationUnverified => "normal_recovery_v2_observation_unverified",
        }
    }
}

impl fmt::Debug for ErrorNormalRecoveryV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl fmt::Display for ErrorNormalRecoveryV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorNormalRecoveryV2 {}

impl DuenioTransaccionAnclado<'_> {
    /// Combina replay cerrado y clasificación desde una única muestra RAW.
    pub(super) fn observar_inventario_normal_replay_v2(
        &mut self,
    ) -> Result<(JournalV2, RecoveryInventory, RelacionRootNormalV2), ErrorNormalRecoveryV2> {
        self.observar_inventario_normal_con_postcheck(|owner| owner.parent_revalidado())
    }

    #[cfg(test)]
    fn observar_inventario_normal_replay_v2_con_postcheck_prueba<P>(
        &mut self,
        postcheck: P,
    ) -> Result<(JournalV2, RecoveryInventory, RelacionRootNormalV2), ErrorNormalRecoveryV2>
    where
        P: FnOnce(&Self) -> Result<File, ErrorJournalAnclado>,
    {
        self.observar_inventario_normal_con_postcheck(postcheck)
    }

    fn observar_inventario_normal_con_postcheck<P>(
        &mut self,
        postcheck: P,
    ) -> Result<(JournalV2, RecoveryInventory, RelacionRootNormalV2), ErrorNormalRecoveryV2>
    where
        P: FnOnce(&Self) -> Result<File, ErrorJournalAnclado>,
    {
        let parent = self
            .parent_revalidado()
            .map_err(|_| ErrorNormalRecoveryV2::OwnerUnverified)?;
        drop(parent);

        let (resultado, normal) = match self.observar_normal_v2() {
            Ok(normal) => {
                let resultado = self.observar_normal_con_inventario(&normal);
                (resultado, Some(normal))
            }
            Err(_) => (Err(ErrorNormalRecoveryV2::ObservationUnverified), None),
        };

        let parent = postcheck(self).map_err(|_| ErrorNormalRecoveryV2::OwnerUnverified)?;
        drop(parent);
        let _normal_retenida_hasta_postcheck = normal;
        resultado
    }

    fn observar_normal_con_inventario(
        &mut self,
        normal: &NormalV2,
    ) -> Result<(JournalV2, RecoveryInventory, RelacionRootNormalV2), ErrorNormalRecoveryV2> {
        let fase = normal
            .fase()
            .map_err(|_| ErrorNormalRecoveryV2::ObservationUnverified)?;
        let (journal, contexto) = match fase {
            FaseFamiliaPersistida::NormalV2 { journal, contexto } => (journal, contexto),
            _ => return Err(ErrorNormalRecoveryV2::ObservationUnverified),
        };
        if !matches!(
            journal.state(),
            JournalState::Prepared | JournalState::BackupReady | JournalState::Installed
        ) {
            return Err(ErrorNormalRecoveryV2::ObservationUnverified);
        }

        let replay = contexto
            .calcular_new_esperado(journal)
            .map_err(|_| ErrorNormalRecoveryV2::ObservationUnverified)?;
        let esperado = ArtifactFingerprint::new(
            replay.journal().new_fingerprint().identity().clone(),
            replay.digest().clone(),
        );
        let raw: SiblingsRecoveryV2Raw = self
            .observar_siblings_recovery_v2_raw(journal)
            .map_err(|_| ErrorNormalRecoveryV2::ObservationUnverified)?;
        if raw.journal() != journal {
            return Err(ErrorNormalRecoveryV2::ObservationUnverified);
        }

        let relacion = match raw.root() {
            None => RelacionRootNormalV2::Ausente,
            Some(actual) if actual == &esperado => RelacionRootNormalV2::Coincide,
            Some(_) => RelacionRootNormalV2::Diferente,
        };
        let mut inventario = raw.clasificar_inventario_recovery_v2();
        inventario.root_new_validated =
            inventario.root == ArtifactState::New && relacion == RelacionRootNormalV2::Coincide;

        Ok((raw.journal().clone(), inventario, relacion))
    }
}

#[cfg(all(test, any(target_os = "linux", windows)))]
mod tests {
    use super::*;
    use crate::transaccion::anclado::duenio_transaccion::fixture_root_replay::{
        materializar, verificar_arbol,
    };
    use crate::transaccion::anclado::duenio_transaccion::normal_v2_test_support::{
        capturar_storage, crear_canary, preparar_normal_v2, verificar_canary, verificar_storage,
    };
    use crate::transaccion::journal::recovery::ArtifactState;
    use crate::transaccion::journal::v2::JournalV2;
    use crate::transaccion::journal::{ArtifactFingerprint, JournalState};
    use crate::transaccion::lock::LockEscritura;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn nominal_prepared_retorna_inventario_y_preserva_registros_y_canary() {
        let temp = tempdir().unwrap();
        let (fixture, digest) =
            preparar_normal_v2(&temp.path().join("parent"), JournalState::Prepared);
        assert!(digest.is_some());
        let storage = capturar_storage(&fixture);
        let (canary, identidad_canary) = crear_canary(&fixture);
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

        let (journal, inventario, relacion) = owner.observar_inventario_normal_replay_v2().unwrap();
        assert!(journal == JournalV2::parse(&fixture.bytes_journal).unwrap());
        assert!(journal.state() == JournalState::Prepared);
        assert!(inventario.root == ArtifactState::New);
        assert!(inventario.root_new_validated);
        assert_eq!(relacion, RelacionRootNormalV2::Coincide);

        let dominado = owner
            .observar_inventario_normal_replay_v2_con_postcheck_prueba(|_| {
                Err(ErrorJournalAnclado::ParentChanged)
            })
            .unwrap_err();
        assert_eq!(dominado, ErrorNormalRecoveryV2::OwnerUnverified);

        drop(owner);
        drop(lock);
        verificar_storage(&fixture, &storage);
        verificar_arbol(&fixture.root, &fixture.entradas);
        verificar_canary(&canary, identidad_canary);
    }

    #[test]
    fn backup_ready_e_installed_se_admiten_como_datos_normales() {
        for state in [JournalState::BackupReady, JournalState::Installed] {
            let temp = tempdir().unwrap();
            let (fixture, digest) = preparar_normal_v2(&temp.path().join("parent"), state);
            assert!(digest.is_some());
            let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
            let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

            let resultado = owner.observar_inventario_normal_replay_v2();
            assert!(resultado.is_ok());
            let (journal, inventario, relacion) = resultado.unwrap();
            assert!(journal.state() == state);
            assert!(inventario.root_new_validated);
            assert_eq!(relacion, RelacionRootNormalV2::Coincide);

            drop(owner);
            drop(lock);
            drop(fixture);
        }
    }

    #[test]
    fn committed_se_rechaza_y_postcheck_domina_el_error_del_cuerpo() {
        let temp = tempdir().unwrap();
        let (fixture, digest) =
            preparar_normal_v2(&temp.path().join("parent"), JournalState::Committed);
        assert!(digest.is_some());
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

        let error = owner.observar_inventario_normal_replay_v2().unwrap_err();
        assert_eq!(error, ErrorNormalRecoveryV2::ObservationUnverified);
        let dominado = owner
            .observar_inventario_normal_replay_v2_con_postcheck_prueba(|_| {
                Err(ErrorJournalAnclado::ParentChanged)
            })
            .unwrap_err();
        assert_eq!(dominado, ErrorNormalRecoveryV2::OwnerUnverified);

        drop(owner);
        drop(lock);
        drop(fixture);
    }

    #[test]
    fn root_ausente_se_clasifica_y_no_valida_replay() {
        let temp = tempdir().unwrap();
        let (fixture, digest) =
            preparar_normal_v2(&temp.path().join("parent"), JournalState::Prepared);
        assert!(digest.is_some());
        let retained = temp.path().join("root-retained");
        fs::rename(&fixture.root, &retained).unwrap();
        let storage = capturar_storage(&fixture);
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

        let (journal, inventario, relacion) = owner.observar_inventario_normal_replay_v2().unwrap();

        drop(owner);
        drop(lock);
        assert!(journal.state() == JournalState::Prepared);
        assert!(inventario.root == ArtifactState::Absent);
        assert!(!inventario.root_new_validated);
        assert_eq!(relacion, RelacionRootNormalV2::Ausente);
        assert!(!fixture.root.exists());
        verificar_arbol(&retained, &fixture.entradas);
        verificar_storage(&fixture, &storage);
    }

    #[test]
    fn contenido_igual_con_identidad_nativa_distinta_no_valida_root() {
        let temp = tempdir().unwrap();
        let (fixture, digest) =
            preparar_normal_v2(&temp.path().join("parent"), JournalState::Prepared);
        let digest = digest.unwrap();
        let retained = temp.path().join("root-original-retained");
        fs::rename(&fixture.root, &retained).unwrap();
        fs::create_dir(&fixture.root).unwrap();
        materializar(&fixture.root, &fixture.entradas);
        let storage = capturar_storage(&fixture);
        let (canary, identidad_canary) = crear_canary(&fixture);
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let observado = owner.observar_root_raw().unwrap().unwrap();
        let misma_digest = ArtifactFingerprint::new(observado.identity().clone(), digest);
        assert!(observado == misma_digest);
        assert!(observado.identity() != fixture.esperado.identity());

        let (journal, inventario, relacion) = owner.observar_inventario_normal_replay_v2().unwrap();

        drop(owner);
        drop(lock);
        assert!(journal.state() == JournalState::Prepared);
        assert!(inventario.root == ArtifactState::Unknown);
        assert!(!inventario.root_new_validated);
        assert_eq!(relacion, RelacionRootNormalV2::Diferente);
        verificar_arbol(&fixture.root, &fixture.entradas);
        verificar_arbol(&retained, &fixture.entradas);
        verificar_storage(&fixture, &storage);
        verificar_canary(&canary, identidad_canary);
    }

    #[test]
    fn identidad_nativa_igual_con_digest_distinto_no_valida_root() {
        let temp = tempdir().unwrap();
        let (fixture, digest) =
            preparar_normal_v2(&temp.path().join("parent"), JournalState::Prepared);
        let digest = digest.unwrap();
        let entrada = fixture
            .entradas
            .iter()
            .find(|entrada| entrada.bytes().is_some())
            .unwrap();
        let path = fixture.root.join(entrada.ruta());
        let bytes_originales = fs::read(&path).unwrap();
        let mut bytes_modificados = bytes_originales.clone();
        bytes_modificados.push(b'!');
        fs::write(&path, &bytes_modificados).unwrap();
        let storage = capturar_storage(&fixture);
        let (canary, identidad_canary) = crear_canary(&fixture);
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let observado = owner.observar_root_raw().unwrap().unwrap();
        let misma_identidad = ArtifactFingerprint::new(fixture.esperado.identity().clone(), digest);
        assert!(observado.identity() == misma_identidad.identity());
        assert!(observado != misma_identidad);

        let (journal, inventario, relacion) = owner.observar_inventario_normal_replay_v2().unwrap();

        drop(owner);
        drop(lock);
        assert!(journal.state() == JournalState::Prepared);
        assert!(inventario.root == ArtifactState::Unknown);
        assert!(!inventario.root_new_validated);
        assert_eq!(relacion, RelacionRootNormalV2::Diferente);
        assert_eq!(fs::read(path).unwrap(), bytes_modificados);
        verificar_storage(&fixture, &storage);
        verificar_canary(&canary, identidad_canary);
    }
}
