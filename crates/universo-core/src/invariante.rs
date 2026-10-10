//! Validación estática de sentencias de invariantes contra una instancia.
//!
//! Esta etapa comprueba nombres, sortes, dominios y referencias a constantes
//! antes de evaluar la fórmula. No decide si una fórmula es verdadera y no
//! lee anexos/cuerpos documentales. Las ubicaciones apuntan a una ruta dentro
//! del AST porque el AST actual conserva posiciones solo por sentencia.

use std::collections::{HashMap, HashSet};

use crate::atomos::IdArista;
use crate::estructura::Estructura;
use crate::formula::{
    DominioEsquema, Formula, Sentencia, SorteVar, SujetoTermino, TerminoArista, TerminoConstExt,
    TerminoEtiqueta, TerminoNodo, TerminoSorte, Var,
};
use crate::manifiesto::Manifiesto;
use crate::sexpr::SentenciaFuente;

/// Código estable de un error de validación estática.
///
/// Los códigos y ubicaciones no incluyen los valores del bundle ni el texto
/// de las fórmulas. El llamador puede mostrar la fuente local si el usuario
/// lo solicita explícitamente.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CodigoValidacionInvariante {
    ManifiestoInvalido,
    ManifiestoDesalineado,
    TipoNodoNoDeclarado,
    TipoAristaNoDeclarado,
    PredicadoNoDeclarado,
    ConstanteExtFueraDeDominio,
    ConstanteNodoInexistente,
    ConstanteAristaInexistente,
    VariableLibre,
    SorteVariableIncorrecta,
    VariableRedeclarada,
    VariableCambiaDeSorte,
    VariableColisionaConMeta,
    IdentificadorInvalido,
    MetaNoLigada,
    MetaDominioDesconocido,
    MetaDominioIncompatible,
    MetaSinUso,
    MetaDuplicada,
    AnidamientoExcesivo,
    LimiteLigadurasExcedido,
}

impl CodigoValidacionInvariante {
    /// Nombre apto para resultados estructurados.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ManifiestoInvalido => "manifest_invalid",
            Self::ManifiestoDesalineado => "manifest_mismatch",
            Self::TipoNodoNoDeclarado => "node_type_undeclared",
            Self::TipoAristaNoDeclarado => "edge_type_undeclared",
            Self::PredicadoNoDeclarado => "predicate_undeclared",
            Self::ConstanteExtFueraDeDominio => "extension_value_out_of_domain",
            Self::ConstanteNodoInexistente => "node_constant_missing",
            Self::ConstanteAristaInexistente => "edge_constant_missing",
            Self::VariableLibre => "free_variable",
            Self::SorteVariableIncorrecta => "variable_sort_mismatch",
            Self::VariableRedeclarada => "variable_redeclared",
            Self::VariableCambiaDeSorte => "variable_sort_reused",
            Self::VariableColisionaConMeta => "variable_schema_collision",
            Self::IdentificadorInvalido => "invalid_identifier",
            Self::MetaNoLigada => "unbound_schema_meta",
            Self::MetaDominioDesconocido => "schema_domain_unknown",
            Self::MetaDominioIncompatible => "schema_domain_mismatch",
            Self::MetaSinUso => "schema_meta_unused",
            Self::MetaDuplicada => "schema_meta_redeclared",
            Self::AnidamientoExcesivo => "formula_nesting_exceeded",
            Self::LimiteLigadurasExcedido => "quantifier_binding_limit_exceeded",
        }
    }

    const fn mensaje(self) -> &'static str {
        match self {
            Self::ManifiestoInvalido => "el manifiesto no es válido",
            Self::ManifiestoDesalineado => "el manifiesto no corresponde a la estructura",
            Self::TipoNodoNoDeclarado => "tipo de nodo no declarado",
            Self::TipoAristaNoDeclarado => "tipo de arista no declarado",
            Self::PredicadoNoDeclarado => "predicado de extensión no declarado",
            Self::ConstanteExtFueraDeDominio => {
                "constante de extensión fuera del dominio declarado"
            }
            Self::ConstanteNodoInexistente => "la constante no designa un nodo de la estructura",
            Self::ConstanteAristaInexistente => {
                "la constante no designa una arista de la estructura"
            }
            Self::VariableLibre => "variable no ligada",
            Self::SorteVariableIncorrecta => "variable usada con una sorte incompatible",
            Self::VariableRedeclarada => "variable redeclarada en un alcance activo",
            Self::VariableCambiaDeSorte => "el nombre de variable cambia de sorte",
            Self::VariableColisionaConMeta => {
                "una variable cuantificada colisiona con una metavariable de esquema"
            }
            Self::IdentificadorInvalido => "identificador de variable inválido",
            Self::MetaNoLigada => "metavariable usada fuera de un esquema",
            Self::MetaDominioDesconocido => "dominio de metavariable no declarado",
            Self::MetaDominioIncompatible => {
                "dominio de metavariable incompatible con esta posición"
            }
            Self::MetaSinUso => "metavariable de esquema sin uso",
            Self::MetaDuplicada => "metavariable de esquema redeclarada",
            Self::AnidamientoExcesivo => "la fórmula excede el límite de anidamiento",
            Self::LimiteLigadurasExcedido => {
                "el cuantificador excede el máximo de ligaduras permitido"
            }
        }
    }
}

/// Diagnóstico de validación que evita incluir en el mensaje valores de la
/// estructura (paths, etiquetas o atributos).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErrorValidacionInvariante {
    pub codigo: CodigoValidacionInvariante,
    /// Ruta determinista dentro del AST, por ejemplo `$.cuerpo.sorte`.
    pub ubicacion: String,
    /// Posición del inicio de la sentencia top-level, si proviene de un
    /// [`SentenciaFuente`]. El AST no conserva spans por nodo.
    pub linea: Option<usize>,
    pub columna: Option<usize>,
}

impl ErrorValidacionInvariante {
    fn nuevo(codigo: CodigoValidacionInvariante, ubicacion: &str) -> Self {
        Self {
            codigo,
            ubicacion: ubicacion.to_string(),
            linea: None,
            columna: None,
        }
    }
}

impl std::fmt::Display for ErrorValidacionInvariante {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.codigo.as_str(), self.codigo.mensaje())?;
        if let (Some(linea), Some(columna)) = (self.linea, self.columna) {
            write!(f, " ({linea}:{columna})")?;
        }
        write!(f, " en {}", self.ubicacion)
    }
}

impl std::error::Error for ErrorValidacionInvariante {}

/// Profundidad máxima validada también para AST construidos directamente,
/// fuera del parser que ya aplica el mismo límite.
pub const PROFUNDIDAD_MAXIMA_FORMULA: usize = 32;

/// Máximo de variables cuantificadas por nodo de cuantificador. Se comprueba
/// antes de recorrer o enumerar el dominio del cuantificador.
pub const MAX_LIGADURAS_POR_CUANTIFICADOR: usize = 64;

/// Valida una sentencia contra el manifiesto que declara su firma y la
/// estructura concreta que interpreta sus constantes.
///
/// `KV` se toma literalmente de `Manifiesto::sortes_nodo`, incluidos los
/// tipos reservados del motor. Las etiquetas (`Lab`) se resuelven durante la
/// instanciación, no en esta validación de tipos.
pub fn validar_sentencia(
    sentencia: &Sentencia,
    manifiesto: &Manifiesto,
    estructura: &Estructura,
) -> Vec<ErrorValidacionInvariante> {
    validar_interna(sentencia, manifiesto, estructura, None)
}

/// Variante que adjunta la línea y columna de inicio conservadas por el
/// parser del programa. La ruta estructural del AST sigue identificando la
/// posición lógica dentro de esa expresión.
pub fn validar_sentencia_fuente(
    fuente: &SentenciaFuente,
    manifiesto: &Manifiesto,
    estructura: &Estructura,
) -> Vec<ErrorValidacionInvariante> {
    validar_interna(
        &fuente.sentencia,
        manifiesto,
        estructura,
        Some((fuente.linea, fuente.columna)),
    )
}

fn validar_interna(
    sentencia: &Sentencia,
    manifiesto: &Manifiesto,
    estructura: &Estructura,
    posicion: Option<(usize, usize)>,
) -> Vec<ErrorValidacionInvariante> {
    let mut errores = Vec::new();
    if manifiesto.validar().is_err() {
        errores.push(ErrorValidacionInvariante::nuevo(
            CodigoValidacionInvariante::ManifiestoInvalido,
            "$",
        ));
        return adjuntar_posicion(errores, posicion);
    }
    if manifiesto != estructura.manifiesto() {
        errores.push(ErrorValidacionInvariante::nuevo(
            CodigoValidacionInvariante::ManifiestoDesalineado,
            "$",
        ));
        return adjuntar_posicion(errores, posicion);
    }

    let mut validador = Validador::nuevo(manifiesto, estructura);
    match sentencia {
        Sentencia::Oracion(formula) => {
            validador.con_ruta("formula", |v| v.formula(formula));
        }
        Sentencia::Esquema { ligaduras, cuerpo } => {
            validador.preparar_metas(ligaduras);
            validador.con_ruta("cuerpo", |v| v.formula(cuerpo));
            for (indice, (meta, _)) in ligaduras.iter().enumerate() {
                if !validador.metas_usadas.contains(meta)
                    && !validador.identificadores_invalidos.contains(meta)
                {
                    validador.con_ruta(&format!("ligaduras[{indice}]"), |v| {
                        v.error(CodigoValidacionInvariante::MetaSinUso)
                    });
                }
            }
        }
    }
    adjuntar_posicion(validador.errores, posicion)
}

fn adjuntar_posicion(
    mut errores: Vec<ErrorValidacionInvariante>,
    posicion: Option<(usize, usize)>,
) -> Vec<ErrorValidacionInvariante> {
    if let Some((linea, columna)) = posicion {
        for error in &mut errores {
            error.linea = Some(linea);
            error.columna = Some(columna);
        }
    }
    errores
}

#[derive(Clone, Copy)]
enum DominioEsperado<'a> {
    SorteNodo,
    SorteArista,
    Etiqueta,
    ConstanteExt(&'a str),
}

struct Validador<'a> {
    manifiesto: &'a Manifiesto,
    estructura: &'a Estructura,
    constantes_arista: HashSet<IdArista>,
    metas: Option<HashMap<Var, DominioEsquema>>,
    metas_usadas: HashSet<Var>,
    nombres_generados: HashSet<Var>,
    identificadores_invalidos: HashSet<Var>,
    alcance: HashMap<Var, SorteVar>,
    sortes_declaradas: HashMap<Var, SorteVar>,
    ruta: String,
    profundidad: usize,
    errores: Vec<ErrorValidacionInvariante>,
}

impl<'a> Validador<'a> {
    fn nuevo(manifiesto: &'a Manifiesto, estructura: &'a Estructura) -> Self {
        let mut constantes_arista = HashSet::new();
        for (_, arista) in estructura.aristas() {
            let (Some(origen), Some(destino)) = (
                estructura.nodo(arista.origen),
                estructura.nodo(arista.destino),
            ) else {
                continue;
            };
            constantes_arista.insert(IdArista {
                origen: origen.path.clone(),
                sorte: arista.tipo.clone(),
                destino: destino.path.clone(),
                ordinal: arista.ordinal,
            });
        }
        Self {
            manifiesto,
            estructura,
            constantes_arista,
            metas: None,
            metas_usadas: HashSet::new(),
            nombres_generados: HashSet::new(),
            identificadores_invalidos: HashSet::new(),
            alcance: HashMap::new(),
            sortes_declaradas: HashMap::new(),
            ruta: "$".to_string(),
            profundidad: 0,
            errores: Vec::new(),
        }
    }

    fn error(&mut self, codigo: CodigoValidacionInvariante) {
        self.errores
            .push(ErrorValidacionInvariante::nuevo(codigo, &self.ruta));
    }

    fn con_ruta(&mut self, segmento: &str, accion: impl FnOnce(&mut Self)) {
        let largo = self.ruta.len();
        if segmento.starts_with('[') {
            self.ruta.push_str(segmento);
        } else {
            self.ruta.push('.');
            self.ruta.push_str(segmento);
        }
        accion(self);
        self.ruta.truncate(largo);
    }

    fn validar_identificador(&mut self, variable: &Var, generado: bool) -> bool {
        let nombre = variable.como_str();
        let invalido = nombre.is_empty()
            || nombre.starts_with('@')
            || (!generado && nombre.starts_with(crate::formula::PREFIJO_GENERADO))
            || nombre.chars().any(|caracter| {
                caracter.is_whitespace()
                    || caracter.is_control()
                    || matches!(caracter, '(' | ')' | '"' | ';')
            });
        if invalido {
            if self.identificadores_invalidos.insert(variable.clone()) {
                self.error(CodigoValidacionInvariante::IdentificadorInvalido);
            }
            false
        } else {
            true
        }
    }

    fn preparar_metas(&mut self, ligaduras: &[(Var, DominioEsquema)]) {
        let mut metas = HashMap::new();
        for (indice, (meta, dominio)) in ligaduras.iter().enumerate() {
            self.con_ruta(&format!("ligaduras[{indice}]"), |v| {
                v.validar_identificador(meta, false);
                if let DominioEsquema::DominioPredicado(nombre) = dominio {
                    if v.manifiesto.predicado(nombre).is_none() {
                        v.error(CodigoValidacionInvariante::MetaDominioDesconocido);
                    }
                }
            });
            if metas.contains_key(meta) {
                self.con_ruta(&format!("ligaduras[{indice}]"), |v| {
                    v.error(CodigoValidacionInvariante::MetaDuplicada)
                });
            } else {
                metas.insert(meta.clone(), dominio.clone());
            }
        }
        self.metas = Some(metas);
    }

    fn meta(&mut self, meta: &Var, esperado: DominioEsperado<'_>) {
        if !self.validar_identificador(meta, false) {
            return;
        }
        let dominio = self
            .metas
            .as_ref()
            .and_then(|metas| metas.get(meta))
            .cloned();
        let Some(dominio) = dominio else {
            self.error(CodigoValidacionInvariante::MetaNoLigada);
            return;
        };
        self.metas_usadas.insert(meta.clone());
        let compatible = match (dominio, esperado) {
            (DominioEsquema::SortesNodo, DominioEsperado::SorteNodo)
            | (DominioEsquema::SortesArista, DominioEsperado::SorteArista)
            | (DominioEsquema::Etiquetas, DominioEsperado::Etiqueta) => true,
            (DominioEsquema::DominioPredicado(declarado), DominioEsperado::ConstanteExt(usado)) => {
                declarado == usado
            }
            _ => false,
        };
        if !compatible {
            self.error(CodigoValidacionInvariante::MetaDominioIncompatible);
        }
    }

    fn formula(&mut self, formula: &Formula) {
        if self.profundidad >= PROFUNDIDAD_MAXIMA_FORMULA {
            self.error(CodigoValidacionInvariante::AnidamientoExcesivo);
            return;
        }
        self.profundidad += 1;
        match formula {
            Formula::Type { sujeto, sorte } => {
                self.con_ruta("sujeto", |v| v.termino_nodo(sujeto));
                self.con_ruta("sorte", |v| {
                    v.termino_sorte(sorte, DominioEsperado::SorteNodo)
                });
            }
            Formula::Edge {
                arista,
                origen,
                destino,
                sorte,
            } => {
                self.con_ruta("arista", |v| v.termino_arista(arista));
                self.con_ruta("origen", |v| v.termino_nodo(origen));
                self.con_ruta("destino", |v| v.termino_nodo(destino));
                self.con_ruta("sorte", |v| {
                    v.termino_sorte(sorte, DominioEsperado::SorteArista)
                });
            }
            Formula::Parent { hijo, padre } => {
                self.con_ruta("hijo", |v| v.termino_nodo(hijo));
                self.con_ruta("padre", |v| v.termino_nodo(padre));
            }
            Formula::HasLab { sujeto, etiqueta } => {
                self.con_ruta("sujeto", |v| v.sujeto(sujeto));
                self.con_ruta("etiqueta", |v| v.etiqueta(etiqueta));
            }
            Formula::Ext {
                predicado,
                sujeto,
                constante,
            } => {
                let declarado = self.manifiesto.predicado(predicado).is_some();
                self.con_ruta("predicado", |v| {
                    if !declarado {
                        v.error(CodigoValidacionInvariante::PredicadoNoDeclarado);
                    }
                });
                self.con_ruta("sujeto", |v| v.termino_nodo(sujeto));
                self.con_ruta("constante", |v| {
                    v.constante_ext(constante, predicado, declarado)
                });
            }
            Formula::TcParent { desde, hasta } => {
                self.con_ruta("desde", |v| v.termino_nodo(desde));
                self.con_ruta("hasta", |v| v.termino_nodo(hasta));
            }
            Formula::IgualNodo(izquierda, derecha) => {
                self.con_ruta("izquierda", |v| v.termino_nodo(izquierda));
                self.con_ruta("derecha", |v| v.termino_nodo(derecha));
            }
            Formula::IgualArista(izquierda, derecha) => {
                self.con_ruta("izquierda", |v| v.termino_arista(izquierda));
                self.con_ruta("derecha", |v| v.termino_arista(derecha));
            }
            Formula::No(interior) => {
                let generado = variable_generada_raiz(formula).cloned();
                let agregado = generado
                    .as_ref()
                    .is_some_and(|variable| self.nombres_generados.insert(variable.clone()));
                self.con_ruta("interior", |v| v.formula(interior));
                if agregado {
                    if let Some(variable) = generado {
                        self.nombres_generados.remove(&variable);
                    }
                }
            }
            Formula::Y(partes) | Formula::O(partes) => {
                for (indice, parte) in partes.iter().enumerate() {
                    self.con_ruta(&format!("[{indice}]"), |v| v.formula(parte));
                }
            }
            Formula::Implica(antecedente, consecuente) => {
                self.con_ruta("antecedente", |v| v.formula(antecedente));
                self.con_ruta("consecuente", |v| v.formula(consecuente));
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
            } => self.cuantificador(ligaduras, guarda.as_deref(), cuerpo),
        }
        self.profundidad -= 1;
    }

    fn cuantificador(
        &mut self,
        ligaduras: &[(Var, SorteVar)],
        guarda: Option<&Formula>,
        cuerpo: &Formula,
    ) {
        if ligaduras.len() > MAX_LIGADURAS_POR_CUANTIFICADOR {
            self.error(CodigoValidacionInvariante::LimiteLigadurasExcedido);
            return;
        }
        let mut incorporadas = Vec::new();
        for (indice, (variable, sorte)) in ligaduras.iter().enumerate() {
            self.con_ruta(&format!("ligaduras[{indice}]"), |v| {
                let generado = v.nombres_generados.contains(variable);
                if !v.validar_identificador(variable, generado) {
                    return;
                }
                if v.metas
                    .as_ref()
                    .is_some_and(|metas| metas.contains_key(variable))
                {
                    v.error(CodigoValidacionInvariante::VariableColisionaConMeta);
                } else if v.alcance.contains_key(variable) {
                    v.error(CodigoValidacionInvariante::VariableRedeclarada);
                } else if v
                    .sortes_declaradas
                    .get(variable)
                    .is_some_and(|anterior| anterior != sorte)
                {
                    v.error(CodigoValidacionInvariante::VariableCambiaDeSorte);
                } else {
                    v.sortes_declaradas.insert(variable.clone(), *sorte);
                    v.alcance.insert(variable.clone(), *sorte);
                    incorporadas.push(variable.clone());
                }
            });
        }
        if let Some(guarda) = guarda {
            self.con_ruta("guarda", |v| v.formula(guarda));
        }
        self.con_ruta("cuerpo", |v| v.formula(cuerpo));
        for variable in incorporadas {
            self.alcance.remove(&variable);
        }
    }

    fn termino_nodo(&mut self, termino: &TerminoNodo) {
        match termino {
            TerminoNodo::Var(variable) => self.variable(variable, SorteVar::Nodo),
            TerminoNodo::Constante(path) => {
                if self.estructura.por_path(path).is_none() {
                    self.error(CodigoValidacionInvariante::ConstanteNodoInexistente);
                }
            }
        }
    }

    fn termino_arista(&mut self, termino: &TerminoArista) {
        match termino {
            TerminoArista::Var(variable) => self.variable(variable, SorteVar::Arista),
            TerminoArista::Constante(id) => {
                if !self.manifiesto.es_sorte_arista(id.sorte.como_str()) {
                    self.error(CodigoValidacionInvariante::TipoAristaNoDeclarado);
                } else if !self.constantes_arista.contains(id) {
                    self.error(CodigoValidacionInvariante::ConstanteAristaInexistente);
                }
            }
        }
    }

    fn variable(&mut self, variable: &Var, esperado: SorteVar) {
        if !self.validar_identificador(variable, self.nombres_generados.contains(variable)) {
            return;
        }
        match self.alcance.get(variable) {
            Some(encontrado) if *encontrado == esperado => {}
            Some(_) => self.error(CodigoValidacionInvariante::SorteVariableIncorrecta),
            None => self.error(CodigoValidacionInvariante::VariableLibre),
        }
    }

    fn sujeto(&mut self, sujeto: &SujetoTermino) {
        match sujeto {
            SujetoTermino::Nodo(termino) => self.termino_nodo(termino),
            SujetoTermino::Arista(termino) => self.termino_arista(termino),
        }
    }

    fn termino_sorte(&mut self, termino: &TerminoSorte, esperado: DominioEsperado<'_>) {
        match termino {
            TerminoSorte::Literal(sorte) => {
                let declarada = match esperado {
                    DominioEsperado::SorteNodo => self.manifiesto.es_sorte_nodo(sorte),
                    DominioEsperado::SorteArista => {
                        self.manifiesto.es_sorte_arista(sorte.como_str())
                    }
                    _ => false,
                };
                if !declarada {
                    let codigo = match esperado {
                        DominioEsperado::SorteNodo => {
                            CodigoValidacionInvariante::TipoNodoNoDeclarado
                        }
                        _ => CodigoValidacionInvariante::TipoAristaNoDeclarado,
                    };
                    self.error(codigo);
                }
            }
            TerminoSorte::Meta(meta) => match esperado {
                DominioEsperado::SorteNodo | DominioEsperado::SorteArista => {
                    self.meta(meta, esperado)
                }
                _ => self.error(CodigoValidacionInvariante::MetaDominioIncompatible),
            },
        }
    }

    fn etiqueta(&mut self, termino: &TerminoEtiqueta) {
        if let TerminoEtiqueta::Meta(meta) = termino {
            self.meta(meta, DominioEsperado::Etiqueta);
        }
    }

    fn constante_ext(&mut self, termino: &TerminoConstExt, predicado: &str, declarado: bool) {
        match termino {
            TerminoConstExt::Literal(valor) => {
                if declarado
                    && !self
                        .manifiesto
                        .predicado(predicado)
                        .is_some_and(|decl| decl.dominio.contains(valor))
                {
                    self.error(CodigoValidacionInvariante::ConstanteExtFueraDeDominio);
                }
            }
            TerminoConstExt::Meta(meta) => {
                self.meta(meta, DominioEsperado::ConstanteExt(predicado));
            }
        }
    }
}

/// Si la fórmula es exactamente el azúcar `Raiz` expandido por el parser,
/// devuelve el único nombre reservado que este AST puede ligar internamente.
fn variable_generada_raiz(formula: &Formula) -> Option<&Var> {
    let Formula::No(interior) = formula else {
        return None;
    };
    let Formula::Existe {
        ligaduras,
        guarda: None,
        cuerpo,
    } = interior.as_ref()
    else {
        return None;
    };
    let [(variable, SorteVar::Nodo)] = ligaduras.as_slice() else {
        return None;
    };
    let Formula::Parent {
        padre: TerminoNodo::Var(padre),
        ..
    } = cuerpo.as_ref()
    else {
        return None;
    };
    let sufijo = variable.como_str().strip_prefix("%p")?;
    (variable == padre && !sufijo.is_empty() && sufijo.bytes().all(|byte| byte.is_ascii_digit()))
        .then_some(variable)
}

#[cfg(test)]
mod tests;
