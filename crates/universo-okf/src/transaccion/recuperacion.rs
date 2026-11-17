//! Recovery de retiro v3 autocontenido. No ejecuta rollback de journal normal.
use super::anclado::duenio_transaccion::{
    DuenioTransaccionAnclado, EstadoPasoRetiroV3, EvidenciaRetiroPreservada, ResultadoPasoRetiroV3,
};
use super::lock::LockEscritura;
use std::{fmt, path::Path};

/// Error propietario: conserva la evidencia recuperada sin mostrarla en diagnósticos.
/// Un resultado ambiguo exige nueva inspección, nunca retry ciego.
pub struct ErrorRecuperacion {
    codigo: &'static str,
    evidencia: Option<Box<EvidenciaRetiroPreservada>>,
}
impl ErrorRecuperacion {
    pub const fn codigo(&self) -> &'static str {
        self.codigo
    }
    /// Indica si el error mantiene el contexto autocontenido del último paso.
    pub fn conserva_evidencia(&self) -> bool {
        self.evidencia.is_some()
    }
    pub(crate) fn desde_paso(codigo: &'static str, paso: ResultadoPasoRetiroV3<'_, '_>) -> Self {
        Self {
            codigo,
            evidencia: Some(Box::new(paso.into_evidencia())),
        }
    }
    #[cfg(test)]
    pub(crate) fn contexto_para_test(&self) -> Option<&[u8]> {
        self.evidencia
            .as_ref()
            .map(|evidencia| evidencia.contexto_para_test())
    }
}
impl fmt::Debug for ErrorRecuperacion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.codigo)
    }
}
impl fmt::Display for ErrorRecuperacion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.codigo)
    }
}
impl std::error::Error for ErrorRecuperacion {}

/// Completa A→B→C de un Intent v3 existente bajo un solo lock cooperativo.
/// Cada transición reobserva el resultado. Fases normales, datos inválidos o
/// resultados ambiguos detienen la operación conservando evidencia en el error.
/// No promete dirsync ni durabilidad ante pérdida de energía.
pub fn recuperar_retiro(raiz: &Path) -> Result<(), ErrorRecuperacion> {
    let mut lock = LockEscritura::adquirir(raiz).map_err(|error| ErrorRecuperacion {
        codigo: error.codigo(),
        evidencia: None,
    })?;
    let mut owner =
        DuenioTransaccionAnclado::desde_lock(&mut lock).map_err(|_| ErrorRecuperacion {
            codigo: "recovery_owner_unverified",
            evidencia: None,
        })?;
    reanudar_retiro_v3(&mut owner)
}

pub(crate) fn reanudar_retiro_v3(
    owner: &mut DuenioTransaccionAnclado<'_>,
) -> Result<(), ErrorRecuperacion> {
    let mut ultimo = None;
    let mut evidencia = None;
    for _ in 0..3 {
        let resultado = owner.avanzar_retiro_v3().map_err(|_| ErrorRecuperacion {
            codigo: "recovery_retirement_unverified",
            evidencia: evidencia.take(),
        })?;
        let estado = resultado.estado();
        match estado {
            EstadoPasoRetiroV3::Finalizado | EstadoPasoRetiroV3::Ausente => return Ok(()),
            EstadoPasoRetiroV3::AvanzoB if ultimo.is_none() => ultimo = Some(estado),
            EstadoPasoRetiroV3::AvanzoC
                if ultimo.is_none() || ultimo == Some(EstadoPasoRetiroV3::AvanzoB) =>
            {
                ultimo = Some(estado)
            }
            EstadoPasoRetiroV3::Conflicto => {
                return Err(ErrorRecuperacion::desde_paso(
                    "recovery_retirement_conflict",
                    resultado,
                ));
            }
            _ => {
                return Err(ErrorRecuperacion::desde_paso(
                    "recovery_retirement_indeterminate",
                    resultado,
                ));
            }
        }
        evidencia = Some(Box::new(resultado.into_evidencia()));
    }
    Err(ErrorRecuperacion {
        codigo: "recovery_retirement_incomplete",
        evidencia,
    })
}
