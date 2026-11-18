//! Fachada de propuesta de solo lectura. No adquiere locks ni ejecuta recovery.
use super::replay::semantica_archivada::{POLICY_ID_V1, VERSION_MOTOR_V1};
use super::{captura::capturar_snapshot, snapshot::SnapshotToken};
use crate::parsear_delta_json;
use std::{fmt, path::Path};
use universo_core::{
    ErrorVeredicto, ModoExplicacion, ResultadoPropuesta, explicar_propuesta, proponer,
    veredicto::ResultadoExplicacionesMedido,
};

/// Presupuesto del documento delta UTF-8 antes del parseo; no es una cota RSS.
pub const MAX_DELTA_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EstadoSimulacion {
    Admitida,
    Rechazada,
}

/// Contadores públicos; no retiene el candidato ni sus valores privados.
#[derive(Debug)]
pub struct ResultadoSimulacion {
    pub estado: EstadoSimulacion,
    pub conflictos: usize,
    pub invariantes_fallidas: usize,
    token: SnapshotToken,
}
impl ResultadoSimulacion {
    /// Opt-in explícito. El token es sensible; Debug permanece redactado.
    pub fn snapshot_token(&self) -> SnapshotToken {
        self.token
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ErrorSimulacion {
    codigo: &'static str,
}
impl ErrorSimulacion {
    pub const fn codigo(self) -> &'static str {
        self.codigo
    }
}
impl fmt::Display for ErrorSimulacion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo)
    }
}
impl std::error::Error for ErrorSimulacion {}

/// Vista inmutable para adaptadores de consulta. No concede autoridad de escritura.
/// La captura conserva las mismas cuotas y política raw de la simulación CLI.
pub struct VistaSimulacion(super::captura::Snapshot);

impl VistaSimulacion {
    pub fn capturar(
        raiz: &Path,
        manifiesto: &Path,
        invariantes: &Path,
    ) -> Result<Self, ErrorSimulacion> {
        capturar_snapshot(
            raiz,
            manifiesto,
            invariantes,
            VERSION_MOTOR_V1,
            POLICY_ID_V1,
        )
        .map(Self)
        .map_err(|error| ErrorSimulacion {
            codigo: error.codigo(),
        })
    }

    pub fn estructura(&self) -> &universo_core::Estructura {
        self.0.estructura()
    }

    pub fn invariantes(&self) -> &[universo_core::SentenciaFuente] {
        self.0.fuentes_invariantes()
    }

    pub fn proponer(
        &self,
        delta: &universo_core::Delta,
    ) -> Result<ResultadoPropuesta, universo_core::ErrorPropuesta> {
        proponer(self.estructura(), delta, self.invariantes())
    }
}

/// Captura bytes inmutables, propone y descarta el candidato sin publicar.
/// La captura por ruta no promete una instantánea frente a escritores externos.
pub fn simular(
    raiz: &Path,
    manifiesto: &Path,
    invariantes: &Path,
    delta_json: &str,
) -> Result<ResultadoSimulacion, ErrorSimulacion> {
    simular_con(raiz, manifiesto, invariantes, delta_json, None).map(|(r, _)| r)
}

/// Explicación opt-in de una simulación, tomada de la misma captura que el
/// veredicto. `medido` es `None` si no hay invariantes que explicar (candidato
/// imposible o conflictos funcionales). `reglas[i]` son el id y el mensaje
/// declarados con `(regla ...)` para la fuente `source_index == i`.
pub struct ExplicacionSimulacion {
    pub medido: Option<Result<ResultadoExplicacionesMedido, ErrorVeredicto>>,
    pub reglas: Vec<(Option<String>, Option<String>)>,
}

fn simular_con(
    raiz: &Path,
    manifiesto: &Path,
    invariantes: &Path,
    delta_json: &str,
    explicar: Option<(ModoExplicacion, u64)>,
) -> Result<(ResultadoSimulacion, Option<ExplicacionSimulacion>), ErrorSimulacion> {
    if delta_json.len() > MAX_DELTA_BYTES {
        return Err(ErrorSimulacion {
            codigo: "delta_resource_limit",
        });
    }
    let delta = parsear_delta_json(delta_json).map_err(|_| ErrorSimulacion {
        codigo: "delta_invalid",
    })?;
    let snapshot = capturar_snapshot(
        raiz,
        manifiesto,
        invariantes,
        VERSION_MOTOR_V1,
        POLICY_ID_V1,
    )
    .map_err(|error| ErrorSimulacion {
        codigo: error.codigo(),
    })?;
    let propuesta = proponer(
        snapshot.estructura(),
        &delta,
        snapshot.fuentes_invariantes(),
    )
    .map_err(|error| ErrorSimulacion {
        codigo: error.codigo(),
    })?;
    let (estado, conflictos, invariantes_fallidas) = match propuesta {
        ResultadoPropuesta::Admitida(_) => (EstadoSimulacion::Admitida, 0, 0),
        ResultadoPropuesta::RechazadaConflictos(conflictos) => (
            EstadoSimulacion::Rechazada,
            conflictos.total_conflictos(),
            0,
        ),
        ResultadoPropuesta::RechazadaInvariantes(invariantes) => {
            (EstadoSimulacion::Rechazada, 0, invariantes.len())
        }
    };
    let explicacion = explicar.map(|(modo, limite)| ExplicacionSimulacion {
        medido: explicar_propuesta(
            snapshot.estructura(),
            &delta,
            snapshot.fuentes_invariantes(),
            modo,
            limite,
        ),
        reglas: snapshot
            .fuentes_invariantes()
            .iter()
            .map(|fuente| (fuente.id.clone(), fuente.mensaje.clone()))
            .collect(),
    });
    Ok((
        ResultadoSimulacion {
            estado,
            conflictos,
            invariantes_fallidas,
            token: snapshot.token(),
        },
        explicacion,
    ))
}

/// Como [`simular_archivo`], más la explicación redactada del candidato.
pub fn simular_archivo_explicado(
    raiz: &Path,
    manifiesto: &Path,
    invariantes: &Path,
    delta: &Path,
    modo: ModoExplicacion,
    limite: u64,
) -> Result<(ResultadoSimulacion, ExplicacionSimulacion), ErrorSimulacion> {
    let texto = leer_delta(delta)?;
    let (resultado, explicacion) =
        simular_con(raiz, manifiesto, invariantes, &texto, Some((modo, limite)))?;
    // `simular_con` siempre devuelve explicación cuando se le pide.
    let explicacion = explicacion.ok_or(ErrorSimulacion {
        codigo: "explanation_internal",
    })?;
    Ok((resultado, explicacion))
}

fn leer_delta(delta: &Path) -> Result<String, ErrorSimulacion> {
    let bytes = super::inventario::leer_archivo_regular_limitado(delta, MAX_DELTA_BYTES + 1)
        .map_err(|error| ErrorSimulacion {
            codigo: if error == super::snapshot::ErrorSnapshot::LimiteInventarioExcedido {
                "delta_resource_limit"
            } else {
                "delta_io"
            },
        })?;
    String::from_utf8(bytes).map_err(|_| ErrorSimulacion {
        codigo: "delta_invalid",
    })
}

/// Lee un delta regular acotado sin seguir su enlace final. En Unix abre
/// nonblocking para rechazar FIFO/dispositivos sin esperar un productor.
pub fn simular_archivo(
    raiz: &Path,
    manifiesto: &Path,
    invariantes: &Path,
    delta: &Path,
) -> Result<ResultadoSimulacion, ErrorSimulacion> {
    let texto = leer_delta(delta)?;
    simular(raiz, manifiesto, invariantes, &texto)
}
