use super::DuenioTransaccionAnclado;
use crate::transaccion::{
    captura::{ConfigRawCapturada, configuracion_apply::ConfiguracionApplyPendiente},
    snapshot::ErrorSnapshot,
};
impl DuenioTransaccionAnclado<'_> {
    pub(crate) fn resolver_configuracion_apply(
        &mut self,
        fuentes: ConfiguracionApplyPendiente,
    ) -> Result<ConfigRawCapturada, ErrorSnapshot> {
        self.parent_revalidado().map_err(|_| ErrorSnapshot::Io)?;
        let result = (|| {
            let entradas = if fuentes.necesita_root() {
                let root = self
                    .abrir_root_lectura()
                    .map_err(|_| ErrorSnapshot::Io)?
                    .ok_or(ErrorSnapshot::Io)?;
                crate::transaccion::inventario::inventariar_root_anclado(root)?
            } else {
                Vec::new()
            };
            fuentes.resolver(&entradas)
        })();
        self.parent_revalidado().map_err(|_| ErrorSnapshot::Io)?;
        result
    }
}
