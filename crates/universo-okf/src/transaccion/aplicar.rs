//! Fachada transaccional: configuración acotada, recovery, CAS y publicación.
use super::{
    anclado::duenio_transaccion::{
        DuenioTransaccionAnclado, ErrorRecoveryPendienteV2,
        instalar_prepared::{EvidenciaFinalizacionPrepared, EvidenciaInstalacionPrepared},
        preparar_apply::{ErrorPreparacionApply, ResultadoPreparacionApply},
        publicar_prepared::EvidenciaPublicacionPrepared,
    },
    captura::configuracion_apply::capturar_configuracion_para_apply,
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
        crate::parsear_delta_json_v1(text).map_err(|_| ErrorAplicacion::nuevo("delta_invalid"))?;
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
    let prepared = owner
        .preparar_apply(config, esperado, &delta)
        .map_err(|e| {
            ErrorAplicacion::retenido(e.codigo(), EvidenciaAplicacion::Preparacion(Box::new(e)))
        })?;
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
            });
        }
    };
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
    Ok(ResultadoAplicacion {
        estado: EstadoAplicacion::Aplicada,
        conflictos: 0,
        invariantes_fallidas: 0,
    })
}
