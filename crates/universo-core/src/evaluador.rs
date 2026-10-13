//! Evaluador de referencia de FO bisortida sobre una estructura finita.
//!
//! La implementación es intencionalmente directa: recorre los dominios
//! finitos de nodos y aristas, y evalúa cada conectivo según Tarski. No usa
//! los índices de átomos ni instala abreviaturas para `tc Parent`.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::atomos::IdArista;
use crate::estructura::Estructura;
use crate::formula::{
    Formula, SorteVar, SujetoTermino, TerminoArista, TerminoConstExt, TerminoEtiqueta, TerminoNodo,
    TerminoSorte, Var,
};
use crate::ids::{AristaId, NodoId};
use crate::path::BundlePath;
use crate::tipos::{Etiqueta, Sorte};

/// Máximo de nodos `Formula` en una ruta del AST; coincide con el límite del
/// parser de autoría, que admite 32 niveles y rechaza el 33.º.
pub const MAX_PROFUNDIDAD_AST: usize = 32;
/// Cantidad máxima de variables que puede ligar un cuantificador.
pub const MAX_LIGADURAS_POR_CUANTIFICADOR: usize = 64;
/// Presupuesto determinista predeterminado, medido en unidades de trabajo y
/// no en tiempo de pared.
pub const PRESUPUESTO_EVALUACION_POR_DEFECTO: u64 = 1_000_000;
/// Límite predeterminado para la API de evidencia: cuenta nodos/entradas y
/// bytes UTF-8 de cadenas copiadas. Es independiente del presupuesto semántico.
pub const PRESUPUESTO_EVIDENCIA_POR_DEFECTO: u64 = 1_000_000;

/// Constructor atómico reconocido por una evidencia de evaluación.
/// No contiene los términos ni valores de la fórmula.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstructorAtomico {
    Type,
    Edge,
    Parent,
    HasLabNodo,
    HasLabArista,
    Ext,
    TcParent,
    IgualNodo,
    IgualArista,
}

/// Identidad canónica de un valor que liga una variable en una instancia.
///
/// No implementa `Display`: la evidencia es un valor estructurado, no una
/// cadena de presentación que pueda revelar identificadores accidentalmente.
#[derive(Clone, PartialEq, Eq)]
pub enum ValorLigadura {
    Nodo(BundlePath),
    Arista(IdArista),
}

impl std::fmt::Debug for ValorLigadura {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Nodo(_) => f.write_str("Nodo([redactado])"),
            Self::Arista(_) => f.write_str("Arista([redactada])"),
        }
    }
}

/// Una ligadura bisortida capturada en orden de declaración.
#[derive(Clone, PartialEq, Eq)]
pub struct LigaduraCanonica {
    pub variable: Var,
    pub valor: ValorLigadura,
}

impl std::fmt::Debug for LigaduraCanonica {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LigaduraCanonica([redactada])")
    }
}

/// Asignación canónica local a una instancia cuantificada.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct AsignacionCanonica {
    pub ligaduras: Vec<LigaduraCanonica>,
}

impl std::fmt::Debug for AsignacionCanonica {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsignacionCanonica")
            .field("cantidad_ligaduras", &self.ligaduras.len())
            .finish()
    }
}

/// Tipo de cuantificador que aparece en la evidencia.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TipoCuantificador {
    ParaTodo,
    Existe,
}

/// Una instancia efectiva de un cuantificador.
///
/// `cuerpo` es `None` cuando la guarda fue falsa. Una instancia siempre
/// contiene la asignación de sus ligaduras, incluso si el cuerpo no aplica.
#[derive(Clone, PartialEq, Eq)]
pub struct InstanciaCuantificada {
    pub asignacion: AsignacionCanonica,
    pub guarda: Option<Box<EvidenciaEvaluacion>>,
    pub cuerpo: Option<Box<EvidenciaEvaluacion>>,
    pub valor: bool,
}

impl std::fmt::Debug for InstanciaCuantificada {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstanciaCuantificada")
            .field("valor", &self.valor)
            .field("cantidad_ligaduras", &self.asignacion.ligaduras.len())
            .field("tiene_guarda", &self.guarda.is_some())
            .field("tiene_cuerpo", &self.cuerpo.is_some())
            .finish()
    }
}

/// Regla local que justifica un valor booleano.
#[derive(Clone, PartialEq, Eq)]
pub enum ReglaEvidencia {
    Atomo {
        constructor: ConstructorAtomico,
        /// Ruta positiva (incluye inicio y fin) solo para un `tc Parent` verdadero.
        camino_tc: Option<Vec<BundlePath>>,
    },
    No {
        interior: Box<EvidenciaEvaluacion>,
    },
    Conjuncion {
        partes_evaluadas: Vec<EvidenciaEvaluacion>,
    },
    Disyuncion {
        partes_evaluadas: Vec<EvidenciaEvaluacion>,
    },
    Implicacion {
        antecedente: Box<EvidenciaEvaluacion>,
        consecuente: Option<Box<EvidenciaEvaluacion>>,
    },
    Cuantificador {
        tipo: TipoCuantificador,
        /// Instancias visitadas en el orden canónico del producto de dominios.
        instancias: Vec<InstanciaCuantificada>,
        /// `false` cuando la evaluación terminó por cortocircuito.
        busqueda_agotada: bool,
    },
}

impl std::fmt::Debug for ReglaEvidencia {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Atomo {
                constructor,
                camino_tc,
            } => f
                .debug_struct("Atomo")
                .field("constructor", constructor)
                .field("tiene_camino_tc", &camino_tc.is_some())
                .finish(),
            Self::No { .. } => f.write_str("No([subevidencia redactada])"),
            Self::Conjuncion { partes_evaluadas } => f
                .debug_struct("Conjuncion")
                .field("partes_evaluadas", &partes_evaluadas.len())
                .finish(),
            Self::Disyuncion { partes_evaluadas } => f
                .debug_struct("Disyuncion")
                .field("partes_evaluadas", &partes_evaluadas.len())
                .finish(),
            Self::Implicacion { consecuente, .. } => f
                .debug_struct("Implicacion")
                .field("consecuente_evaluado", &consecuente.is_some())
                .finish(),
            Self::Cuantificador {
                tipo,
                instancias,
                busqueda_agotada,
            } => f
                .debug_struct("Cuantificador")
                .field("tipo", tipo)
                .field("cantidad_instancias", &instancias.len())
                .field("busqueda_agotada", busqueda_agotada)
                .finish(),
        }
    }
}

/// Evidencia recursiva; no contiene el texto original de la fórmula.
#[derive(Clone, PartialEq, Eq)]
pub struct EvidenciaEvaluacion {
    pub valor: bool,
    pub regla: ReglaEvidencia,
}

impl std::fmt::Debug for EvidenciaEvaluacion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EvidenciaEvaluacion")
            .field("valor", &self.valor)
            .field("regla", &self.regla)
            .finish()
    }
}

/// Resultado de una decisión con evidencia reproducible.
#[derive(Clone, PartialEq, Eq)]
pub struct ResultadoTrazable {
    pub valor: bool,
    pub evidencia: EvidenciaEvaluacion,
}

impl std::fmt::Debug for ResultadoTrazable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResultadoTrazable")
            .field("valor", &self.valor)
            .field("evidencia", &"[redactada]")
            .finish()
    }
}

/// Resultado trazable acompañado por el consumo determinista real de ambos
/// presupuestos internos del evaluador.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultadoTrazableMedido {
    pub resultado: ResultadoTrazable,
    pub unidades_semanticas: u64,
    pub unidades_evidencia: u64,
}

/// Error al evaluar una fórmula que no está bien formada sobre la firma de
/// la estructura dada, o que no es una oración cerrada.
#[derive(Clone, PartialEq, Eq)]
pub enum ErrorEvaluacion {
    ProfundidadAstExcedida {
        maximo: usize,
    },
    DemasiadasLigaduras {
        cantidad: usize,
        maximo: usize,
    },
    PresupuestoAgotado {
        limite: u64,
    },
    PresupuestoEvidenciaAgotado {
        limite: u64,
    },
    VariableNoLigada {
        variable: Var,
        sorte: SorteVar,
    },
    VariableDeSorteIncorrecta {
        variable: Var,
        esperada: SorteVar,
    },
    VariableRedeclarada(Var),
    ConstanteNodoInexistente(BundlePath),
    ConstanteAristaInexistente(IdArista),
    SorteNodoNoDeclarada(Sorte),
    SorteAristaNoDeclarada(Sorte),
    EtiquetaNoEnUso(Etiqueta),
    PredicadoNoDeclarado(String),
    ConstanteFueraDeDominio {
        predicado: String,
        constante: String,
    },
    MetaNoInstanciada(Var),
    NodoLigadoInexistente(Var),
    AristaLigadaInexistente(Var),
}

impl std::fmt::Debug for ErrorEvaluacion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProfundidadAstExcedida { maximo } => f
                .debug_struct("ProfundidadAstExcedida")
                .field("maximo", maximo)
                .finish(),
            Self::DemasiadasLigaduras { cantidad, maximo } => f
                .debug_struct("DemasiadasLigaduras")
                .field("cantidad", cantidad)
                .field("maximo", maximo)
                .finish(),
            Self::PresupuestoAgotado { limite } => f
                .debug_struct("PresupuestoAgotado")
                .field("limite", limite)
                .finish(),
            Self::PresupuestoEvidenciaAgotado { limite } => f
                .debug_struct("PresupuestoEvidenciaAgotado")
                .field("limite", limite)
                .finish(),
            Self::VariableNoLigada { sorte, .. } => f
                .debug_tuple("VariableNoLigada")
                .field(sorte)
                .field(&"[redactada]")
                .finish(),
            Self::VariableDeSorteIncorrecta { esperada, .. } => f
                .debug_tuple("VariableDeSorteIncorrecta")
                .field(esperada)
                .field(&"[redactada]")
                .finish(),
            Self::VariableRedeclarada(_) => f.write_str("VariableRedeclarada([redactada])"),
            Self::ConstanteNodoInexistente(_) => {
                f.write_str("ConstanteNodoInexistente([redactada])")
            }
            Self::ConstanteAristaInexistente(_) => {
                f.write_str("ConstanteAristaInexistente([redactada])")
            }
            Self::SorteNodoNoDeclarada(_) => f.write_str("SorteNodoNoDeclarada([redactada])"),
            Self::SorteAristaNoDeclarada(_) => f.write_str("SorteAristaNoDeclarada([redactada])"),
            Self::EtiquetaNoEnUso(_) => f.write_str("EtiquetaNoEnUso([redactada])"),
            Self::PredicadoNoDeclarado(_) => f.write_str("PredicadoNoDeclarado([redactado])"),
            Self::ConstanteFueraDeDominio { .. } => {
                f.write_str("ConstanteFueraDeDominio([redactada])")
            }
            Self::MetaNoInstanciada(_) => f.write_str("MetaNoInstanciada([redactada])"),
            Self::NodoLigadoInexistente(_) => f.write_str("NodoLigadoInexistente([redactada])"),
            Self::AristaLigadaInexistente(_) => f.write_str("AristaLigadaInexistente([redactada])"),
        }
    }
}

impl std::fmt::Display for ErrorEvaluacion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProfundidadAstExcedida { .. } => {
                f.write_str("la fórmula excede un límite estructural del evaluador")
            }
            Self::DemasiadasLigaduras { .. } => {
                f.write_str("un cuantificador excede un límite estructural del evaluador")
            }
            Self::PresupuestoAgotado { .. } => f.write_str("se agotó el presupuesto de evaluación"),
            Self::PresupuestoEvidenciaAgotado { .. } => {
                f.write_str("se agotó el presupuesto de evidencia")
            }
            Self::VariableNoLigada { .. } => {
                f.write_str("la fórmula contiene una variable no ligada")
            }
            Self::VariableDeSorteIncorrecta { .. } => {
                f.write_str("la fórmula usa una variable en una posición incompatible")
            }
            Self::VariableRedeclarada(_) => {
                f.write_str("la fórmula contiene una variable redeclarada")
            }
            Self::ConstanteNodoInexistente(_) => {
                f.write_str("la fórmula refiere a un nodo inexistente")
            }
            Self::ConstanteAristaInexistente(_) => {
                f.write_str("la fórmula refiere a una arista inexistente")
            }
            Self::SorteNodoNoDeclarada(_) | Self::SorteAristaNoDeclarada(_) => {
                f.write_str("la fórmula refiere a una sorte no declarada")
            }
            Self::EtiquetaNoEnUso(_) => {
                f.write_str("la fórmula refiere a una etiqueta no disponible")
            }
            Self::PredicadoNoDeclarado(_) => {
                f.write_str("la fórmula refiere a un predicado no declarado")
            }
            Self::ConstanteFueraDeDominio { .. } => {
                f.write_str("la fórmula usa un valor fuera de un dominio cerrado")
            }
            Self::MetaNoInstanciada(_) => {
                f.write_str("la fórmula contiene una metavariable sin instanciar")
            }
            Self::NodoLigadoInexistente(_) => {
                f.write_str("una asignación refiere a un nodo inexistente")
            }
            Self::AristaLigadaInexistente(_) => {
                f.write_str("una asignación refiere a una arista inexistente")
            }
        }
    }
}

impl std::error::Error for ErrorEvaluacion {}

/// Error de la API de trazabilidad con presupuesto global y consumo parcial
/// observado antes de detenerse. El total semántico más evidencial nunca
/// supera el límite de la llamada; el árbol de evidencia fallido no se devuelve.
#[derive(Clone, PartialEq, Eq)]
pub struct ErrorEvaluacionMedida {
    pub error: ErrorEvaluacion,
    pub unidades_semanticas: u64,
    pub unidades_evidencia: u64,
    pub unidades_consumidas: u64,
}

impl std::fmt::Debug for ErrorEvaluacionMedida {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ErrorEvaluacionMedida")
            .field("error", &self.error)
            .field("unidades_semanticas", &self.unidades_semanticas)
            .field("unidades_evidencia", &self.unidades_evidencia)
            .field("unidades_consumidas", &self.unidades_consumidas)
            .finish()
    }
}

impl std::fmt::Display for ErrorEvaluacionMedida {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}

impl std::error::Error for ErrorEvaluacionMedida {}

#[derive(Clone, Copy)]
struct ContadorGlobal {
    restante: u64,
    unidades_semanticas: u64,
    unidades_evidencia: u64,
}

impl ContadorGlobal {
    fn nuevo(limite: u64) -> Self {
        Self {
            restante: limite,
            unidades_semanticas: 0,
            unidades_evidencia: 0,
        }
    }

    fn consumir(&mut self, cantidad: u64, semantica: bool) -> bool {
        if self.restante < cantidad {
            return false;
        }
        let destino = if semantica {
            &mut self.unidades_semanticas
        } else {
            &mut self.unidades_evidencia
        };
        let Some(consumidas) = destino.checked_add(cantidad) else {
            return false;
        };
        self.restante -= cantidad;
        *destino = consumidas;
        true
    }
}

struct Presupuesto {
    limite: u64,
    restante: u64,
    total_compartido: Option<Rc<Cell<ContadorGlobal>>>,
}

impl Presupuesto {
    fn con_total(limite: u64, total_compartido: Option<Rc<Cell<ContadorGlobal>>>) -> Self {
        Self {
            limite,
            restante: limite,
            total_compartido,
        }
    }

    fn consumir(&mut self, cantidad: u64) -> Result<(), ErrorEvaluacion> {
        if self.restante < cantidad {
            return Err(ErrorEvaluacion::PresupuestoAgotado {
                limite: self.limite,
            });
        }
        if let Some(total) = &self.total_compartido {
            let mut contador = total.get();
            if !contador.consumir(cantidad, true) {
                return Err(ErrorEvaluacion::PresupuestoAgotado {
                    limite: self.limite,
                });
            }
            total.set(contador);
        }
        self.restante -= cantidad;
        Ok(())
    }

    fn consumidas(&self) -> u64 {
        self.limite - self.restante
    }
}

/// Contexto interno para compartir el presupuesto semántico entre varias
/// fórmulas. Cada llamada conserva el mismo preflight y la misma evaluación
/// que `evaluar_con_presupuesto`; el contador común cobra ambos recorridos.
pub(crate) struct ContextoPresupuestoEvaluacion {
    limite: u64,
    total_compartido: Rc<Cell<ContadorGlobal>>,
}

impl ContextoPresupuestoEvaluacion {
    pub(crate) fn nuevo(limite: u64) -> Self {
        Self {
            limite,
            total_compartido: Rc::new(Cell::new(ContadorGlobal::nuevo(limite))),
        }
    }

    pub(crate) fn evaluar(
        &mut self,
        formula: &Formula,
        estructura: &Estructura,
    ) -> Result<bool, ErrorEvaluacion> {
        let mut presupuesto = preparar_evaluacion_con_total(
            formula,
            estructura,
            self.limite,
            Some(self.total_compartido.clone()),
        )?;
        let mut modo_evidencia = ModoEvidencia::Omitir;
        evaluar_interno(
            formula,
            estructura,
            &mut Entorno::default(),
            &mut presupuesto,
            &mut modo_evidencia,
        )
        .map(|resultado| resultado.valor)
    }
}

struct PresupuestoEvidencia {
    limite: u64,
    restante: u64,
    total_compartido: Option<Rc<Cell<ContadorGlobal>>>,
}

impl PresupuestoEvidencia {
    fn con_total(limite: u64, total_compartido: Option<Rc<Cell<ContadorGlobal>>>) -> Self {
        Self {
            limite,
            restante: limite,
            total_compartido,
        }
    }

    fn consumir(&mut self, cantidad: u64) -> Result<(), ErrorEvaluacion> {
        if self.restante < cantidad {
            return Err(ErrorEvaluacion::PresupuestoEvidenciaAgotado {
                limite: self.limite,
            });
        }
        if let Some(total) = &self.total_compartido {
            let mut contador = total.get();
            if !contador.consumir(cantidad, false) {
                return Err(ErrorEvaluacion::PresupuestoEvidenciaAgotado {
                    limite: self.limite,
                });
            }
            total.set(contador);
        }
        self.restante -= cantidad;
        Ok(())
    }

    fn consumidas(&self) -> u64 {
        self.limite - self.restante
    }
}

enum ModoEvidencia {
    Omitir,
    Capturar(PresupuestoEvidencia),
}

impl ModoEvidencia {
    fn capturando(&self) -> bool {
        matches!(self, Self::Capturar(_))
    }

    fn consumir(&mut self, cantidad: u64) -> Result<(), ErrorEvaluacion> {
        if let Self::Capturar(presupuesto) = self {
            presupuesto.consumir(cantidad)?;
        }
        Ok(())
    }

    fn consumir_bytes(&mut self, bytes: usize) -> Result<(), ErrorEvaluacion> {
        self.consumir(u64::try_from(bytes).unwrap_or(u64::MAX))
    }

    fn consumidas(&self) -> u64 {
        match self {
            Self::Omitir => 0,
            Self::Capturar(presupuesto) => presupuesto.consumidas(),
        }
    }
}

fn preparar_evaluacion(
    formula: &Formula,
    estructura: &Estructura,
    limite: u64,
) -> Result<Presupuesto, ErrorEvaluacion> {
    preparar_evaluacion_con_total(formula, estructura, limite, None)
}

fn preparar_evaluacion_con_total(
    formula: &Formula,
    estructura: &Estructura,
    limite: u64,
    total_compartido: Option<Rc<Cell<ContadorGlobal>>>,
) -> Result<Presupuesto, ErrorEvaluacion> {
    let mut presupuesto = Presupuesto::con_total(limite, total_compartido);
    validar_formula(
        formula,
        estructura,
        &mut Alcance::default(),
        &mut presupuesto,
        0,
    )?;
    Ok(presupuesto)
}

/// Evalúa una fórmula cerrada en la estructura finita dada.
///
/// Primero comprueba toda la fórmula contra la firma y la estructura, incluso
/// las ramas que la evaluación booleana pudiera cortocircuitar. Así, una
/// constante desconocida no se convierte en `false` ni queda oculta por una
/// rama no visitada. Después aplica la semántica clásica de Tarski, con
/// [`PRESUPUESTO_EVALUACION_POR_DEFECTO`] unidades deterministas de trabajo.
///
/// Para un AST no confiable o construido directamente, antes de evaluar el
/// llamador debe validar la oración
/// `Sentencia::Oracion(formula.to_owned())` mediante
/// [`crate::invariante::validar_sentencia`], pasando `estructura.manifiesto()`
/// y `estructura`, y proceder solo si el vector de diagnósticos está vacío.
/// Aquí `estructura` ya es `&Estructura`; si el llamador tiene un valor propio,
/// debe pasar `&estructura`. Este evaluador no sustituye la validación estática
/// de autoría ni expande esquemas.
pub fn evaluar(formula: &Formula, estructura: &Estructura) -> Result<bool, ErrorEvaluacion> {
    evaluar_con_presupuesto(formula, estructura, PRESUPUESTO_EVALUACION_POR_DEFECTO)
}

/// Evalúa una fórmula cerrada con un presupuesto explícito de unidades de
/// trabajo. Se consume al visitar nodos del AST durante el preflight y la
/// evaluación, asignaciones de ligaduras, elementos de dominios inspeccionados,
/// coste estimado de ordenarlos y pasos de `tc Parent`. El resultado no depende
/// del tiempo de pared; si el límite no alcanza, devuelve
/// [`ErrorEvaluacion::PresupuestoAgotado`].
///
/// Para un AST no confiable o construido directamente, antes de evaluar el
/// llamador debe validar la oración
/// `Sentencia::Oracion(formula.to_owned())` mediante
/// [`crate::invariante::validar_sentencia`], pasando `estructura.manifiesto()`
/// y `estructura`, y proceder solo si el vector de diagnósticos está vacío.
/// Aquí `estructura` ya es `&Estructura`; si el llamador tiene un valor propio,
/// debe pasar `&estructura`. Este evaluador no sustituye la validación estática
/// de autoría ni expande esquemas.
pub fn evaluar_con_presupuesto(
    formula: &Formula,
    estructura: &Estructura,
    limite: u64,
) -> Result<bool, ErrorEvaluacion> {
    let mut presupuesto = preparar_evaluacion(formula, estructura, limite)?;
    let mut modo_evidencia = ModoEvidencia::Omitir;
    evaluar_interno(
        formula,
        estructura,
        &mut Entorno::default(),
        &mut presupuesto,
        &mut modo_evidencia,
    )
    .map(|resultado| resultado.valor)
}

/// Evalúa una fórmula cerrada y conserva el árbol de decisiones visitado.
/// Las asignaciones se expresan con rutas de nodo e identidades canónicas de
/// arista, en el orden de ligaduras; los cuantificadores anidados conservan su
/// propio ámbito y no se aplanan en un mapa ambiguo. Para `tc Parent` verdadero
/// incluye la ruta positiva recorrida. No contiene el texto de la fórmula ni
/// implementa `Display` para las asignaciones.
///
/// El cálculo semántico, el preflight y el presupuesto son los mismos que en
/// [`evaluar`]. Si la evaluación falla o agota presupuesto, no se devuelve
/// evidencia parcial.
/// Para un AST no confiable, primero compruebe que
/// `crate::invariante::validar_sentencia(&Sentencia::Oracion(formula.clone()),
/// estructura.manifiesto(), estructura)` devuelve un vector vacío; esta API no
/// sustituye la validación estática ni expande esquemas.
pub fn evaluar_trazable(
    formula: &Formula,
    estructura: &Estructura,
) -> Result<ResultadoTrazable, ErrorEvaluacion> {
    evaluar_trazable_con_presupuestos(
        formula,
        estructura,
        PRESUPUESTO_EVALUACION_POR_DEFECTO,
        PRESUPUESTO_EVIDENCIA_POR_DEFECTO,
    )
}

/// Variante de [`evaluar_trazable`] con presupuesto explícito para el cálculo.
/// La materialización de evidencia conserva el límite predeterminado de
/// [`PRESUPUESTO_EVIDENCIA_POR_DEFECTO`].
/// Para un AST no confiable o construido directamente, aplique antes
/// [`crate::invariante::validar_sentencia`] con la sentencia, el manifiesto y
/// la estructura, igual que para [`evaluar`].
pub fn evaluar_trazable_con_presupuesto(
    formula: &Formula,
    estructura: &Estructura,
    limite: u64,
) -> Result<ResultadoTrazable, ErrorEvaluacion> {
    evaluar_trazable_con_presupuestos(
        formula,
        estructura,
        limite,
        PRESUPUESTO_EVIDENCIA_POR_DEFECTO,
    )
}

/// Evalúa con presupuestos independientes para cómputo semántico y evidencia.
/// El segundo cuenta nodos/entradas y bytes UTF-8 de las cadenas copiadas a la
/// evidencia; al agotarse devuelve
/// [`ErrorEvaluacion::PresupuestoEvidenciaAgotado`] y descarta el árbol parcial
/// sin alterar el límite de evaluación.
pub fn evaluar_trazable_con_presupuestos(
    formula: &Formula,
    estructura: &Estructura,
    limite_evaluacion: u64,
    limite_evidencia: u64,
) -> Result<ResultadoTrazable, ErrorEvaluacion> {
    evaluar_trazable_con_presupuestos_medidos(
        formula,
        estructura,
        limite_evaluacion,
        limite_evidencia,
    )
    .map(|resultado| resultado.resultado)
}

/// Variante medida de [`evaluar_trazable_con_presupuestos`].
///
/// Devuelve las unidades efectivamente consumidas por el preflight/evaluación
/// semántica y por la materialización de evidencia. Los presupuestos son
/// independientes y cada contador corresponde a su categoría, incluso si el
/// resultado semántico es `true`.
pub fn evaluar_trazable_con_presupuestos_medidos(
    formula: &Formula,
    estructura: &Estructura,
    limite_evaluacion: u64,
    limite_evidencia: u64,
) -> Result<ResultadoTrazableMedido, ErrorEvaluacion> {
    evaluar_trazable_medido_interno(
        formula,
        estructura,
        limite_evaluacion,
        limite_evidencia,
        None,
    )
}

/// Evalúa y mide con una única bolsa compartida por preflight, semántica y
/// materialización de evidencia. Cada categoría la descuenta en tiempo real,
/// así que su consumo combinado nunca supera `limite_total`; si se agota, el
/// error informa cuánto se consumió y descarta cualquier evidencia parcial.
pub fn evaluar_trazable_con_presupuesto_total_medido(
    formula: &Formula,
    estructura: &Estructura,
    limite_total: u64,
) -> Result<ResultadoTrazableMedido, ErrorEvaluacionMedida> {
    let total_compartido = Rc::new(Cell::new(ContadorGlobal::nuevo(limite_total)));
    match evaluar_trazable_medido_interno(
        formula,
        estructura,
        limite_total,
        limite_total,
        Some(total_compartido.clone()),
    ) {
        Ok(resultado) => Ok(resultado),
        Err(error) => {
            let contador = total_compartido.get();
            Err(ErrorEvaluacionMedida {
                error,
                unidades_semanticas: contador.unidades_semanticas,
                unidades_evidencia: contador.unidades_evidencia,
                unidades_consumidas: limite_total - contador.restante,
            })
        }
    }
}

fn evaluar_trazable_medido_interno(
    formula: &Formula,
    estructura: &Estructura,
    limite_evaluacion: u64,
    limite_evidencia: u64,
    total_compartido: Option<Rc<Cell<ContadorGlobal>>>,
) -> Result<ResultadoTrazableMedido, ErrorEvaluacion> {
    let mut presupuesto = preparar_evaluacion_con_total(
        formula,
        estructura,
        limite_evaluacion,
        total_compartido.clone(),
    )?;
    let mut modo_evidencia = ModoEvidencia::Capturar(PresupuestoEvidencia::con_total(
        limite_evidencia,
        total_compartido,
    ));
    // La envoltura pública también cuenta como una entrada y se cobra antes
    // de materializar el árbol que se devolvería dentro de ella.
    modo_evidencia.consumir(1)?;
    let resultado = evaluar_interno(
        formula,
        estructura,
        &mut Entorno::default(),
        &mut presupuesto,
        &mut modo_evidencia,
    )?;
    let evidencia = resultado
        .evidencia
        .expect("el modo trazable construye evidencia para toda fórmula visitada");
    Ok(ResultadoTrazableMedido {
        resultado: ResultadoTrazable {
            valor: resultado.valor,
            evidencia,
        },
        unidades_semanticas: presupuesto.consumidas(),
        unidades_evidencia: modo_evidencia.consumidas(),
    })
}

#[derive(Default)]
struct Alcance {
    activas: HashMap<Var, SorteVar>,
    declaradas: HashMap<Var, SorteVar>,
}

fn validar_formula(
    formula: &Formula,
    estructura: &Estructura,
    alcance: &mut Alcance,
    presupuesto: &mut Presupuesto,
    profundidad: usize,
) -> Result<(), ErrorEvaluacion> {
    presupuesto.consumir(1)?;
    if profundidad >= MAX_PROFUNDIDAD_AST {
        return Err(ErrorEvaluacion::ProfundidadAstExcedida {
            maximo: MAX_PROFUNDIDAD_AST,
        });
    }
    match formula {
        Formula::Type { sujeto, sorte } => {
            validar_nodo(sujeto, estructura, alcance)?;
            validar_sorte(sorte, estructura, SorteVar::Nodo)
        }
        Formula::Edge {
            arista,
            origen,
            destino,
            sorte,
        } => {
            validar_arista(arista, estructura, alcance, presupuesto)?;
            validar_nodo(origen, estructura, alcance)?;
            validar_nodo(destino, estructura, alcance)?;
            validar_sorte(sorte, estructura, SorteVar::Arista)
        }
        Formula::Parent { hijo, padre } => {
            validar_nodo(hijo, estructura, alcance)?;
            validar_nodo(padre, estructura, alcance)
        }
        Formula::HasLab { sujeto, etiqueta } => {
            match sujeto {
                SujetoTermino::Nodo(termino) => validar_nodo(termino, estructura, alcance)?,
                SujetoTermino::Arista(termino) => {
                    validar_arista(termino, estructura, alcance, presupuesto)?
                }
            }
            validar_etiqueta(etiqueta, estructura, presupuesto)
        }
        Formula::Ext {
            predicado,
            sujeto,
            constante,
        } => {
            validar_nodo(sujeto, estructura, alcance)?;
            let declaracion = estructura
                .manifiesto()
                .predicado(predicado)
                .ok_or_else(|| ErrorEvaluacion::PredicadoNoDeclarado(predicado.clone()))?;
            match constante {
                TerminoConstExt::Literal(valor) => {
                    if declaracion.dominio.contains(valor) {
                        Ok(())
                    } else {
                        Err(ErrorEvaluacion::ConstanteFueraDeDominio {
                            predicado: predicado.clone(),
                            constante: valor.clone(),
                        })
                    }
                }
                TerminoConstExt::Meta(variable) => {
                    Err(ErrorEvaluacion::MetaNoInstanciada(variable.clone()))
                }
            }
        }
        Formula::TcParent { desde, hasta } => {
            validar_nodo(desde, estructura, alcance)?;
            validar_nodo(hasta, estructura, alcance)
        }
        Formula::IgualNodo(a, b) => {
            validar_nodo(a, estructura, alcance)?;
            validar_nodo(b, estructura, alcance)
        }
        Formula::IgualArista(a, b) => {
            validar_arista(a, estructura, alcance, presupuesto)?;
            validar_arista(b, estructura, alcance, presupuesto)
        }
        Formula::No(interior) => {
            validar_formula(interior, estructura, alcance, presupuesto, profundidad + 1)
        }
        Formula::Y(partes) | Formula::O(partes) => {
            for parte in partes {
                validar_formula(parte, estructura, alcance, presupuesto, profundidad + 1)?;
            }
            Ok(())
        }
        Formula::Implica(antecedente, consecuente) => {
            validar_formula(
                antecedente,
                estructura,
                alcance,
                presupuesto,
                profundidad + 1,
            )?;
            validar_formula(
                consecuente,
                estructura,
                alcance,
                presupuesto,
                profundidad + 1,
            )
        }
        Formula::ParaTodo {
            ligaduras,
            guarda,
            cuerpo,
        }
        | Formula::Existe {
            ligaduras,
            guarda,
            cuerpo,
        } => {
            if ligaduras.len() > MAX_LIGADURAS_POR_CUANTIFICADOR {
                return Err(ErrorEvaluacion::DemasiadasLigaduras {
                    cantidad: ligaduras.len(),
                    maximo: MAX_LIGADURAS_POR_CUANTIFICADOR,
                });
            }
            let anteriores = alcance.activas.clone();
            let mut locales = HashSet::new();
            for (variable, sorte) in ligaduras {
                presupuesto.consumir(1)?;
                if !locales.insert(variable.clone()) || alcance.activas.contains_key(variable) {
                    return Err(ErrorEvaluacion::VariableRedeclarada(variable.clone()));
                }
                if alcance
                    .declaradas
                    .get(variable)
                    .is_some_and(|declarada| declarada != sorte)
                {
                    return Err(ErrorEvaluacion::VariableRedeclarada(variable.clone()));
                }
                alcance.declaradas.entry(variable.clone()).or_insert(*sorte);
                alcance.activas.insert(variable.clone(), *sorte);
            }
            let resultado = (|| {
                if let Some(guarda) = guarda {
                    validar_formula(guarda, estructura, alcance, presupuesto, profundidad + 1)?;
                }
                validar_formula(cuerpo, estructura, alcance, presupuesto, profundidad + 1)
            })();
            alcance.activas = anteriores;
            resultado
        }
    }
}

fn validar_nodo(
    termino: &TerminoNodo,
    estructura: &Estructura,
    alcance: &Alcance,
) -> Result<(), ErrorEvaluacion> {
    match termino {
        TerminoNodo::Var(variable) => validar_variable(variable, SorteVar::Nodo, alcance),
        TerminoNodo::Constante(path) => estructura
            .por_path(path)
            .map(|_| ())
            .ok_or_else(|| ErrorEvaluacion::ConstanteNodoInexistente(path.clone())),
    }
}

fn validar_arista(
    termino: &TerminoArista,
    estructura: &Estructura,
    alcance: &Alcance,
    presupuesto: &mut Presupuesto,
) -> Result<(), ErrorEvaluacion> {
    match termino {
        TerminoArista::Var(variable) => validar_variable(variable, SorteVar::Arista, alcance),
        TerminoArista::Constante(id) => {
            if encontrar_arista(estructura, id, presupuesto)?.is_some() {
                Ok(())
            } else {
                Err(ErrorEvaluacion::ConstanteAristaInexistente(id.clone()))
            }
        }
    }
}

fn validar_variable(
    variable: &Var,
    esperada: SorteVar,
    alcance: &Alcance,
) -> Result<(), ErrorEvaluacion> {
    match alcance.activas.get(variable) {
        Some(encontrada) if *encontrada == esperada => Ok(()),
        Some(_) => Err(ErrorEvaluacion::VariableDeSorteIncorrecta {
            variable: variable.clone(),
            esperada,
        }),
        None => Err(ErrorEvaluacion::VariableNoLigada {
            variable: variable.clone(),
            sorte: esperada,
        }),
    }
}

fn validar_sorte(
    termino: &TerminoSorte,
    estructura: &Estructura,
    contexto: SorteVar,
) -> Result<(), ErrorEvaluacion> {
    match termino {
        TerminoSorte::Meta(variable) => Err(ErrorEvaluacion::MetaNoInstanciada(variable.clone())),
        TerminoSorte::Literal(sorte) => match contexto {
            SorteVar::Nodo if estructura.manifiesto().es_sorte_nodo(sorte) => Ok(()),
            SorteVar::Nodo => Err(ErrorEvaluacion::SorteNodoNoDeclarada(sorte.clone())),
            SorteVar::Arista if estructura.manifiesto().es_sorte_arista(sorte.como_str()) => Ok(()),
            SorteVar::Arista => Err(ErrorEvaluacion::SorteAristaNoDeclarada(sorte.clone())),
        },
    }
}

fn validar_etiqueta(
    termino: &TerminoEtiqueta,
    estructura: &Estructura,
    presupuesto: &mut Presupuesto,
) -> Result<(), ErrorEvaluacion> {
    match termino {
        TerminoEtiqueta::Meta(variable) => {
            Err(ErrorEvaluacion::MetaNoInstanciada(variable.clone()))
        }
        TerminoEtiqueta::Literal(etiqueta) => {
            for (_, nodo) in estructura.nodos() {
                presupuesto.consumir(1)?;
                if nodo.etiqueta == *etiqueta {
                    return Ok(());
                }
            }
            for (_, arista) in estructura.aristas() {
                presupuesto.consumir(1)?;
                if arista.etiqueta == *etiqueta {
                    return Ok(());
                }
            }
            Err(ErrorEvaluacion::EtiquetaNoEnUso(etiqueta.clone()))
        }
    }
}

#[derive(Default)]
struct Entorno {
    nodos: HashMap<Var, NodoId>,
    aristas: HashMap<Var, AristaId>,
}

struct ModoCuantificador<'a> {
    universal: bool,
    evidencia: &'a mut ModoEvidencia,
}

struct EvaluacionInterna {
    valor: bool,
    evidencia: Option<EvidenciaEvaluacion>,
}

fn resultado_interno(
    valor: bool,
    modo: &mut ModoEvidencia,
    regla: impl FnOnce(&mut ModoEvidencia) -> Result<ReglaEvidencia, ErrorEvaluacion>,
) -> Result<EvaluacionInterna, ErrorEvaluacion> {
    if !modo.capturando() {
        return Ok(EvaluacionInterna {
            valor,
            evidencia: None,
        });
    }
    modo.consumir(1)?;
    let regla = regla(modo)?;
    Ok(EvaluacionInterna {
        valor,
        evidencia: Some(EvidenciaEvaluacion { valor, regla }),
    })
}

fn tomar_evidencia(
    evaluacion: EvaluacionInterna,
    modo: &mut ModoEvidencia,
    destino: &mut Vec<EvidenciaEvaluacion>,
) -> Result<(), ErrorEvaluacion> {
    if modo.capturando() {
        modo.consumir(1)?;
        destino.push(
            evaluacion
                .evidencia
                .expect("el modo trazable conserva evidencia de cada hijo evaluado"),
        );
    }
    Ok(())
}

fn encajar_evidencia(
    evidencia: EvidenciaEvaluacion,
    modo: &mut ModoEvidencia,
) -> Result<Box<EvidenciaEvaluacion>, ErrorEvaluacion> {
    modo.consumir(1)?;
    Ok(Box::new(evidencia))
}

fn evaluar_interno(
    formula: &Formula,
    estructura: &Estructura,
    entorno: &mut Entorno,
    presupuesto: &mut Presupuesto,
    modo: &mut ModoEvidencia,
) -> Result<EvaluacionInterna, ErrorEvaluacion> {
    presupuesto.consumir(1)?;
    match formula {
        Formula::Type { sujeto, sorte } => {
            let id = resolver_nodo(sujeto, estructura, entorno)?;
            let esperado = resolver_sorte(sorte)?;
            let nodo = estructura.nodo(id).ok_or_else(|| {
                ErrorEvaluacion::NodoLigadoInexistente(nombre_variable_nodo(sujeto))
            })?;
            let valor = nodo.tipo == esperado;
            resultado_interno(valor, modo, |_| {
                Ok({
                    ReglaEvidencia::Atomo {
                        constructor: ConstructorAtomico::Type,
                        camino_tc: None,
                    }
                })
            })
        }
        Formula::Edge {
            arista,
            origen,
            destino,
            sorte,
        } => {
            let id = resolver_arista(arista, estructura, entorno, presupuesto)?;
            let origen = resolver_nodo(origen, estructura, entorno)?;
            let destino = resolver_nodo(destino, estructura, entorno)?;
            let sorte = resolver_sorte(sorte)?;
            let arista = estructura.arista(id).ok_or_else(|| {
                ErrorEvaluacion::AristaLigadaInexistente(nombre_variable_arista(arista))
            })?;
            let valor =
                arista.origen == origen && arista.destino == destino && arista.tipo == sorte;
            resultado_interno(valor, modo, |_| {
                Ok({
                    ReglaEvidencia::Atomo {
                        constructor: ConstructorAtomico::Edge,
                        camino_tc: None,
                    }
                })
            })
        }
        Formula::Parent { hijo, padre } => {
            let hijo = resolver_nodo(hijo, estructura, entorno)?;
            let padre = resolver_nodo(padre, estructura, entorno)?;
            let valor = estructura.padre(hijo) == Some(padre);
            resultado_interno(valor, modo, |_| {
                Ok({
                    ReglaEvidencia::Atomo {
                        constructor: ConstructorAtomico::Parent,
                        camino_tc: None,
                    }
                })
            })
        }
        Formula::HasLab { sujeto, etiqueta } => {
            let etiqueta = resolver_etiqueta(etiqueta)?;
            let (valor, constructor) = match sujeto {
                SujetoTermino::Nodo(termino) => {
                    let id = resolver_nodo(termino, estructura, entorno)?;
                    let nodo = estructura.nodo(id).ok_or_else(|| {
                        ErrorEvaluacion::NodoLigadoInexistente(nombre_variable_nodo(termino))
                    })?;
                    (nodo.etiqueta == etiqueta, ConstructorAtomico::HasLabNodo)
                }
                SujetoTermino::Arista(termino) => {
                    let id = resolver_arista(termino, estructura, entorno, presupuesto)?;
                    let arista = estructura.arista(id).ok_or_else(|| {
                        ErrorEvaluacion::AristaLigadaInexistente(nombre_variable_arista(termino))
                    })?;
                    (
                        arista.etiqueta == etiqueta,
                        ConstructorAtomico::HasLabArista,
                    )
                }
            };
            resultado_interno(valor, modo, |_| {
                Ok({
                    ReglaEvidencia::Atomo {
                        constructor,
                        camino_tc: None,
                    }
                })
            })
        }
        Formula::Ext {
            predicado,
            sujeto,
            constante,
        } => {
            let id = resolver_nodo(sujeto, estructura, entorno)?;
            let constante = resolver_const_ext(constante)?;
            if estructura.nodo(id).is_none() {
                return Err(ErrorEvaluacion::NodoLigadoInexistente(
                    nombre_variable_nodo(sujeto),
                ));
            }
            let valor = estructura.atributos(id).get(predicado) == Some(&constante);
            resultado_interno(valor, modo, |_| {
                Ok({
                    ReglaEvidencia::Atomo {
                        constructor: ConstructorAtomico::Ext,
                        camino_tc: None,
                    }
                })
            })
        }
        Formula::TcParent { desde, hasta } => {
            let desde_id = resolver_nodo(desde, estructura, entorno)?;
            let hasta_id = resolver_nodo(hasta, estructura, entorno)?;
            if estructura.nodo(desde_id).is_none() {
                return Err(ErrorEvaluacion::NodoLigadoInexistente(
                    nombre_variable_nodo(desde),
                ));
            }
            let mut actual = estructura.padre(desde_id);
            let mut visitados = HashSet::new();
            while let Some(ancestro) = actual {
                presupuesto.consumir(1)?;
                estructura.nodo(ancestro).ok_or_else(|| {
                    ErrorEvaluacion::NodoLigadoInexistente(nombre_variable_nodo(desde))
                })?;
                if ancestro == hasta_id {
                    let camino = if modo.capturando() {
                        // Reconstruir solo cuando hay ruta positiva evita
                        // materializar evidencia descartable para TC falso.
                        // Cada entrada temporal/final se cobra antes del push,
                        // la reserva y el clone que la materializan.
                        let mut nodos = Vec::new();
                        modo.consumir(1)?;
                        nodos.push(desde_id);
                        let mut nodo_actual = desde_id;
                        while nodo_actual != hasta_id {
                            let padre = estructura.padre(nodo_actual).ok_or_else(|| {
                                ErrorEvaluacion::NodoLigadoInexistente(nombre_variable_nodo(desde))
                            })?;
                            modo.consumir(1)?;
                            nodos.push(padre);
                            nodo_actual = padre;
                        }
                        modo.consumir(u64::try_from(nodos.len()).unwrap_or(u64::MAX))?;
                        let mut camino = Vec::with_capacity(nodos.len());
                        for id in nodos {
                            let nodo = estructura.nodo(id).ok_or_else(|| {
                                ErrorEvaluacion::NodoLigadoInexistente(nombre_variable_nodo(desde))
                            })?;
                            modo.consumir_bytes(nodo.path.como_str().len())?;
                            camino.push(nodo.path.clone());
                        }
                        Some(camino)
                    } else {
                        None
                    };
                    return resultado_interno(true, modo, |_| {
                        Ok(ReglaEvidencia::Atomo {
                            constructor: ConstructorAtomico::TcParent,
                            camino_tc: camino,
                        })
                    });
                }
                if !visitados.insert(ancestro) {
                    break;
                }
                actual = estructura.padre(ancestro);
            }
            resultado_interno(false, modo, |_| {
                Ok(ReglaEvidencia::Atomo {
                    constructor: ConstructorAtomico::TcParent,
                    camino_tc: None,
                })
            })
        }
        Formula::IgualNodo(a, b) => {
            let valor =
                resolver_nodo(a, estructura, entorno)? == resolver_nodo(b, estructura, entorno)?;
            resultado_interno(valor, modo, |_| {
                Ok(ReglaEvidencia::Atomo {
                    constructor: ConstructorAtomico::IgualNodo,
                    camino_tc: None,
                })
            })
        }
        Formula::IgualArista(a, b) => {
            let valor = resolver_arista(a, estructura, entorno, presupuesto)?
                == resolver_arista(b, estructura, entorno, presupuesto)?;
            resultado_interno(valor, modo, |_| {
                Ok(ReglaEvidencia::Atomo {
                    constructor: ConstructorAtomico::IgualArista,
                    camino_tc: None,
                })
            })
        }
        Formula::No(interior) => {
            let evaluacion = evaluar_interno(interior, estructura, entorno, presupuesto, modo)?;
            let valor = !evaluacion.valor;
            resultado_interno(valor, modo, |modo| {
                Ok(ReglaEvidencia::No {
                    interior: encajar_evidencia(
                        evaluacion
                            .evidencia
                            .expect("el modo trazable conserva evidencia del interior"),
                        modo,
                    )?,
                })
            })
        }
        Formula::Y(partes) => {
            let mut partes_evaluadas = Vec::new();
            for parte in partes {
                let evaluacion = evaluar_interno(parte, estructura, entorno, presupuesto, modo)?;
                let valor = evaluacion.valor;
                tomar_evidencia(evaluacion, modo, &mut partes_evaluadas)?;
                if !valor {
                    return resultado_interno(false, modo, |_| {
                        Ok(ReglaEvidencia::Conjuncion { partes_evaluadas })
                    });
                }
            }
            resultado_interno(true, modo, |_| {
                Ok(ReglaEvidencia::Conjuncion { partes_evaluadas })
            })
        }
        Formula::O(partes) => {
            let mut partes_evaluadas = Vec::new();
            for parte in partes {
                let evaluacion = evaluar_interno(parte, estructura, entorno, presupuesto, modo)?;
                let valor = evaluacion.valor;
                tomar_evidencia(evaluacion, modo, &mut partes_evaluadas)?;
                if valor {
                    return resultado_interno(true, modo, |_| {
                        Ok(ReglaEvidencia::Disyuncion { partes_evaluadas })
                    });
                }
            }
            resultado_interno(false, modo, |_| {
                Ok(ReglaEvidencia::Disyuncion { partes_evaluadas })
            })
        }
        Formula::Implica(antecedente, consecuente) => {
            let antecedente = evaluar_interno(antecedente, estructura, entorno, presupuesto, modo)?;
            if !antecedente.valor {
                resultado_interno(true, modo, |modo| {
                    Ok(ReglaEvidencia::Implicacion {
                        antecedente: encajar_evidencia(
                            antecedente
                                .evidencia
                                .expect("el modo trazable conserva evidencia del antecedente"),
                            modo,
                        )?,
                        consecuente: None,
                    })
                })
            } else {
                let consecuente =
                    evaluar_interno(consecuente, estructura, entorno, presupuesto, modo)?;
                let valor = consecuente.valor;
                resultado_interno(valor, modo, |modo| {
                    Ok(ReglaEvidencia::Implicacion {
                        antecedente: encajar_evidencia(
                            antecedente
                                .evidencia
                                .expect("el modo trazable conserva evidencia del antecedente"),
                            modo,
                        )?,
                        consecuente: Some(encajar_evidencia(
                            consecuente
                                .evidencia
                                .expect("el modo trazable conserva evidencia del consecuente"),
                            modo,
                        )?),
                    })
                })
            }
        }
        Formula::ParaTodo {
            ligaduras,
            guarda,
            cuerpo,
        } => evaluar_cuantificador(
            ligaduras,
            guarda.as_deref(),
            cuerpo,
            estructura,
            entorno,
            presupuesto,
            ModoCuantificador {
                universal: true,
                evidencia: modo,
            },
        ),
        Formula::Existe {
            ligaduras,
            guarda,
            cuerpo,
        } => evaluar_cuantificador(
            ligaduras,
            guarda.as_deref(),
            cuerpo,
            estructura,
            entorno,
            presupuesto,
            ModoCuantificador {
                universal: false,
                evidencia: modo,
            },
        ),
    }
}

fn evaluar_cuantificador(
    ligaduras: &[(Var, SorteVar)],
    guarda: Option<&Formula>,
    cuerpo: &Formula,
    estructura: &Estructura,
    entorno: &mut Entorno,
    presupuesto: &mut Presupuesto,
    modo: ModoCuantificador<'_>,
) -> Result<EvaluacionInterna, ErrorEvaluacion> {
    let ModoCuantificador {
        universal,
        evidencia,
    } = modo;
    let capturando = evidencia.capturando();
    if ligaduras.is_empty() {
        presupuesto.consumir(1)?;
        let evaluacion = evaluar_instancia_cuantificada(
            guarda,
            cuerpo,
            universal,
            estructura,
            entorno,
            presupuesto,
            evidencia,
        )?;
        let valor = evaluacion.valor;
        let mut instancias = Vec::new();
        if capturando {
            let instancia =
                instancia_con_evidencia(evaluacion, AsignacionCanonica::default(), evidencia)?;
            evidencia.consumir(1)?;
            instancias.push(instancia);
        }
        return resultado_interno(valor, evidencia, |_| {
            Ok(ReglaEvidencia::Cuantificador {
                tipo: tipo_cuantificador(universal),
                instancias,
                busqueda_agotada: true,
            })
        });
    }

    let mut dominio_nodos = Vec::new();
    for (id, _) in estructura.nodos() {
        presupuesto.consumir(1)?;
        dominio_nodos.push(id);
    }
    consumir_costo_ordenamiento(presupuesto, dominio_nodos.len())?;
    dominio_nodos.sort_by(|a, b| {
        estructura
            .nodo(*a)
            .map(|n| &n.path)
            .cmp(&estructura.nodo(*b).map(|n| &n.path))
    });
    let mut dominio_aristas = Vec::new();
    for (id, _) in estructura.aristas() {
        presupuesto.consumir(1)?;
        dominio_aristas.push(id);
    }
    consumir_costo_ordenamiento(presupuesto, dominio_aristas.len())?;
    dominio_aristas.sort_by(|a, b| {
        identidad_arista(estructura, *a)
            .map(|id| id.to_string())
            .cmp(&identidad_arista(estructura, *b).map(|id| id.to_string()))
    });

    // El producto cartesiano se recorre con un odómetro. Así, el número de
    // ligaduras no añade recursión a la pila; solo la profundidad del AST lo
    // hace, y esa queda limitada por el preflight.
    if ligaduras.iter().any(|(_, sorte)| match sorte {
        SorteVar::Nodo => dominio_nodos.is_empty(),
        SorteVar::Arista => dominio_aristas.is_empty(),
    }) {
        return resultado_interno(universal, evidencia, |_| {
            Ok(ReglaEvidencia::Cuantificador {
                tipo: tipo_cuantificador(universal),
                instancias: Vec::new(),
                busqueda_agotada: true,
            })
        });
    }

    let anteriores_nodos: Vec<(Var, Option<NodoId>)> = ligaduras
        .iter()
        .filter(|(_, sorte)| *sorte == SorteVar::Nodo)
        .map(|(variable, _)| (variable.clone(), entorno.nodos.get(variable).copied()))
        .collect();
    let anteriores_aristas: Vec<(Var, Option<AristaId>)> = ligaduras
        .iter()
        .filter(|(_, sorte)| *sorte == SorteVar::Arista)
        .map(|(variable, _)| (variable.clone(), entorno.aristas.get(variable).copied()))
        .collect();

    let resultado = (|| {
        let mut indices = vec![0; ligaduras.len()];
        let mut instancias = Vec::new();
        loop {
            presupuesto.consumir(1)?;
            for (indice, (variable, sorte)) in ligaduras.iter().enumerate() {
                presupuesto.consumir(1)?;
                match sorte {
                    SorteVar::Nodo => {
                        entorno
                            .nodos
                            .insert(variable.clone(), dominio_nodos[indices[indice]]);
                    }
                    SorteVar::Arista => {
                        entorno
                            .aristas
                            .insert(variable.clone(), dominio_aristas[indices[indice]]);
                    }
                }
            }

            let evaluacion = evaluar_instancia_cuantificada(
                guarda,
                cuerpo,
                universal,
                estructura,
                entorno,
                presupuesto,
                evidencia,
            )?;
            let valor = evaluacion.valor;
            if capturando {
                let asignacion = capturar_asignacion(
                    ligaduras,
                    &indices,
                    &dominio_nodos,
                    &dominio_aristas,
                    estructura,
                    evidencia,
                )?;
                let instancia = instancia_con_evidencia(evaluacion, asignacion, evidencia)?;
                evidencia.consumir(1)?;
                instancias.push(instancia);
            }
            if universal && !valor {
                return resultado_interno(false, evidencia, |_| {
                    Ok(ReglaEvidencia::Cuantificador {
                        tipo: TipoCuantificador::ParaTodo,
                        instancias,
                        busqueda_agotada: false,
                    })
                });
            }
            if !universal && valor {
                return resultado_interno(true, evidencia, |_| {
                    Ok(ReglaEvidencia::Cuantificador {
                        tipo: TipoCuantificador::Existe,
                        instancias,
                        busqueda_agotada: false,
                    })
                });
            }
            if !avanzar_combinacion(
                &mut indices,
                ligaduras,
                dominio_nodos.len(),
                dominio_aristas.len(),
            ) {
                return resultado_interno(universal, evidencia, |_| {
                    Ok(ReglaEvidencia::Cuantificador {
                        tipo: tipo_cuantificador(universal),
                        instancias,
                        busqueda_agotada: true,
                    })
                });
            }
        }
    })();

    for (variable, anterior) in anteriores_nodos {
        restaurar(&mut entorno.nodos, &variable, anterior);
    }
    for (variable, anterior) in anteriores_aristas {
        restaurar(&mut entorno.aristas, &variable, anterior);
    }
    resultado
}

struct EvaluacionInstancia {
    valor: bool,
    guarda: Option<EvaluacionInterna>,
    cuerpo: Option<EvaluacionInterna>,
}

fn evaluar_instancia_cuantificada(
    guarda: Option<&Formula>,
    cuerpo: &Formula,
    universal: bool,
    estructura: &Estructura,
    entorno: &mut Entorno,
    presupuesto: &mut Presupuesto,
    evidencia: &mut ModoEvidencia,
) -> Result<EvaluacionInstancia, ErrorEvaluacion> {
    let guarda_evaluada = match guarda {
        Some(guarda) => Some(evaluar_interno(
            guarda,
            estructura,
            entorno,
            presupuesto,
            evidencia,
        )?),
        None => None,
    };
    let cubierta = guarda_evaluada.as_ref().is_none_or(|guardia| guardia.valor);
    if !cubierta {
        return Ok(EvaluacionInstancia {
            valor: universal,
            guarda: guarda_evaluada,
            cuerpo: None,
        });
    }
    let cuerpo_evaluado = evaluar_interno(cuerpo, estructura, entorno, presupuesto, evidencia)?;
    Ok(EvaluacionInstancia {
        valor: cuerpo_evaluado.valor,
        guarda: guarda_evaluada,
        cuerpo: Some(cuerpo_evaluado),
    })
}

fn tipo_cuantificador(universal: bool) -> TipoCuantificador {
    if universal {
        TipoCuantificador::ParaTodo
    } else {
        TipoCuantificador::Existe
    }
}

fn instancia_con_evidencia(
    instancia: EvaluacionInstancia,
    asignacion: AsignacionCanonica,
    evidencia: &mut ModoEvidencia,
) -> Result<InstanciaCuantificada, ErrorEvaluacion> {
    evidencia.consumir(1)?;
    let guarda = match instancia.guarda {
        Some(guardia) => Some(encajar_evidencia(
            guardia
                .evidencia
                .expect("el modo trazable conserva evidencia de la guarda"),
            evidencia,
        )?),
        None => None,
    };
    let cuerpo = match instancia.cuerpo {
        Some(cuerpo) => Some(encajar_evidencia(
            cuerpo
                .evidencia
                .expect("el modo trazable conserva evidencia del cuerpo"),
            evidencia,
        )?),
        None => None,
    };
    Ok(InstanciaCuantificada {
        asignacion,
        guarda,
        cuerpo,
        valor: instancia.valor,
    })
}

fn capturar_asignacion(
    ligaduras: &[(Var, SorteVar)],
    indices: &[usize],
    dominio_nodos: &[NodoId],
    dominio_aristas: &[AristaId],
    estructura: &Estructura,
    evidencia: &mut ModoEvidencia,
) -> Result<AsignacionCanonica, ErrorEvaluacion> {
    evidencia.consumir(u64::try_from(ligaduras.len()).unwrap_or(u64::MAX))?;
    let mut asignacion = AsignacionCanonica {
        ligaduras: Vec::with_capacity(ligaduras.len()),
    };
    for (indice, (variable, sorte)) in ligaduras.iter().enumerate() {
        let valor = match sorte {
            SorteVar::Nodo => {
                let id = dominio_nodos[indices[indice]];
                let nodo = estructura
                    .nodo(id)
                    .ok_or_else(|| ErrorEvaluacion::NodoLigadoInexistente(variable.clone()))?;
                evidencia.consumir_bytes(nodo.path.como_str().len())?;
                ValorLigadura::Nodo(nodo.path.clone())
            }
            SorteVar::Arista => {
                let id = dominio_aristas[indices[indice]];
                let arista = estructura
                    .arista(id)
                    .ok_or_else(|| ErrorEvaluacion::AristaLigadaInexistente(variable.clone()))?;
                let origen = estructura
                    .nodo(arista.origen)
                    .ok_or_else(|| ErrorEvaluacion::AristaLigadaInexistente(variable.clone()))?;
                let destino = estructura
                    .nodo(arista.destino)
                    .ok_or_else(|| ErrorEvaluacion::AristaLigadaInexistente(variable.clone()))?;
                let bytes = origen
                    .path
                    .como_str()
                    .len()
                    .saturating_add(destino.path.como_str().len())
                    .saturating_add(arista.tipo.como_str().len());
                evidencia.consumir_bytes(bytes)?;
                ValorLigadura::Arista(IdArista {
                    origen: origen.path.clone(),
                    sorte: arista.tipo.clone(),
                    destino: destino.path.clone(),
                    ordinal: arista.ordinal,
                })
            }
        };
        evidencia.consumir_bytes(variable.como_str().len())?;
        asignacion.ligaduras.push(LigaduraCanonica {
            variable: variable.clone(),
            valor,
        });
    }
    Ok(asignacion)
}

fn avanzar_combinacion(
    indices: &mut [usize],
    ligaduras: &[(Var, SorteVar)],
    cantidad_nodos: usize,
    cantidad_aristas: usize,
) -> bool {
    for indice in (0..ligaduras.len()).rev() {
        indices[indice] += 1;
        let cantidad = match ligaduras[indice].1 {
            SorteVar::Nodo => cantidad_nodos,
            SorteVar::Arista => cantidad_aristas,
        };
        if indices[indice] < cantidad {
            return true;
        }
        indices[indice] = 0;
    }
    false
}

fn consumir_costo_ordenamiento(
    presupuesto: &mut Presupuesto,
    cantidad: usize,
) -> Result<(), ErrorEvaluacion> {
    if cantidad < 2 {
        return Ok(());
    }
    let niveles = usize::BITS - (cantidad - 1).leading_zeros();
    let cantidad = u64::try_from(cantidad).unwrap_or(u64::MAX);
    let costo = cantidad.saturating_mul(u64::from(niveles));
    presupuesto.consumir(costo)
}

fn restaurar<K: std::hash::Hash + Eq + Clone, V>(
    entorno: &mut HashMap<K, V>,
    llave: &K,
    anterior: Option<V>,
) {
    if let Some(anterior) = anterior {
        entorno.insert(llave.clone(), anterior);
    } else {
        entorno.remove(llave);
    }
}

fn resolver_nodo(
    termino: &TerminoNodo,
    estructura: &Estructura,
    entorno: &Entorno,
) -> Result<NodoId, ErrorEvaluacion> {
    match termino {
        TerminoNodo::Var(variable) => {
            entorno
                .nodos
                .get(variable)
                .copied()
                .ok_or_else(|| ErrorEvaluacion::VariableNoLigada {
                    variable: variable.clone(),
                    sorte: SorteVar::Nodo,
                })
        }
        TerminoNodo::Constante(path) => estructura
            .por_path(path)
            .ok_or_else(|| ErrorEvaluacion::ConstanteNodoInexistente(path.clone())),
    }
}

fn resolver_arista(
    termino: &TerminoArista,
    estructura: &Estructura,
    entorno: &Entorno,
    presupuesto: &mut Presupuesto,
) -> Result<AristaId, ErrorEvaluacion> {
    match termino {
        TerminoArista::Var(variable) => entorno.aristas.get(variable).copied().ok_or_else(|| {
            ErrorEvaluacion::VariableNoLigada {
                variable: variable.clone(),
                sorte: SorteVar::Arista,
            }
        }),
        TerminoArista::Constante(id) => encontrar_arista(estructura, id, presupuesto)?
            .ok_or_else(|| ErrorEvaluacion::ConstanteAristaInexistente(id.clone())),
    }
}

fn resolver_sorte(termino: &TerminoSorte) -> Result<Sorte, ErrorEvaluacion> {
    match termino {
        TerminoSorte::Literal(sorte) => Ok(sorte.clone()),
        TerminoSorte::Meta(variable) => Err(ErrorEvaluacion::MetaNoInstanciada(variable.clone())),
    }
}

fn resolver_etiqueta(termino: &TerminoEtiqueta) -> Result<Etiqueta, ErrorEvaluacion> {
    match termino {
        TerminoEtiqueta::Literal(etiqueta) => Ok(etiqueta.clone()),
        TerminoEtiqueta::Meta(variable) => {
            Err(ErrorEvaluacion::MetaNoInstanciada(variable.clone()))
        }
    }
}

fn resolver_const_ext(termino: &TerminoConstExt) -> Result<String, ErrorEvaluacion> {
    match termino {
        TerminoConstExt::Literal(constante) => Ok(constante.clone()),
        TerminoConstExt::Meta(variable) => {
            Err(ErrorEvaluacion::MetaNoInstanciada(variable.clone()))
        }
    }
}

fn identidad_arista(estructura: &Estructura, id: AristaId) -> Option<IdArista> {
    let arista = estructura.arista(id)?;
    Some(IdArista {
        origen: estructura.nodo(arista.origen)?.path.clone(),
        sorte: arista.tipo.clone(),
        destino: estructura.nodo(arista.destino)?.path.clone(),
        ordinal: arista.ordinal,
    })
}

fn encontrar_arista(
    estructura: &Estructura,
    buscada: &IdArista,
    presupuesto: &mut Presupuesto,
) -> Result<Option<AristaId>, ErrorEvaluacion> {
    for (id, _) in estructura.aristas() {
        presupuesto.consumir(1)?;
        if identidad_arista(estructura, id).as_ref() == Some(buscada) {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

fn nombre_variable_nodo(termino: &TerminoNodo) -> Var {
    match termino {
        TerminoNodo::Var(variable) => variable.clone(),
        TerminoNodo::Constante(path) => Var::nuevo(&format!("@{path}")),
    }
}

fn nombre_variable_arista(termino: &TerminoArista) -> Var {
    match termino {
        TerminoArista::Var(variable) => variable.clone(),
        TerminoArista::Constante(id) => Var::nuevo(&format!("@{id}")),
    }
}

#[cfg(test)]
#[path = "evaluador/tests.rs"]
mod tests;
