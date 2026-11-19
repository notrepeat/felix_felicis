//! Observación v3 separada: no amplía el selector legacy ni habilita retiro ABC.
use super::{
    ErrorObservacionIntentVersionada as Error, NombresIntentVersionado, validar_temporales_intent,
};
use crate::transaccion::anclado::duenio_transaccion::{
    DuenioTransaccionAnclado, lector_intent_canonico,
};
use crate::transaccion::anclado::intent_persistido_v3::archivo::{IntentV3EnArchivo, leer_archivo};
use crate::transaccion::anclado::{identidad_archivo, scope_filesystem};

impl DuenioTransaccionAnclado<'_> {
    /// Exige Intent presente, temporales ausentes y owner verificable antes/después.
    /// Retiene el archivo; las sondas secuenciales no constituyen snapshot atómico.
    pub(in crate::transaccion::anclado::duenio_transaccion) fn observar_intent_v3(
        &mut self,
    ) -> Result<IntentV3EnArchivo, Error> {
        self.observar_intent_v3_con_hook(|| {})
    }

    pub(in crate::transaccion::anclado::duenio_transaccion) fn observar_intent_v3_con_hook<
        F: FnOnce(),
    >(
        &mut self,
        after_read: F,
    ) -> Result<IntentV3EnArchivo, Error> {
        self.revalidar_owner_y_temporales()?;
        let lectura = (|| {
            let nombres = NombresIntentVersionado::desde_owner(self)?;
            let parent = self
                .parent_revalidado()
                .map_err(|_| Error::OwnerUnverified)?;
            validar_temporales_intent(self, &parent, &nombres)?;
            let file = lector_intent_canonico::abrir_intent_canonico(&parent, &nombres.canonico)
                .map_err(|_| Error::CanonicalUnverified)?
                .ok_or(Error::CanonicalUnverified)?;
            let parent_id = self
                .identidad_parent
                .journal_identity()
                .ok_or(Error::OwnerUnverified)?;
            let observado = leer_archivo(file, self.scope_parent, &parent_id)
                .map_err(|_| Error::CanonicalUnverified)?;
            after_read();
            let probe = lector_intent_canonico::abrir_intent_canonico(&parent, &nombres.canonico)
                .map_err(|_| Error::CanonicalUnverified)?
                .ok_or(Error::CanonicalUnverified)?;
            let identity =
                identidad_archivo(&observado.archivo).map_err(|_| Error::CanonicalUnverified)?;
            if identidad_archivo(&probe).map_err(|_| Error::CanonicalUnverified)? != identity
                || scope_filesystem(&probe).map_err(|_| Error::CanonicalUnverified)?
                    != self.scope_parent
            {
                return Err(Error::CanonicalUnverified);
            }
            Ok(observado)
        })();
        self.revalidar_owner_y_temporales()?;
        lectura
    }
}
