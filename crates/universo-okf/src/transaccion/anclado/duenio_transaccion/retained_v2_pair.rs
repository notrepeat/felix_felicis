use std::fmt;
use std::fs::File;

use crate::transaccion::journal::selector::JournalPersistido;

use super::super::contexto_admision::ObservacionContextoVersionada;
use super::super::journal_temporal::{
    ErrorJournalAnclado, versionado::ObservacionJournalPersistido,
};
use super::DuenioTransaccionAnclado;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ErrorObservacionPareadaV2 {
    OwnerUnverified,
    PairUnverified,
}

impl fmt::Debug for ErrorObservacionPareadaV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::OwnerUnverified => "OwnerUnverified",
            Self::PairUnverified => "PairUnverified",
        })
    }
}

/// Retiene las lecturas J2/ENV2 cuyo binding estructural ya coincide.
/// No confirma la identidad final de los nombres canónicos.
#[must_use]
pub(super) struct ParejaV2LeidaLigada {
    journal: ObservacionJournalPersistido,
    contexto: ObservacionContextoVersionada,
}

impl fmt::Debug for ParejaV2LeidaLigada {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ParejaV2LeidaLigada { .. }")
    }
}

impl DuenioTransaccionAnclado<'_> {
    /// Lee y retiene J2/ENV2 con binding coincidente, sin clasificar una fase.
    pub(super) fn leer_pareja_v2_ligada(
        &mut self,
    ) -> Result<ParejaV2LeidaLigada, ErrorObservacionPareadaV2> {
        self.leer_con_postcheck(|_| {}, |owner| owner.parent_revalidado())
    }

    #[cfg(test)]
    pub(super) fn leer_pareja_v2_ligada_con_prueba<H, P>(
        &mut self,
        despues_del_binding: H,
        postcheck: P,
    ) -> Result<ParejaV2LeidaLigada, ErrorObservacionPareadaV2>
    where
        H: FnOnce(&mut Self),
        P: FnOnce(&Self) -> Result<File, ErrorJournalAnclado>,
    {
        self.leer_con_postcheck(despues_del_binding, postcheck)
    }

    fn leer_con_postcheck<H, P>(
        &mut self,
        despues_del_binding: H,
        postcheck: P,
    ) -> Result<ParejaV2LeidaLigada, ErrorObservacionPareadaV2>
    where
        H: FnOnce(&mut Self),
        P: FnOnce(&Self) -> Result<File, ErrorJournalAnclado>,
    {
        let parent = self
            .parent_revalidado()
            .map_err(|_| ErrorObservacionPareadaV2::OwnerUnverified)?;
        drop(parent);

        let resultado = self.leer_y_ligar_pareja(despues_del_binding);
        let parent = postcheck(self).map_err(|_| ErrorObservacionPareadaV2::OwnerUnverified)?;
        drop(parent);
        resultado
    }

    fn leer_y_ligar_pareja<H>(
        &mut self,
        despues_del_binding: H,
    ) -> Result<ParejaV2LeidaLigada, ErrorObservacionPareadaV2>
    where
        H: FnOnce(&mut Self),
    {
        let journal = self
            .observar_journal_versionado()
            .map_err(|_| ErrorObservacionPareadaV2::PairUnverified)?
            .ok_or(ErrorObservacionPareadaV2::PairUnverified)?;
        let contexto = self
            .observar_contexto_versionado()
            .map_err(|_| ErrorObservacionPareadaV2::PairUnverified)?
            .ok_or(ErrorObservacionPareadaV2::PairUnverified)?;

        let JournalPersistido::V2(journal_v2) = journal.journal() else {
            return Err(ErrorObservacionPareadaV2::PairUnverified);
        };
        contexto
            .validar_binding_v2(journal_v2)
            .map_err(|_| ErrorObservacionPareadaV2::PairUnverified)?;

        despues_del_binding(self);
        Ok(ParejaV2LeidaLigada { journal, contexto })
    }
}
