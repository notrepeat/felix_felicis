//! Clasificación pura de retiro v3; nunca transforma datos en permisos de borrado.
use super::IntentRetiroPersistidoV3;
use crate::transaccion::journal::selector::JournalPersistido;

#[derive(Debug)]
pub(in crate::transaccion::anclado) enum FamiliaV3<'a> {
    RetiroA(&'a IntentRetiroPersistidoV3),
    RetiroB(&'a IntentRetiroPersistidoV3),
    CConContextoParaReplay(&'a IntentRetiroPersistidoV3),
}

#[derive(Debug, PartialEq, Eq)]
pub(in crate::transaccion::anclado) struct ErrorFamiliaV3;

pub(in crate::transaccion::anclado) fn clasificar<'a>(
    journal: Option<&JournalPersistido>,
    contexto_original: Option<&[u8]>,
    intent: &'a IntentRetiroPersistidoV3,
) -> Result<FamiliaV3<'a>, ErrorFamiliaV3> {
    if let Some(contexto) = contexto_original {
        if contexto != intent.contexto_original() {
            return Err(ErrorFamiliaV3);
        }
    }
    match (journal, contexto_original) {
        (Some(JournalPersistido::V2(journal)), Some(_))
            if journal == intent.metadata().journal() =>
        {
            Ok(FamiliaV3::RetiroA(intent))
        }
        (None, Some(_)) => Ok(FamiliaV3::RetiroB(intent)),
        (None, None) => Ok(FamiliaV3::CConContextoParaReplay(intent)),
        _ => Err(ErrorFamiliaV3),
    }
}

#[cfg(test)]
#[path = "familia_tests.rs"]
mod tests;
