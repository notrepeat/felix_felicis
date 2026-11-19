//! Expectativa raw ligada al registro tipado de Journal v2.
//!
//! Este cálculo consume datos persistidos ya tipados; no comprueba autoridad ni
//! observa el filesystem. El resultado conserva el registro completo.

use std::fmt;

use crate::transaccion::admision::ContextoAdmision;
use crate::transaccion::journal::v2::{JournalKind, JournalV2};
use crate::transaccion::journal::{ArtifactFingerprint, RawTreeDigest};
use crate::transaccion::snapshot::{EntradaSnapshot, calcular_digest_raw_tree};

use super::ErrorReplay;
use super::new_esperado::{
    ErrorNewEsperado, InventarioRawRepropuesto, calcular_inventario_raw_repropuesto,
};
use super::semantica_archivada::{POLICY_ID_V1, VERSION_MOTOR_V1};

/// Datos raw esperados que retienen sin proyección el Journal v2 recibido.
pub(crate) struct NewEsperadoV2 {
    journal: JournalV2,
    entradas: Vec<EntradaSnapshot>,
    digest: RawTreeDigest,
}

impl NewEsperadoV2 {
    pub(crate) fn journal(&self) -> &JournalV2 {
        &self.journal
    }

    pub(crate) fn entradas(&self) -> &[EntradaSnapshot] {
        &self.entradas
    }

    pub(crate) fn digest(&self) -> &RawTreeDigest {
        &self.digest
    }
}

impl fmt::Debug for NewEsperadoV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("NewEsperadoV2 { .. }")
    }
}

/// Error cerrado y redactado al reconstruir una expectativa desde Journal v2.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorNewEsperadoV2 {
    JournalKindUnsupported,
    Replay(ErrorReplay),
    Calculo(ErrorNewEsperado),
    HuellaOldNoCoincide,
    HuellaNewNoCoincide,
}

impl ErrorNewEsperadoV2 {
    pub(crate) const fn codigo(self) -> &'static str {
        match self {
            Self::JournalKindUnsupported => "new_v2_journal_kind_unsupported",
            Self::Replay(error) => error.codigo(),
            Self::Calculo(error) => error.codigo(),
            Self::HuellaOldNoCoincide => "new_old_fingerprint_mismatch",
            Self::HuellaNewNoCoincide => "new_expected_fingerprint_mismatch",
        }
    }
}

impl fmt::Debug for ErrorNewEsperadoV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl fmt::Display for ErrorNewEsperadoV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorNewEsperadoV2 {}

/// Reconstruye el inventario raw para el perfil cerrado y conserva Journal v2.
pub(crate) fn calcular_new_esperado_v2(
    contexto: &ContextoAdmision,
    journal: &JournalV2,
) -> Result<NewEsperadoV2, ErrorNewEsperadoV2> {
    if journal.kind() != JournalKind::ContextRequired {
        return Err(ErrorNewEsperadoV2::JournalKindUnsupported);
    }
    if contexto.version_motor() != VERSION_MOTOR_V1 || contexto.policy_id() != POLICY_ID_V1 {
        return Err(ErrorNewEsperadoV2::Replay(
            ErrorReplay::MotorPoliticaNoCoinciden,
        ));
    }
    contexto.validar_base().map_err(|error| {
        ErrorNewEsperadoV2::Calculo(ErrorNewEsperado::Replay(ErrorReplay::Contexto(error)))
    })?;

    let digest_old = calcular_digest_raw_tree(&contexto.entradas_base())
        .map_err(|error| ErrorNewEsperadoV2::Calculo(ErrorNewEsperado::Snapshot(error)))?;
    let old = ArtifactFingerprint::new(journal.old().identity().clone(), digest_old);
    if &old != journal.old() {
        return Err(ErrorNewEsperadoV2::HuellaOldNoCoincide);
    }

    let inventario = calcular_inventario_raw_repropuesto(contexto, VERSION_MOTOR_V1, POLICY_ID_V1)
        .map_err(ErrorNewEsperadoV2::Calculo)?;
    validar_new(journal, &inventario)?;

    Ok(NewEsperadoV2 {
        journal: journal.clone(),
        entradas: inventario.entradas,
        digest: inventario.digest,
    })
}

fn validar_new(
    journal: &JournalV2,
    inventario: &InventarioRawRepropuesto,
) -> Result<(), ErrorNewEsperadoV2> {
    let new = ArtifactFingerprint::new(
        journal.new_fingerprint().identity().clone(),
        inventario.digest.clone(),
    );
    if &new == journal.new_fingerprint() {
        Ok(())
    } else {
        Err(ErrorNewEsperadoV2::HuellaNewNoCoincide)
    }
}
