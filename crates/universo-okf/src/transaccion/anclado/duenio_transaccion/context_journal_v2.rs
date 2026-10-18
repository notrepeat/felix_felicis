//! Publica un recibo Journal V2 y vuelve a observar Contexto antes del postcheck.

use std::fmt;

use super::super::contexto_admision::ReciboContextoPublicado;
use super::super::journal_temporal::{
    ErrorJournalAnclado, IdentidadArchivoJournal, ResultadoPublicacionJournal,
};
use super::context_journal_v2_observation::EstadoObservacionContextoJournalV2 as EstadoContextoJournalV2;
use super::{
    DuenioTransaccionAnclado, ReciboTemporalJournalV2, ResultadoOwnerPostcheck,
    ejecutar_con_owner_y_postcheck,
};
use crate::transaccion::journal::JournalState;
use crate::transaccion::journal::v2::JournalV2;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ErrorFaseJournalV2 {
    NoPreparado,
}

impl fmt::Debug for ErrorFaseJournalV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("NoPreparado")
    }
}

pub(super) struct CuerpoContextoJournalV2 {
    inicial: EstadoContextoJournalV2,
    publicacion: Option<Result<ResultadoPublicacionJournal, ErrorJournalAnclado>>,
    final_contexto: Option<EstadoContextoJournalV2>,
    error_fase: Option<ErrorFaseJournalV2>,
}

impl fmt::Debug for CuerpoContextoJournalV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CuerpoContextoJournalV2 { .. }")
    }
}

impl<'owner, 'lock> ReciboTemporalJournalV2<'owner, 'lock> {
    /// Consume el temporal J2 y conserva el resultado crudo hasta el postcheck.
    ///
    /// Tras cualquier intento obtiene una lectura Contexto nueva. El postcheck
    /// exterior domina el resultado sin inferir si hubo efecto.
    #[must_use]
    pub(super) fn publicar_con_contexto(
        self,
        recibo_contexto: &ReciboContextoPublicado,
    ) -> ResultadoOwnerPostcheck<CuerpoContextoJournalV2> {
        self.publicar_con_contexto_core(
            recibo_contexto,
            super::publicar_journal_v2_desde_owner,
            |owner, _| owner.parent_revalidado().map(|_| ()),
        )
    }

    #[cfg(test)]
    fn publicar_con_contexto_con_prueba<Publicar, Postcheck>(
        self,
        recibo_contexto: &ReciboContextoPublicado,
        publicar: Publicar,
        postcheck: Postcheck,
    ) -> ResultadoOwnerPostcheck<CuerpoContextoJournalV2>
    where
        Publicar: FnOnce(
            &mut DuenioTransaccionAnclado<'lock>,
            IdentidadArchivoJournal,
            &JournalV2,
        ) -> Result<ResultadoPublicacionJournal, ErrorJournalAnclado>,
        Postcheck: FnOnce(
            &mut DuenioTransaccionAnclado<'lock>,
            &CuerpoContextoJournalV2,
        ) -> Result<(), ErrorJournalAnclado>,
    {
        self.publicar_con_contexto_core(recibo_contexto, publicar, postcheck)
    }

    fn publicar_con_contexto_core<Publicar, Postcheck>(
        self,
        recibo_contexto: &ReciboContextoPublicado,
        publicar: Publicar,
        postcheck: Postcheck,
    ) -> ResultadoOwnerPostcheck<CuerpoContextoJournalV2>
    where
        Publicar: FnOnce(
            &mut DuenioTransaccionAnclado<'lock>,
            IdentidadArchivoJournal,
            &JournalV2,
        ) -> Result<ResultadoPublicacionJournal, ErrorJournalAnclado>,
        Postcheck: FnOnce(
            &mut DuenioTransaccionAnclado<'lock>,
            &CuerpoContextoJournalV2,
        ) -> Result<(), ErrorJournalAnclado>,
    {
        let Self {
            _owner: owner,
            journal,
            identidad,
        } = self;

        ejecutar_con_owner_y_postcheck(
            owner,
            |_| {},
            |owner| {
                let inicial =
                    super::context_journal_v2_observation::observar_contexto_con_journal_v2(
                        owner,
                        recibo_contexto,
                        &journal,
                    );
                let mut cuerpo = CuerpoContextoJournalV2 {
                    inicial,
                    publicacion: None,
                    final_contexto: None,
                    error_fase: None,
                };
                if !matches!(&cuerpo.inicial, EstadoContextoJournalV2::Observado(_)) {
                    return cuerpo;
                }
                if journal.state() != JournalState::Prepared {
                    cuerpo.error_fase = Some(ErrorFaseJournalV2::NoPreparado);
                    return cuerpo;
                }

                cuerpo.publicacion = Some(publicar(owner, identidad, &journal));
                cuerpo.final_contexto = Some(
                    super::context_journal_v2_observation::observar_contexto_con_journal_v2(
                        owner,
                        recibo_contexto,
                        &journal,
                    ),
                );
                cuerpo
            },
            |owner, cuerpo| postcheck(owner, cuerpo),
        )
    }
}

#[cfg(test)]
#[path = "context_journal_v2_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "context_journal_v2_adversarial_tests.rs"]
mod adversarial_tests;
