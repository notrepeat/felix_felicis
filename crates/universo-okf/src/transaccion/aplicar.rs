//! Fachada transaccional: configuración acotada, recovery, CAS y publicación.
use super::{
    anclado::duenio_transaccion::{
        DuenioTransaccionAnclado, ErrorRecoveryPendienteV2,
        instalar_prepared::{EvidenciaFinalizacionPrepared, EvidenciaInstalacionPrepared},
        preparar_apply::{ErrorPreparacionApply, ResultadoPreparacionApply},
        publicar_prepared::EvidenciaPublicacionPrepared,
    },
    captura::configuracion_apply::capturar_configuracion_para_apply,
    historial::Procedencia,
    lock::LockEscritura,
    raiz::UbicacionRaiz,
    snapshot::SnapshotToken,
};
use std::{fmt, path::Path};
use universo_core::ResultadoPropuesta;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EstadoAplicacion {
    Aplicada,
    Rechazada,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResultadoAplicacion {
    pub estado: EstadoAplicacion,
    pub conflictos: usize,
    pub invariantes_fallidas: usize,
    /// `Aplicada`: se escribió el sello del estado admitido (ver `sello`).
    /// `false` si no se pudo, o si el root cambió fuera de Felix entre la
    /// instalación y el sellado. En un rechazo el sello no se toca.
    pub sellado: bool,
    /// `Aplicada`: se añadió la entrada al historial de procedencia.
    pub registrado: bool,
}
#[allow(dead_code)] // Campos retenidos deliberadamente; nunca serializados.
enum EvidenciaAplicacion {
    Preparacion(Box<ErrorPreparacionApply>),
    Publicacion(Box<EvidenciaPublicacionPrepared>),
    Instalacion(Box<EvidenciaInstalacionPrepared>),
    Finalizacion(Box<EvidenciaFinalizacionPrepared>),
    Recovery(Box<ErrorRecoveryPendienteV2>),
}
/// Error redactado que retiene la evidencia de operaciones parciales.
pub struct ErrorAplicacion {
    codigo: &'static str,
    evidencia: Option<EvidenciaAplicacion>,
}
impl ErrorAplicacion {
    fn nuevo(codigo: &'static str) -> Self {
        Self {
            codigo,
            evidencia: None,
        }
    }
    pub fn codigo(&self) -> &'static str {
        self.codigo
    }
    pub fn conserva_evidencia(&self) -> bool {
        self.evidencia.is_some()
    }
    fn retenido(codigo: &'static str, evidencia: EvidenciaAplicacion) -> Self {
        Self {
            codigo,
            evidencia: Some(evidencia),
        }
    }
}
impl fmt::Debug for ErrorAplicacion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.codigo)
    }
}
impl fmt::Display for ErrorAplicacion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.codigo)
    }
}
impl std::error::Error for ErrorAplicacion {}

/// Lee delta/configuración antes del lock y conserva el guard hasta cleanup.
/// La configuración externa se captura previamente; la interna se resuelve
/// desde el root anclado después del recovery bajo el mismo lock.
pub fn aplicar_archivo(
    raiz: &Path,
    manifiesto: &Path,
    invariantes: &Path,
    delta_path: &Path,
    esperado: SnapshotToken,
) -> Result<ResultadoAplicacion, ErrorAplicacion> {
    aplicar_archivo_con_procedencia(
        raiz,
        manifiesto,
        invariantes,
        delta_path,
        esperado,
        &Procedencia::default(),
    )
}

/// Como [`aplicar_archivo`], y registra autor y motivo en el historial.
pub fn aplicar_archivo_con_procedencia(
    raiz: &Path,
    manifiesto: &Path,
    invariantes: &Path,
    delta_path: &Path,
    esperado: SnapshotToken,
    procedencia: &Procedencia,
) -> Result<ResultadoAplicacion, ErrorAplicacion> {
    procedencia
        .validar()
        .map_err(|e| ErrorAplicacion::nuevo(e.codigo()))?;
    aplicar_archivo_con_hooks(
        raiz,
        manifiesto,
        invariantes,
        delta_path,
        esperado,
        procedencia,
        || {},
        || {},
    )
}

/// `despues_stage` corre justo después de crear el stage y `antes_de_sellar`
/// tras `Committed`, antes de recapturar el estado (solo para tests).
#[allow(clippy::too_many_arguments)] // Privada: los dos hooks finales sólo los usan los tests.
fn aplicar_archivo_con_hooks(
    raiz: &Path,
    manifiesto: &Path,
    invariantes: &Path,
    delta_path: &Path,
    esperado: SnapshotToken,
    procedencia: &Procedencia,
    despues_stage: impl FnOnce(),
    antes_de_sellar: impl FnOnce(),
) -> Result<ResultadoAplicacion, ErrorAplicacion> {
    let ubicacion =
        UbicacionRaiz::nueva(raiz).map_err(|_| ErrorAplicacion::nuevo("apply_root_invalid"))?;
    let config = capturar_configuracion_para_apply(ubicacion.ruta(), manifiesto, invariantes)
        .map_err(|e| ErrorAplicacion::nuevo(e.codigo()))?;
    let bytes = super::inventario::leer_archivo_regular_limitado(
        delta_path,
        super::simulacion::MAX_DELTA_BYTES + 1,
    )
    .map_err(|e| ErrorAplicacion::nuevo(e.codigo()))?;
    if bytes.len() > super::simulacion::MAX_DELTA_BYTES {
        return Err(ErrorAplicacion::nuevo("delta_resource_limit"));
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| ErrorAplicacion::nuevo("delta_invalid"))?;
    let delta =
        crate::parsear_delta_json(text).map_err(|_| ErrorAplicacion::nuevo("delta_invalid"))?;
    let mut lock =
        LockEscritura::adquirir_en(&ubicacion).map_err(|e| ErrorAplicacion::nuevo(e.codigo()))?;
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock)
        .map_err(|_| ErrorAplicacion::nuevo("apply_owner_invalid"))?;
    owner.recuperar_pendiente_v2().map_err(|e| {
        ErrorAplicacion::retenido(
            "apply_recovery_unverified",
            EvidenciaAplicacion::Recovery(Box::new(e)),
        )
    })?;
    let config = owner
        .resolver_configuracion_apply(config)
        .map_err(|e| ErrorAplicacion::nuevo(e.codigo()))?;
    let prepared = match owner.preparar_apply_con_hook(config, esperado, &delta, despues_stage) {
        Ok(prepared) => prepared,
        Err(e) => {
            let e = owner.limpiar_stage_de_preparacion_fallida(e);
            return Err(ErrorAplicacion::retenido(
                e.codigo(),
                EvidenciaAplicacion::Preparacion(Box::new(e)),
            ));
        }
    };
    let prepared = match prepared {
        ResultadoPreparacionApply::Preparada(prepared) => prepared,
        ResultadoPreparacionApply::Rechazada(result) => {
            let (conflictos, invariantes_fallidas) = match result {
                ResultadoPropuesta::RechazadaConflictos(conflicts) => {
                    (conflicts.total_conflictos(), 0)
                }
                ResultadoPropuesta::RechazadaInvariantes(failures) => (0, failures.len()),
                ResultadoPropuesta::Admitida(_) => {
                    return Err(ErrorAplicacion::nuevo("apply_unexpected_verdict"));
                }
            };
            return Ok(ResultadoAplicacion {
                estado: EstadoAplicacion::Rechazada,
                conflictos,
                invariantes_fallidas,
                sellado: false,
                registrado: false,
            });
        }
    };
    // Desde aquí el root puede cambiar: el sello anterior deja de describirlo.
    // Mejor esfuerzo: si no se puede retirar, queda un sello viejo, que la
    // guardia informa como «difiere» (alarma conservadora, nunca falso OK).
    let _ = super::sello::borrar_sello(&ubicacion);
    let digest_nuevo = prepared.digest_nuevo();
    let publicada = prepared.publicar_prepared().map_err(|e| {
        ErrorAplicacion::retenido(
            "prepared_publication_unverified",
            EvidenciaAplicacion::Publicacion(Box::new(e.into_evidencia_sin_owner())),
        )
    })?;
    let instalada = publicada.instalar().map_err(|e| {
        ErrorAplicacion::retenido(
            "prepared_installation_unverified",
            EvidenciaAplicacion::Instalacion(Box::new(e.into_evidencia_sin_owner())),
        )
    })?;
    instalada.finalizar().map_err(|e| {
        ErrorAplicacion::retenido(
            "prepared_finalization_unverified",
            EvidenciaAplicacion::Finalizacion(Box::new(e.into_evidencia_sin_owner())),
        )
    })?;
    // Sella el estado admitido bajo el mismo lock, sólo si el árbol recapturado
    // es exactamente el que se verificó e instaló: una edición ajena en esa
    // ventana (el lock es advisory) no debe quedar aceptada. El commit ya es
    // durable: no sellar no deshace la aplicación; se informa en `sellado`.
    antes_de_sellar();
    let sellado_con =
        match super::sello::capturar_veredicto_con_digest(raiz, manifiesto, invariantes) {
            Ok((token, 0, digest)) if digest == digest_nuevo => {
                super::sello::escribir_sello(&ubicacion, token)
                    .is_ok()
                    .then_some(token)
            }
            _ => None,
        };
    let sellado = sellado_con.is_some();
    // Historial informativo bajo el mismo lock; un fallo no deshace el commit.
    let registrado =
        super::historial::anotar(&ubicacion, procedencia, sellado_con, sellado).is_ok();
    Ok(ResultadoAplicacion {
        estado: EstadoAplicacion::Aplicada,
        conflictos: 0,
        invariantes_fallidas: 0,
        sellado,
        registrado,
    })
}

#[cfg(test)]
#[path = "aplicar_tests.rs"]
mod tests;
