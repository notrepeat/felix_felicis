//! Compara root raw con replay v2 bajo el mismo owner, sin emitir autoridad.

use std::fmt;
use std::fs::File;

use super::super::journal_temporal::ErrorJournalAnclado;
use super::DuenioTransaccionAnclado;
use super::familia_persistida::FaseFamiliaPersistida;
use super::retained_v2_pair::NormalV2;
use crate::transaccion::journal::ArtifactFingerprint;

/// Relación de datos entre la expectativa replay v2 y el root raw observado.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RelacionRootNormalV2 {
    Coincide,
    Diferente,
    Ausente,
}

impl fmt::Debug for RelacionRootNormalV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Coincide => "Coincide",
            Self::Diferente => "Diferente",
            Self::Ausente => "Ausente",
        })
    }
}

/// Fallo fijo de owner o de los datos necesarios para la comparación.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ErrorComparacionRootNormalV2 {
    OwnerUnverified,
    ReplayUnverified,
}

impl ErrorComparacionRootNormalV2 {
    pub(super) const fn codigo(self) -> &'static str {
        match self {
            Self::OwnerUnverified => "root_normal_v2_owner_unverified",
            Self::ReplayUnverified => "root_normal_v2_replay_unverified",
        }
    }
}

impl fmt::Debug for ErrorComparacionRootNormalV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl fmt::Display for ErrorComparacionRootNormalV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorComparacionRootNormalV2 {}

impl DuenioTransaccionAnclado<'_> {
    /// Observa normal v2 con este owner y compara su replay cerrado con root raw.
    pub(super) fn comparar_root_normal_v2(
        &mut self,
    ) -> Result<RelacionRootNormalV2, ErrorComparacionRootNormalV2> {
        self.comparar_con_postcheck(|owner| owner.parent_revalidado())
    }

    #[cfg(test)]
    fn comparar_root_normal_v2_con_postcheck_prueba<P>(
        &mut self,
        postcheck: P,
    ) -> Result<RelacionRootNormalV2, ErrorComparacionRootNormalV2>
    where
        P: FnOnce(&Self) -> Result<File, ErrorJournalAnclado>,
    {
        self.comparar_con_postcheck(postcheck)
    }

    fn comparar_con_postcheck<P>(
        &mut self,
        postcheck: P,
    ) -> Result<RelacionRootNormalV2, ErrorComparacionRootNormalV2>
    where
        P: FnOnce(&Self) -> Result<File, ErrorJournalAnclado>,
    {
        let parent = self
            .parent_revalidado()
            .map_err(|_| ErrorComparacionRootNormalV2::OwnerUnverified)?;
        drop(parent);

        let (resultado, normal) = match self.observar_normal_v2() {
            Ok(normal) => {
                let resultado = self.comparar_normal_con_root(&normal);
                (resultado, Some(normal))
            }
            Err(_) => (Err(ErrorComparacionRootNormalV2::ReplayUnverified), None),
        };

        let parent = postcheck(self).map_err(|_| ErrorComparacionRootNormalV2::OwnerUnverified)?;
        drop(parent);
        let _normal_retenida_hasta_postcheck = normal;
        resultado
    }

    fn comparar_normal_con_root(
        &mut self,
        normal: &NormalV2,
    ) -> Result<RelacionRootNormalV2, ErrorComparacionRootNormalV2> {
        let esperado = match normal
            .fase()
            .map_err(|_| ErrorComparacionRootNormalV2::ReplayUnverified)?
        {
            FaseFamiliaPersistida::NormalV2 { journal, contexto } => {
                let calculado = contexto
                    .calcular_new_esperado(journal)
                    .map_err(|_| ErrorComparacionRootNormalV2::ReplayUnverified)?;
                ArtifactFingerprint::new(
                    calculado.journal().new_fingerprint().identity().clone(),
                    calculado.digest().clone(),
                )
            }
            _ => return Err(ErrorComparacionRootNormalV2::ReplayUnverified),
        };

        let observado = self
            .observar_root_raw()
            .map_err(|_| ErrorComparacionRootNormalV2::ReplayUnverified)?;
        Ok(match observado.as_ref() {
            None => RelacionRootNormalV2::Ausente,
            Some(actual) if actual == &esperado => RelacionRootNormalV2::Coincide,
            Some(_) => RelacionRootNormalV2::Diferente,
        })
    }
}

#[cfg(all(test, any(target_os = "linux", windows)))]
mod tests {
    use super::*;
    use crate::transaccion::anclado::duenio_transaccion::fixture_root_replay::{
        materializar, verificar_arbol,
    };
    use crate::transaccion::anclado::duenio_transaccion::normal_v2_test_support::{
        capturar_storage, crear_canary, preparar_normal_v2_con_tags, verificar_canary,
        verificar_storage,
    };
    use crate::transaccion::journal::{ArtifactFingerprint, JournalState};
    use crate::transaccion::lock::LockEscritura;
    use crate::transaccion::replay::semantica_archivada::{POLICY_ID_V1, VERSION_MOTOR_V1};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn nominal_coincide_y_postcheck_domina_el_resultado_exitoso() {
        let temp = tempdir().unwrap();
        let (fixture, digest) = preparar_normal_v2_con_tags(
            &temp.path().join("parent"),
            JournalState::Prepared,
            VERSION_MOTOR_V1,
            POLICY_ID_V1,
        );
        assert!(digest.is_some());
        let (canary, identidad_canary) = crear_canary(&fixture);
        let storage = capturar_storage(&fixture);
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

        let relacion = owner.comparar_root_normal_v2().unwrap();
        assert_eq!(relacion, RelacionRootNormalV2::Coincide);
        assert_eq!(format!("{relacion:?}"), "Coincide");
        let dominado = owner
            .comparar_root_normal_v2_con_postcheck_prueba(|_| {
                Err(ErrorJournalAnclado::ParentChanged)
            })
            .unwrap_err();
        assert_eq!(dominado, ErrorComparacionRootNormalV2::OwnerUnverified);

        drop(owner);
        drop(lock);
        verificar_storage(&fixture, &storage);
        verificar_arbol(&fixture.root, &fixture.entradas);
        verificar_canary(&canary, identidad_canary);
    }

    #[test]
    fn root_ausente_devuelve_ausente_sin_tocar_storage() {
        let temp = tempdir().unwrap();
        let (fixture, digest) = preparar_normal_v2_con_tags(
            &temp.path().join("parent"),
            JournalState::Prepared,
            VERSION_MOTOR_V1,
            POLICY_ID_V1,
        );
        assert!(digest.is_some());
        let retenida = temp.path().join("root-retained");
        fs::rename(&fixture.root, &retenida).unwrap();
        let (canary, identidad_canary) = crear_canary(&fixture);
        let storage = capturar_storage(&fixture);
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

        let relacion = owner.comparar_root_normal_v2().unwrap();

        drop(owner);
        drop(lock);
        assert_eq!(relacion, RelacionRootNormalV2::Ausente);
        assert!(!fixture.root.exists());
        verificar_arbol(&retenida, &fixture.entradas);
        verificar_storage(&fixture, &storage);
        verificar_canary(&canary, identidad_canary);
    }

    #[test]
    fn mismo_digest_con_identidad_distinta_es_diferente() {
        let temp = tempdir().unwrap();
        let (fixture, digest) = preparar_normal_v2_con_tags(
            &temp.path().join("parent"),
            JournalState::Prepared,
            VERSION_MOTOR_V1,
            POLICY_ID_V1,
        );
        let digest = digest.unwrap();
        let retenida = temp.path().join("root-original-retained");
        fs::rename(&fixture.root, &retenida).unwrap();
        fs::create_dir(&fixture.root).unwrap();
        materializar(&fixture.root, &fixture.entradas);
        let (canary, identidad_canary) = crear_canary(&fixture);
        let storage = capturar_storage(&fixture);
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let observado = owner.observar_root_raw().unwrap().unwrap();
        let mismo_digest = ArtifactFingerprint::new(observado.identity().clone(), digest);
        assert!(observado == mismo_digest);
        assert!(observado.identity() != fixture.esperado.identity());
        assert!(observado != fixture.esperado);

        let relacion = owner.comparar_root_normal_v2().unwrap();

        drop(owner);
        drop(lock);
        assert_eq!(relacion, RelacionRootNormalV2::Diferente);
        verificar_arbol(&fixture.root, &fixture.entradas);
        verificar_arbol(&retenida, &fixture.entradas);
        verificar_storage(&fixture, &storage);
        verificar_canary(&canary, identidad_canary);
    }

    #[test]
    fn digest_alterado_con_identidad_igual_es_diferente() {
        let temp = tempdir().unwrap();
        let (fixture, digest) = preparar_normal_v2_con_tags(
            &temp.path().join("parent"),
            JournalState::Prepared,
            VERSION_MOTOR_V1,
            POLICY_ID_V1,
        );
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
        let (canary, identidad_canary) = crear_canary(&fixture);
        let storage = capturar_storage(&fixture);
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let observado = owner.observar_root_raw().unwrap().unwrap();
        let mismo_id = ArtifactFingerprint::new(fixture.esperado.identity().clone(), digest);
        assert!(observado.identity() == mismo_id.identity());
        assert!(observado != mismo_id);

        let relacion = owner.comparar_root_normal_v2().unwrap();

        drop(owner);
        drop(lock);
        assert_eq!(relacion, RelacionRootNormalV2::Diferente);
        assert_eq!(fs::read(path).unwrap(), bytes_modificados);
        verificar_storage(&fixture, &storage);
        verificar_canary(&canary, identidad_canary);
    }

    #[test]
    fn replay_incompatible_falla_y_postcheck_domina_el_error() {
        let temp = tempdir().unwrap();
        let (fixture, digest) = preparar_normal_v2_con_tags(
            &temp.path().join("parent"),
            JournalState::Prepared,
            "engine-unsupported/1",
            "policy-unsupported/1",
        );
        assert!(digest.is_none());
        let (canary, identidad_canary) = crear_canary(&fixture);
        let storage = capturar_storage(&fixture);
        let mut lock = LockEscritura::adquirir_en(&fixture.ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();

        let replay = owner.comparar_root_normal_v2().unwrap_err();
        assert_eq!(replay, ErrorComparacionRootNormalV2::ReplayUnverified);
        assert_eq!(format!("{replay:?}"), "root_normal_v2_replay_unverified");
        let dominado = owner
            .comparar_root_normal_v2_con_postcheck_prueba(|_| {
                Err(ErrorJournalAnclado::ParentChanged)
            })
            .unwrap_err();
        assert_eq!(dominado, ErrorComparacionRootNormalV2::OwnerUnverified);

        drop(owner);
        drop(lock);
        verificar_storage(&fixture, &storage);
        verificar_arbol(&fixture.root, &fixture.entradas);
        verificar_canary(&canary, identidad_canary);
    }
}
