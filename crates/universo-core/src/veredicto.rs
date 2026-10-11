//! Agregación de explicaciones para las instancias ya expandidas.
//!
//! Este módulo no determina el estado global (`admit`, `reject` o `error`):
//! deja esa decisión al llamador. Cada instancia falsa recibe evidencia
//! trazable. Los componentes explicativos adicionales se incorporarán en slices
//! posteriores cuando sus algoritmos estén disponibles.

use std::fmt;

use crate::estructura::Estructura;
use crate::evaluador::{
    ErrorEvaluacion, EvidenciaEvaluacion, ReglaEvidencia, ResultadoTrazable, TipoCuantificador,
    evaluar_trazable_con_presupuesto_total_medido,
};
use crate::formula::Formula;
use crate::invariante::expansion::ResultadoExpansionFuente;

mod presencia;
pub use presencia::CorePresencia;
use presencia::{ErrorPresencia, ResultadoPresencia, extraer_core_presencia};

/// Máximo predeterminado de explicaciones al enumerar.
pub const MAX_EXPLICACIONES_POR_DEFECTO: usize = 100;
/// Presupuesto determinista del recorrido, evaluación semántica, evidencia y
/// materialización de objetos.
pub const PRESUPUESTO_EXPLICACION_POR_DEFECTO: u64 = 1_000_000;

/// Política de selección de explicaciones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModoExplicacion {
    /// Conserva solo la primera instancia falsa en orden de expansión.
    Decidir,
    /// Enumera hasta `maximo_resultados`; detecta una más para marcar truncado.
    Enumerar { maximo_resultados: usize },
}

impl ModoExplicacion {
    /// Enumera con el máximo predeterminado de 100 objetos.
    pub const fn enumerar_por_defecto() -> Self {
        Self::Enumerar {
            maximo_resultados: MAX_EXPLICACIONES_POR_DEFECTO,
        }
    }
}

/// Procedencia de la instancia en el programa fuente y en la expansión.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcedenciaExplicacion {
    /// Índice 0-based de la sentencia de origen.
    pub source_index: usize,
    /// Posición 1-based de inicio, ausente si no había parser de fuente.
    pub linea: Option<usize>,
    pub columna: Option<usize>,
    /// Índice 0-based de la instancia dentro de su esquema; `None` para una
    /// oración directa.
    pub instance_index: Option<u64>,
}

impl From<&crate::invariante::expansion::OracionExpandida> for ProcedenciaExplicacion {
    fn from(oracion: &crate::invariante::expansion::OracionExpandida) -> Self {
        Self {
            source_index: oracion.source_index,
            linea: oracion.linea,
            columna: oracion.columna,
            instance_index: oracion.instance_index,
        }
    }
}

/// Evidencia trazable de una instancia cuya evaluación fue falsa.
#[derive(Clone, PartialEq, Eq)]
pub struct Contraejemplo {
    /// Árbol de evaluación del snapshot finito; sus tipos redaccionan Debug.
    pub evidencia: EvidenciaEvaluacion,
}

impl fmt::Debug for Contraejemplo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Contraejemplo")
            .field("evidencia", &"[redactada]")
            .finish()
    }
}

/// Un objeto por instancia falsa, en orden canónico de expansión.
#[derive(Clone, PartialEq, Eq)]
pub struct ObjetoExplicacion {
    pub procedencia: ProcedenciaExplicacion,
    pub counterexample: Contraejemplo,
    core: Option<CorePresencia>,
}

impl ObjetoExplicacion {
    /// Devuelve el core positivo cuando aplica el fragmento operacional.
    ///
    /// Los hechos solo se obtienen mediante el método explícitamente sensible
    /// [`CorePresencia::atomos_sensibles`]; no aparecen en `Debug` ni en
    /// `Display`.
    pub fn core_presencia(&self) -> Option<&CorePresencia> {
        self.core.as_ref()
    }
}

impl fmt::Debug for ObjetoExplicacion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ObjetoExplicacion")
            .field("procedencia", &self.procedencia)
            .field("counterexample", &"[redactado]")
            .field("tiene_core_presencia", &self.core.is_some())
            .finish()
    }
}

/// Motivo por el que una enumeración devuelve un prefijo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RazonTruncamiento {
    ResultLimit,
}

impl RazonTruncamiento {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ResultLimit => "result_limit",
        }
    }
}

/// Resultado de la política solicitada. No contiene un estado global.
#[derive(Clone, PartialEq, Eq)]
pub enum ResultadoExplicaciones {
    Decision(Option<ObjetoExplicacion>),
    Enumeracion {
        objetos: Vec<ObjetoExplicacion>,
        truncated: bool,
        reason: Option<RazonTruncamiento>,
    },
}

/// Resultado acompañado por el desglose del presupuesto global consumido.
#[derive(Clone, PartialEq, Eq)]
pub struct ResultadoExplicacionesMedido {
    pub resultado: ResultadoExplicaciones,
    pub unidades_semanticas: u64,
    pub unidades_evidencia: u64,
    /// Recorrido de candidatos y materialización de objetos.
    pub unidades_agregador: u64,
    pub unidades_consumidas: u64,
}

impl fmt::Debug for ResultadoExplicacionesMedido {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResultadoExplicacionesMedido")
            .field("resultado", &self.resultado)
            .field("unidades_semanticas", &self.unidades_semanticas)
            .field("unidades_evidencia", &self.unidades_evidencia)
            .field("unidades_agregador", &self.unidades_agregador)
            .field("unidades_consumidas", &self.unidades_consumidas)
            .finish()
    }
}

impl fmt::Debug for ResultadoExplicaciones {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decision(objeto) => f.debug_tuple("Decision").field(objeto).finish(),
            Self::Enumeracion {
                objetos,
                truncated,
                reason,
            } => f
                .debug_struct("Enumeracion")
                .field("cantidad_objetos", &objetos.len())
                .field("truncated", truncated)
                .field("reason", reason)
                .finish(),
        }
    }
}

/// Error al obtener explicaciones. Nunca lleva de vuelta los datos del bundle.
#[derive(Clone, PartialEq, Eq)]
pub enum ErrorVeredicto {
    Evaluacion(ErrorEvaluacion),
    PresupuestoAgotado { limite: u64 },
}

impl fmt::Debug for ErrorVeredicto {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Evaluacion(error) => f.debug_tuple("Evaluacion").field(error).finish(),
            Self::PresupuestoAgotado { limite } => f
                .debug_struct("PresupuestoAgotado")
                .field("limite", limite)
                .finish(),
        }
    }
}

impl fmt::Display for ErrorVeredicto {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Evaluacion(_) => f.write_str("no se pudo evaluar una instancia"),
            Self::PresupuestoAgotado { .. } => {
                f.write_str("se agotó el presupuesto de explicación")
            }
        }
    }
}

impl std::error::Error for ErrorVeredicto {}

impl From<ErrorEvaluacion> for ErrorVeredicto {
    fn from(error: ErrorEvaluacion) -> Self {
        Self::Evaluacion(error)
    }
}

/// Presupuesto global determinista para recorrido, evaluación semántica,
/// evidencia y materialización de objetos.
pub(super) struct PresupuestoExplicacion {
    limite: u64,
    restante: u64,
    unidades_semanticas: u64,
    unidades_evidencia: u64,
    unidades_agregador: u64,
}

#[derive(Clone, Copy)]
struct MetricasExplicacion {
    unidades_semanticas: u64,
    unidades_evidencia: u64,
    unidades_agregador: u64,
    unidades_consumidas: u64,
}

impl PresupuestoExplicacion {
    fn nuevo(limite: u64) -> Self {
        Self {
            limite,
            restante: limite,
            unidades_semanticas: 0,
            unidades_evidencia: 0,
            unidades_agregador: 0,
        }
    }

    pub(super) fn consumir(&mut self, unidades: u64) -> Result<(), u64> {
        let total_agregador = self
            .unidades_agregador
            .checked_add(unidades)
            .ok_or(self.limite)?;
        self.descontar(unidades)?;
        self.unidades_agregador = total_agregador;
        Ok(())
    }

    fn consumir_evaluacion(
        &mut self,
        unidades_semanticas: u64,
        unidades_evidencia: u64,
    ) -> Result<(), u64> {
        let total = unidades_semanticas
            .checked_add(unidades_evidencia)
            .ok_or(self.limite)?;
        let total_semantico = self
            .unidades_semanticas
            .checked_add(unidades_semanticas)
            .ok_or(self.limite)?;
        let total_evidencia = self
            .unidades_evidencia
            .checked_add(unidades_evidencia)
            .ok_or(self.limite)?;
        self.descontar(total)?;
        self.unidades_semanticas = total_semantico;
        self.unidades_evidencia = total_evidencia;
        Ok(())
    }

    fn descontar(&mut self, unidades: u64) -> Result<(), u64> {
        if self.restante < unidades {
            return Err(self.limite);
        }
        self.restante -= unidades;
        Ok(())
    }

    fn restante(&self) -> u64 {
        self.restante
    }

    fn metricas(&self) -> MetricasExplicacion {
        MetricasExplicacion {
            unidades_semanticas: self.unidades_semanticas,
            unidades_evidencia: self.unidades_evidencia,
            unidades_agregador: self.unidades_agregador,
            unidades_consumidas: self.limite - self.restante,
        }
    }
}

/// Obtiene explicaciones con el presupuesto predeterminado.
pub fn explicar(
    expansion: &ResultadoExpansionFuente,
    estructura: &Estructura,
    modo: ModoExplicacion,
) -> Result<ResultadoExplicaciones, ErrorVeredicto> {
    explicar_con_consumos(expansion, estructura, modo).map(|medido| medido.resultado)
}

/// Variante de [`explicar`] que devuelve consumo semántico, de evidencia y
/// overhead del agregador.
pub fn explicar_con_consumos(
    expansion: &ResultadoExpansionFuente,
    estructura: &Estructura,
    modo: ModoExplicacion,
) -> Result<ResultadoExplicacionesMedido, ErrorVeredicto> {
    explicar_con_presupuesto_y_consumos(
        expansion,
        estructura,
        modo,
        PRESUPUESTO_EXPLICACION_POR_DEFECTO,
    )
}

/// Variante con límite explícito para todo el trabajo de explicación.
/// Si se agota antes de completar el resultado solicitado, no devuelve una
/// lista parcial; el límite de resultados de `Enumerar` sí es truncamiento
/// deliberado y detecta una instancia falsa más allá del prefijo.
pub fn explicar_con_presupuesto(
    expansion: &ResultadoExpansionFuente,
    estructura: &Estructura,
    modo: ModoExplicacion,
    limite: u64,
) -> Result<ResultadoExplicaciones, ErrorVeredicto> {
    explicar_con_presupuesto_y_consumos(expansion, estructura, modo, limite)
        .map(|medido| medido.resultado)
}

/// Variante medida de [`explicar_con_presupuesto`]. El límite es global para
/// recorrido, cálculo semántico, evidencia, extracción de core y objetos. Para
/// cada traza, el evaluador recibe el presupuesto que aún queda; el consumo
/// real de sus dos contadores se descuenta antes de continuar.
pub fn explicar_con_presupuesto_y_consumos(
    expansion: &ResultadoExpansionFuente,
    estructura: &Estructura,
    modo: ModoExplicacion,
    limite: u64,
) -> Result<ResultadoExplicacionesMedido, ErrorVeredicto> {
    let mut presupuesto = PresupuestoExplicacion::nuevo(limite);
    match modo {
        ModoExplicacion::Decidir => {
            for oracion in &expansion.oraciones {
                consumir_presupuesto(&mut presupuesto)?;
                let traza = evaluar_traza(&oracion.formula, estructura, &mut presupuesto)?;
                if !traza.valor {
                    let core = extraer_core(
                        &oracion.formula,
                        &traza.evidencia,
                        estructura,
                        &mut presupuesto,
                    )?;
                    consumir_presupuesto(&mut presupuesto)?;
                    let objeto = clasificar_objeto(
                        ProcedenciaExplicacion::from(oracion),
                        traza.evidencia,
                        core,
                    );
                    return Ok(medido(
                        ResultadoExplicaciones::Decision(Some(objeto)),
                        &presupuesto,
                    ));
                }
            }
            Ok(medido(ResultadoExplicaciones::Decision(None), &presupuesto))
        }
        ModoExplicacion::Enumerar { maximo_resultados } => {
            let mut objetos = Vec::new();
            for oracion in &expansion.oraciones {
                consumir_presupuesto(&mut presupuesto)?;
                let traza = evaluar_traza(&oracion.formula, estructura, &mut presupuesto)?;
                if traza.valor {
                    continue;
                }
                if objetos.len() >= maximo_resultados {
                    return Ok(medido(
                        ResultadoExplicaciones::Enumeracion {
                            objetos,
                            truncated: true,
                            reason: Some(RazonTruncamiento::ResultLimit),
                        },
                        &presupuesto,
                    ));
                }
                let core = extraer_core(
                    &oracion.formula,
                    &traza.evidencia,
                    estructura,
                    &mut presupuesto,
                )?;
                consumir_presupuesto(&mut presupuesto)?;
                objetos.push(clasificar_objeto(
                    ProcedenciaExplicacion::from(oracion),
                    traza.evidencia,
                    core,
                ));
            }
            Ok(medido(
                ResultadoExplicaciones::Enumeracion {
                    objetos,
                    truncated: false,
                    reason: None,
                },
                &presupuesto,
            ))
        }
    }
}

fn evaluar_traza(
    formula: &Formula,
    estructura: &Estructura,
    presupuesto: &mut PresupuestoExplicacion,
) -> Result<ResultadoTrazable, ErrorVeredicto> {
    let limite_traza = presupuesto.restante();
    let resultado =
        evaluar_trazable_con_presupuesto_total_medido(formula, estructura, limite_traza).map_err(
            |medido| match medido.error {
                ErrorEvaluacion::PresupuestoAgotado { .. }
                | ErrorEvaluacion::PresupuestoEvidenciaAgotado { .. } => {
                    ErrorVeredicto::PresupuestoAgotado {
                        limite: presupuesto.limite,
                    }
                }
                error => ErrorVeredicto::Evaluacion(error),
            },
        )?;
    presupuesto
        .consumir_evaluacion(resultado.unidades_semanticas, resultado.unidades_evidencia)
        .map_err(|limite| ErrorVeredicto::PresupuestoAgotado { limite })?;
    Ok(resultado.resultado)
}

fn medido(
    resultado: ResultadoExplicaciones,
    presupuesto: &PresupuestoExplicacion,
) -> ResultadoExplicacionesMedido {
    let metricas = presupuesto.metricas();
    ResultadoExplicacionesMedido {
        resultado,
        unidades_semanticas: metricas.unidades_semanticas,
        unidades_evidencia: metricas.unidades_evidencia,
        unidades_agregador: metricas.unidades_agregador,
        unidades_consumidas: metricas.unidades_consumidas,
    }
}

fn consumir_presupuesto(presupuesto: &mut PresupuestoExplicacion) -> Result<(), ErrorVeredicto> {
    presupuesto
        .consumir(1)
        .map_err(|limite| ErrorVeredicto::PresupuestoAgotado { limite })
}

fn extraer_core(
    formula: &Formula,
    evidencia: &EvidenciaEvaluacion,
    estructura: &Estructura,
    presupuesto: &mut PresupuestoExplicacion,
) -> Result<Option<CorePresencia>, ErrorVeredicto> {
    let (
        Formula::ParaTodo { .. },
        ReglaEvidencia::Cuantificador {
            tipo: TipoCuantificador::ParaTodo,
            instancias,
            ..
        },
    ) = (formula, &evidencia.regla)
    else {
        return Ok(None);
    };

    for instancia in instancias {
        consumir_presupuesto(presupuesto)?;
        if !instancia.valor {
            return match extraer_core_presencia(formula, instancia, estructura, presupuesto)
                .map_err(|error| match error {
                    ErrorPresencia::PresupuestoAgotado { limite } => {
                        ErrorVeredicto::PresupuestoAgotado { limite }
                    }
                })? {
                ResultadoPresencia::Core(core) => Ok(Some(core)),
                ResultadoPresencia::NoAplicable => Ok(None),
            };
        }
    }
    Ok(None)
}

fn clasificar_objeto(
    procedencia: ProcedenciaExplicacion,
    evidencia: EvidenciaEvaluacion,
    core: Option<CorePresencia>,
) -> ObjetoExplicacion {
    ObjetoExplicacion {
        procedencia,
        counterexample: Contraejemplo { evidencia },
        core,
    }
}

#[cfg(test)]
#[path = "veredicto/tests.rs"]
mod tests;
