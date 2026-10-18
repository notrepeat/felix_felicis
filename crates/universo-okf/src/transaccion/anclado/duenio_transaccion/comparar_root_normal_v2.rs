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
    use crate::transaccion::admision::envelope::{
        tests as fixtures, v2::EnvelopeContextoAdmisionV2,
    };
    use crate::transaccion::anclado::duenio_transaccion::fixture_root_replay::{
        FixturePublicada, contexto_path, id_archivo, materializar, preparar_con_tags,
        verificar_arbol,
    };
    use crate::transaccion::journal::v2::JournalV2;
    use crate::transaccion::journal::{ArtifactFingerprint, Journal, RawTreeDigest};
    use crate::transaccion::lock::LockEscritura;
    use crate::transaccion::replay::semantica_archivada::{POLICY_ID_V1, VERSION_MOTOR_V1};
    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;

    const CANARY: &[u8] = b"unrelated sibling remains unchanged";

    struct Storage {
        contexto: Vec<u8>,
        journal: Vec<u8>,
        identidad_contexto: crate::transaccion::anclado::IdentidadArchivo,
        identidad_journal: crate::transaccion::anclado::IdentidadArchivo,
    }

    fn preparar_normal_v2(
        parent: &Path,
        motor: &str,
        policy: &str,
    ) -> (FixturePublicada, Option<RawTreeDigest>) {
        let mut fixture = preparar_con_tags(parent, motor, policy);
        let journal_v1 = Journal::parse(&fixture.bytes_journal).unwrap();
        let journal_v2 = JournalV2::new(
            journal_v1.transaction_id(),
            journal_v1.parent().clone(),
            journal_v1.old().clone(),
            journal_v1.new_fingerprint().clone(),
        )
        .unwrap();
        let envelope = EnvelopeContextoAdmisionV2::ligar(
            fixtures::contexto_con_tags(b"\n", motor, policy),
            &journal_v2,
        );
        let digest = envelope
            .calcular_new_esperado(&journal_v2)
            .ok()
            .map(|new| new.digest().clone());
        if let Some(digest) = &digest {
            let esperado =
                ArtifactFingerprint::new(fixture.esperado.identity().clone(), digest.clone());
            assert!(esperado == fixture.esperado);
        }

        fixture.bytes_journal = journal_v2.encode().unwrap();
        fixture.bytes_contexto = envelope.encode().unwrap();
        fs::write(
            fixture.ubicacion.journal().as_path(),
            &fixture.bytes_journal,
        )
        .unwrap();
        fs::write(contexto_path(&fixture.ubicacion), &fixture.bytes_contexto).unwrap();
        (fixture, digest)
    }

    fn capturar_storage(fixture: &FixturePublicada) -> Storage {
        let contexto = contexto_path(&fixture.ubicacion);
        let journal = fixture.ubicacion.journal().as_path().to_path_buf();
        Storage {
            contexto: fs::read(&contexto).unwrap(),
            journal: fs::read(&journal).unwrap(),
            identidad_contexto: id_archivo(&contexto),
            identidad_journal: id_archivo(&journal),
        }
    }

    fn verificar_storage(fixture: &FixturePublicada, storage: &Storage) {
        crate::transaccion::anclado::duenio_transaccion::fixture_root_replay::assert_storage_unchanged(
            fixture,
            &storage.contexto,
            &storage.journal,
            storage.identidad_contexto,
            storage.identidad_journal,
        );
    }

    fn crear_canary(
        fixture: &FixturePublicada,
    ) -> (PathBuf, crate::transaccion::anclado::IdentidadArchivo) {
        let path = fixture.root.parent().unwrap().join("unrelated-canary");
        fs::write(&path, CANARY).unwrap();
        (path.clone(), id_archivo(&path))
    }

    fn verificar_canary(path: &Path, identidad: crate::transaccion::anclado::IdentidadArchivo) {
        assert_eq!(fs::read(path).unwrap(), CANARY);
        assert!(id_archivo(path) == identidad);
    }

    #[test]
    fn nominal_coincide_y_postcheck_domina_el_resultado_exitoso() {
        let temp = tempdir().unwrap();
        let (fixture, digest) =
            preparar_normal_v2(&temp.path().join("parent"), VERSION_MOTOR_V1, POLICY_ID_V1);
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
        let (fixture, digest) =
            preparar_normal_v2(&temp.path().join("parent"), VERSION_MOTOR_V1, POLICY_ID_V1);
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
        let (fixture, digest) =
            preparar_normal_v2(&temp.path().join("parent"), VERSION_MOTOR_V1, POLICY_ID_V1);
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
        let (fixture, digest) =
            preparar_normal_v2(&temp.path().join("parent"), VERSION_MOTOR_V1, POLICY_ID_V1);
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
        let (fixture, digest) = preparar_normal_v2(
            &temp.path().join("parent"),
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
