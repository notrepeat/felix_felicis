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

/// Conserva la pareja A1 tras revalidar los dos nombres por identidad nativa.
/// Es dato de filesystem observado, no representa una fase ni concede autoridad.
#[must_use]
pub(super) struct ParejaV2Revalidada {
    pareja: ParejaV2LeidaLigada,
}

impl fmt::Debug for ParejaV2Revalidada {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ParejaV2Revalidada { .. }")
    }
}

/// Retiene la pareja A2 tras comprobar los seis slots antes y después.
/// La identidad final de Journal y Context sigue pendiente tras el guard final.
/// Es una observación privada, no clasifica fase ni concede autoridad.
#[must_use]
pub(super) struct ObservacionParejaV2ConSlotsVerificadosPendienteIdentidad {
    pareja: ParejaV2Revalidada,
}

impl fmt::Debug for ObservacionParejaV2ConSlotsVerificadosPendienteIdentidad {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ObservacionParejaV2ConSlotsVerificadosPendienteIdentidad { .. }")
    }
}

impl DuenioTransaccionAnclado<'_> {
    /// Comprueba slots antes y después de A2; B2b aún debe repetir ambas identidades.
    pub(super) fn observar_pareja_v2_con_slots_verificados_pendiente_identidad(
        &mut self,
    ) -> Result<ObservacionParejaV2ConSlotsVerificadosPendienteIdentidad, ErrorObservacionPareadaV2>
    {
        self.observar_slots_con_postcheck(|_| {}, |owner| owner.parent_revalidado())
    }

    #[cfg(test)]
    pub(super) fn observar_pareja_v2_con_slots_verificados_pendiente_identidad_con_prueba<H, P>(
        &mut self,
        despues_de_a2: H,
        postcheck: P,
    ) -> Result<ObservacionParejaV2ConSlotsVerificadosPendienteIdentidad, ErrorObservacionPareadaV2>
    where
        H: FnOnce(&mut Self),
        P: FnOnce(&Self) -> Result<File, ErrorJournalAnclado>,
    {
        self.observar_slots_con_postcheck(despues_de_a2, postcheck)
    }

    fn observar_slots_con_postcheck<H, P>(
        &mut self,
        despues_de_a2: H,
        postcheck: P,
    ) -> Result<ObservacionParejaV2ConSlotsVerificadosPendienteIdentidad, ErrorObservacionPareadaV2>
    where
        H: FnOnce(&mut Self),
        P: FnOnce(&Self) -> Result<File, ErrorJournalAnclado>,
    {
        let parent = self
            .parent_revalidado()
            .map_err(|_| ErrorObservacionPareadaV2::OwnerUnverified)?;
        drop(parent);

        let resultado = (|| {
            let inicial = self
                .observar_siblings_nuevo_flujo()
                .map_err(|_| ErrorObservacionPareadaV2::PairUnverified)?;
            inicial
                .exigir_slots_normal_v2()
                .map_err(|_| ErrorObservacionPareadaV2::PairUnverified)?;

            let pareja = self.revalidar_pareja_v2()?;
            despues_de_a2(self);

            let final_observado = self
                .observar_siblings_nuevo_flujo()
                .map_err(|_| ErrorObservacionPareadaV2::PairUnverified)?;
            final_observado
                .exigir_slots_normal_v2()
                .map_err(|_| ErrorObservacionPareadaV2::PairUnverified)?;

            Ok(ObservacionParejaV2ConSlotsVerificadosPendienteIdentidad { pareja })
        })();

        let parent = postcheck(self).map_err(|_| ErrorObservacionPareadaV2::OwnerUnverified)?;
        drop(parent);
        resultado
    }
}

impl DuenioTransaccionAnclado<'_> {
    /// Revalida los nombres canónicos de la pareja retenida bajo el owner.
    pub(super) fn revalidar_pareja_v2(
        &mut self,
    ) -> Result<ParejaV2Revalidada, ErrorObservacionPareadaV2> {
        self.revalidar_con_postcheck(
            |owner| owner.leer_y_confirmar_pareja(),
            |owner| owner.parent_revalidado(),
        )
    }

    #[cfg(test)]
    pub(super) fn revalidar_pareja_v2_con_prueba<H, P>(
        &mut self,
        despues_del_binding: H,
        postcheck: P,
    ) -> Result<ParejaV2Revalidada, ErrorObservacionPareadaV2>
    where
        H: FnOnce(&mut Self),
        P: FnOnce(&Self) -> Result<File, ErrorJournalAnclado>,
    {
        self.revalidar_con_postcheck(
            |owner| owner.leer_y_confirmar_pareja_con_prueba(despues_del_binding),
            postcheck,
        )
    }

    fn revalidar_con_postcheck<C, P>(
        &mut self,
        cuerpo: C,
        postcheck: P,
    ) -> Result<ParejaV2Revalidada, ErrorObservacionPareadaV2>
    where
        C: FnOnce(&mut Self) -> Result<ParejaV2Revalidada, ErrorObservacionPareadaV2>,
        P: FnOnce(&Self) -> Result<File, ErrorJournalAnclado>,
    {
        let parent = self
            .parent_revalidado()
            .map_err(|_| ErrorObservacionPareadaV2::OwnerUnverified)?;
        drop(parent);

        let resultado = cuerpo(self);
        let parent = postcheck(self).map_err(|_| ErrorObservacionPareadaV2::OwnerUnverified)?;
        drop(parent);
        resultado
    }

    fn leer_y_confirmar_pareja(&mut self) -> Result<ParejaV2Revalidada, ErrorObservacionPareadaV2> {
        let pareja = self.leer_pareja_v2_ligada()?;
        self.confirmar_identidades_pareja(&pareja)?;
        Ok(ParejaV2Revalidada { pareja })
    }

    #[cfg(test)]
    fn leer_y_confirmar_pareja_con_prueba<H>(
        &mut self,
        despues_del_binding: H,
    ) -> Result<ParejaV2Revalidada, ErrorObservacionPareadaV2>
    where
        H: FnOnce(&mut Self),
    {
        let pareja = self.leer_pareja_v2_ligada_con_prueba(despues_del_binding, |owner| {
            owner.parent_revalidado()
        })?;
        self.confirmar_identidades_pareja(&pareja)?;
        Ok(ParejaV2Revalidada { pareja })
    }

    fn confirmar_identidades_pareja(
        &self,
        pareja: &ParejaV2LeidaLigada,
    ) -> Result<(), ErrorObservacionPareadaV2> {
        let ubicacion = self._lock.ubicacion();
        let nombre_journal = ubicacion
            .journal()
            .as_path()
            .file_name()
            .ok_or(ErrorObservacionPareadaV2::PairUnverified)?
            .to_os_string();
        let basename = ubicacion
            .ruta()
            .file_name()
            .ok_or(ErrorObservacionPareadaV2::PairUnverified)?;
        let nombre_contexto = super::super::contexto_admision::nombre_canonico(basename)
            .map_err(|_| ErrorObservacionPareadaV2::PairUnverified)?;

        super::visible_identity_probe::confirmar_sibling_presente_con_identidad(
            &self.parent,
            &nombre_journal,
            self.scope_parent,
            pareja.journal.identidad().identidad_archivo(),
        )
        .map_err(|_| ErrorObservacionPareadaV2::PairUnverified)?;
        super::visible_identity_probe::confirmar_sibling_presente_con_identidad(
            &self.parent,
            &nombre_contexto,
            self.scope_parent,
            pareja.contexto.identidad_archivo_observado(),
        )
        .map_err(|_| ErrorObservacionPareadaV2::PairUnverified)
    }
}

/// Conserva B2a tras repetir las identidades nativas después del guard de slots.
/// Es observación privada; no clasifica fase ni concede autoridad.
#[must_use]
pub(super) struct ObservacionParejaV2ConIdentidadesFinales {
    observacion: ObservacionParejaV2ConSlotsVerificadosPendienteIdentidad,
}

impl fmt::Debug for ObservacionParejaV2ConIdentidadesFinales {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ObservacionParejaV2ConIdentidadesFinales { .. }")
    }
}

impl DuenioTransaccionAnclado<'_> {
    /// Repite J/C por identidad después de B2a y vuelve a comprobar el owner.
    pub(super) fn observar_pareja_v2_con_identidades_finales(
        &mut self,
    ) -> Result<ObservacionParejaV2ConIdentidadesFinales, ErrorObservacionPareadaV2> {
        self.observar_identidades_con_postcheck(|_| {}, |owner| owner.parent_revalidado())
    }

    /// El hook corre después del guard final y postcheck propios de B2a, justo
    /// antes de reabrir J/C para comparar sus identidades.
    #[cfg(test)]
    pub(super) fn observar_pareja_v2_con_identidades_finales_con_prueba<H, P>(
        &mut self,
        despues_de_b2a: H,
        postcheck: P,
    ) -> Result<ObservacionParejaV2ConIdentidadesFinales, ErrorObservacionPareadaV2>
    where
        H: FnOnce(&mut Self),
        P: FnOnce(&Self) -> Result<File, ErrorJournalAnclado>,
    {
        self.observar_identidades_con_postcheck(despues_de_b2a, postcheck)
    }

    fn observar_identidades_con_postcheck<H, P>(
        &mut self,
        despues_de_b2a: H,
        postcheck: P,
    ) -> Result<ObservacionParejaV2ConIdentidadesFinales, ErrorObservacionPareadaV2>
    where
        H: FnOnce(&mut Self),
        P: FnOnce(&Self) -> Result<File, ErrorJournalAnclado>,
    {
        let parent = self
            .parent_revalidado()
            .map_err(|_| ErrorObservacionPareadaV2::OwnerUnverified)?;
        drop(parent);

        let resultado = (|| {
            let observacion =
                self.observar_pareja_v2_con_slots_verificados_pendiente_identidad()?;
            despues_de_b2a(self);
            self.confirmar_identidades_pareja(&observacion.pareja.pareja)?;
            Ok(ObservacionParejaV2ConIdentidadesFinales { observacion })
        })();

        let parent = postcheck(self).map_err(|_| ErrorObservacionPareadaV2::OwnerUnverified)?;
        drop(parent);
        resultado
    }
}
