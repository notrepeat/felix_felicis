//! `presence_core` para el fragmento positivo estrictamente operacional.
//!
//! Solo reconoce `forall x̄ :guarda (and Gᵢ) (not (and Aᵢ))`, donde los
//! átomos de guarda y patrón son directos y usan `Type`, `Edge`, `Parent`,
//! `HasLab` o `Ext`. Para la asignación falsificante concreta, el core es el
//! conjunto canónico de hechos extensionales del snapshot que hacen verdaderos
//! esos átomos. No se intenta satisfacer fórmulas globales ni inferir conflictos.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::atomos::{Atomo, IdArista, Sujeto as SujetoAtomo};
use crate::estructura::Estructura;
use crate::evaluador::{
    AsignacionCanonica, InstanciaCuantificada, MAX_LIGADURAS_POR_CUANTIFICADOR, ValorLigadura,
};
use crate::formula::{
    Formula, SorteVar, SujetoTermino, TerminoArista, TerminoConstExt, TerminoEtiqueta, TerminoNodo,
    TerminoSorte, Var,
};
use crate::path::BundlePath;
use crate::tipos::{Etiqueta, Sorte};

use super::PresupuestoExplicacion;

/// Salida local de extracción. `NoAplicable` no altera el rechazo ni implica
/// que la fórmula sea satisfacible globalmente.
#[derive(Clone, PartialEq, Eq)]
pub(super) enum ResultadoPresencia {
    Core(CorePresencia),
    NoAplicable,
}

impl fmt::Debug for ResultadoPresencia {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Core(core) => f.debug_tuple("Core").field(core).finish(),
            Self::NoAplicable => f.write_str("NoAplicable"),
        }
    }
}

/// Hechos positivos de un core, ordenados por el orden canónico del snapshot.
/// Los valores son internos: esta implementación no los muestra por `Debug`
/// ni ofrece `Display`.
#[derive(Clone, PartialEq, Eq)]
pub struct CorePresencia {
    hechos: Vec<Atomo>,
}

impl CorePresencia {
    /// Devuelve los hechos sensibles del core. No usar para `Debug` ni para
    /// logging general: paths, etiquetas, sortes y valores pueden revelar
    /// contenido privado del bundle.
    pub fn atomos_sensibles(&self) -> &[Atomo] {
        &self.hechos
    }
}

impl fmt::Debug for CorePresencia {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CorePresencia")
            .field("cantidad_hechos", &self.hechos.len())
            .field("hechos", &"[redactados]")
            .finish()
    }
}

impl fmt::Display for CorePresencia {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "presence_core ({} hechos redactados)", self.hechos.len())
    }
}

/// Único error del extractor. No conserva la fórmula, los términos ni el
/// snapshot; el agregador convierte el límite agotado en error de recurso.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ErrorPresencia {
    PresupuestoAgotado { limite: u64 },
}

impl fmt::Debug for ErrorPresencia {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PresupuestoAgotado { limite } => f
                .debug_struct("PresupuestoPresenciaAgotado")
                .field("limite", limite)
                .finish(),
        }
    }
}

impl fmt::Display for ErrorPresencia {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PresupuestoAgotado { .. } => {
                f.write_str("se agotó el presupuesto de explicación")
            }
        }
    }
}

impl std::error::Error for ErrorPresencia {}

/// Extrae un core subconjunto-minimal para una asignación universal fallida.
///
/// Solo devuelve un core cuando la forma sintáctica, la instancia de traza y
/// los hechos del snapshot coinciden con el fragmento documentado. La
/// eliminación se ejecuta en orden canónico y usa el presupuesto global de
/// explicación compartido con el agregador. Si el fragmento no aplica, el
/// resultado es `NoAplicable`; nunca se intenta SAT/implicación global.
pub(super) fn extraer_core_presencia(
    formula: &Formula,
    instancia: &InstanciaCuantificada,
    estructura: &Estructura,
    presupuesto: &mut PresupuestoExplicacion,
) -> Result<ResultadoPresencia, ErrorPresencia> {
    consumir(presupuesto, 1)?;

    let Formula::ParaTodo {
        ligaduras,
        guarda,
        cuerpo,
    } = formula
    else {
        return Ok(ResultadoPresencia::NoAplicable);
    };
    if ligaduras.len() > MAX_LIGADURAS_POR_CUANTIFICADOR {
        return Ok(ResultadoPresencia::NoAplicable);
    }
    if !traza_es_instancia_fallida(instancia, guarda.is_some(), presupuesto)? {
        return Ok(ResultadoPresencia::NoAplicable);
    }

    let Some(guardia) = componentes_guardia(guarda.as_deref()) else {
        return Ok(ResultadoPresencia::NoAplicable);
    };
    let Some(patron) = componentes_patron(cuerpo) else {
        return Ok(ResultadoPresencia::NoAplicable);
    };
    for atomo in guardia.iter().chain(patron.iter()) {
        consumir(presupuesto, 1)?;
        if !es_atomo_positivo(atomo) {
            return Ok(ResultadoPresencia::NoAplicable);
        }
    }
    if !asignacion_corresponde(ligaduras, &instancia.asignacion, presupuesto)? {
        return Ok(ResultadoPresencia::NoAplicable);
    }

    let snapshot = cargar_snapshot(estructura, presupuesto)?;
    let mut requeridos = BTreeSet::new();
    let mut hechos = BTreeMap::new();
    for formula_atomica in guardia.iter().chain(patron.iter()) {
        consumir(presupuesto, 1)?;
        let Some(atomico) = instanciar_atomo(formula_atomica, &instancia.asignacion, presupuesto)?
        else {
            return Ok(ResultadoPresencia::NoAplicable);
        };
        let Some(indice) = buscar_en_snapshot(&atomico, &snapshot, presupuesto)? else {
            return Ok(ResultadoPresencia::NoAplicable);
        };
        consumir(presupuesto, factor_orden(hechos.len().saturating_add(1)))?;
        requeridos.insert(indice);
        hechos.entry(indice).or_insert(atomico.a_atomo());
    }

    let requeridos: Vec<usize> = requeridos.into_iter().collect();
    let mut core = requeridos.clone();
    if !condicion_sigue_falsa(&requeridos, &core, presupuesto)? {
        return Ok(ResultadoPresencia::NoAplicable);
    }

    // Eliminación determinista. En este fragmento extensional, cada átomo
    // ground tiene un único hecho exacto de soporte; duplicados sintácticos
    // ya se fusionaron por índice canónico. La comprobación se mantiene
    // explícita para hacer verificable la minimalidad por inclusión.
    let mut cursor = 0;
    while cursor < core.len() {
        consumir(presupuesto, 1)?;
        let removido = core.remove(cursor);
        let suficiente = condicion_sigue_falsa(&requeridos, &core, presupuesto)?;
        if !suficiente {
            core.insert(cursor, removido);
            cursor += 1;
        }
    }

    let mut hechos_core = Vec::with_capacity(core.len());
    for indice in core {
        let Some(hecho) = hechos.remove(&indice) else {
            return Ok(ResultadoPresencia::NoAplicable);
        };
        hechos_core.push(hecho);
    }
    Ok(ResultadoPresencia::Core(CorePresencia {
        hechos: hechos_core,
    }))
}

fn consumir(presupuesto: &mut PresupuestoExplicacion, unidades: u64) -> Result<(), ErrorPresencia> {
    presupuesto
        .consumir(unidades)
        .map_err(|limite| ErrorPresencia::PresupuestoAgotado { limite })
}

fn traza_es_instancia_fallida(
    instancia: &InstanciaCuantificada,
    tiene_guardia: bool,
    presupuesto: &mut PresupuestoExplicacion,
) -> Result<bool, ErrorPresencia> {
    // Se cobra antes de leer los campos de la instancia; cada subevidencia
    // que se inspecciona se cobra antes de leer su veredicto.
    consumir(presupuesto, 1)?;
    if instancia.valor {
        return Ok(false);
    }
    consumir(presupuesto, 1)?;
    let Some(cuerpo) = instancia.cuerpo.as_deref() else {
        return Ok(false);
    };
    if cuerpo.valor {
        return Ok(false);
    }
    if tiene_guardia {
        consumir(presupuesto, 1)?;
        let Some(guarda) = instancia.guarda.as_deref() else {
            return Ok(false);
        };
        if !guarda.valor {
            return Ok(false);
        }
    } else if instancia.guarda.is_some() {
        return Ok(false);
    }
    Ok(true)
}

#[derive(Clone, Copy)]
struct Componentes<'a> {
    lista: Option<&'a [Formula]>,
    unico: Option<&'a Formula>,
}

impl<'a> Componentes<'a> {
    fn iter(&self) -> impl Iterator<Item = &'a Formula> + '_ {
        self.lista
            .into_iter()
            .flat_map(|elementos| elementos.iter())
            .chain(self.unico)
    }
}

fn componentes_guardia(formula: Option<&Formula>) -> Option<Componentes<'_>> {
    match formula {
        None => Some(Componentes {
            lista: None,
            unico: None,
        }),
        Some(Formula::Y(partes)) if !partes.is_empty() => Some(Componentes {
            lista: Some(partes),
            unico: None,
        }),
        Some(atomo) if es_atomo_positivo(atomo) => Some(Componentes {
            lista: None,
            unico: Some(atomo),
        }),
        _ => None,
    }
}

fn componentes_patron(formula: &Formula) -> Option<Componentes<'_>> {
    let Formula::No(interior) = formula else {
        return None;
    };
    let Formula::Y(partes) = interior.as_ref() else {
        return None;
    };
    (!partes.is_empty()).then_some(Componentes {
        lista: Some(partes),
        unico: None,
    })
}

fn es_atomo_positivo(formula: &Formula) -> bool {
    match formula {
        Formula::Type { sorte, .. } | Formula::Edge { sorte, .. } => {
            matches!(sorte, TerminoSorte::Literal(_))
        }
        Formula::Parent { .. } => true,
        Formula::HasLab { etiqueta, .. } => matches!(etiqueta, TerminoEtiqueta::Literal(_)),
        Formula::Ext { constante, .. } => matches!(constante, TerminoConstExt::Literal(_)),
        _ => false,
    }
}

fn asignacion_corresponde(
    ligaduras: &[(Var, SorteVar)],
    asignacion: &AsignacionCanonica,
    presupuesto: &mut PresupuestoExplicacion,
) -> Result<bool, ErrorPresencia> {
    if ligaduras.len() != asignacion.ligaduras.len() {
        return Ok(false);
    }
    let mut variables = BTreeSet::new();
    for (variable, sorte) in ligaduras {
        let costo_insercion = largo(variable.como_str().len())
            .saturating_mul(factor_orden(ligaduras.len()))
            .saturating_add(1);
        consumir(presupuesto, costo_insercion)?;
        if !variables.insert(variable.clone()) {
            return Ok(false);
        }
        let mut encontradas = 0usize;
        let mut coincide = false;
        for ligadura in &asignacion.ligaduras {
            comparar_variables(presupuesto, variable, &ligadura.variable)?;
            if ligadura.variable != *variable {
                continue;
            }
            encontradas += 1;
            coincide = matches!(
                (sorte, &ligadura.valor),
                (SorteVar::Nodo, ValorLigadura::Nodo(_))
                    | (SorteVar::Arista, ValorLigadura::Arista(_))
            );
        }
        if encontradas != 1 || !coincide {
            return Ok(false);
        }
    }
    Ok(true)
}

enum SujetoRef<'a> {
    Nodo(&'a BundlePath),
    Arista(&'a IdArista),
}

enum AtomoRef<'a> {
    Type {
        sujeto: &'a BundlePath,
        sorte: &'a Sorte,
    },
    Edge {
        id: &'a IdArista,
        origen: &'a BundlePath,
        destino: &'a BundlePath,
        sorte: &'a Sorte,
    },
    Parent {
        hijo: &'a BundlePath,
        padre: &'a BundlePath,
    },
    HasLab {
        sujeto: SujetoRef<'a>,
        etiqueta: &'a Etiqueta,
    },
    Ext {
        predicado: &'a str,
        sujeto: &'a BundlePath,
        constante: &'a str,
    },
}

impl AtomoRef<'_> {
    fn a_atomo(&self) -> Atomo {
        match self {
            Self::Type { sujeto, sorte } => Atomo::Type {
                sujeto: (*sujeto).clone(),
                sorte: (*sorte).clone(),
            },
            Self::Edge {
                id,
                origen,
                destino,
                sorte,
            } => Atomo::Edge {
                id: (*id).clone(),
                origen: (*origen).clone(),
                destino: (*destino).clone(),
                sorte: (*sorte).clone(),
            },
            Self::Parent { hijo, padre } => Atomo::Parent {
                hijo: (*hijo).clone(),
                padre: (*padre).clone(),
            },
            Self::HasLab { sujeto, etiqueta } => Atomo::HasLab {
                sujeto: match sujeto {
                    SujetoRef::Nodo(path) => SujetoAtomo::Nodo((*path).clone()),
                    SujetoRef::Arista(id) => SujetoAtomo::Arista((*id).clone()),
                },
                etiqueta: (*etiqueta).clone(),
            },
            Self::Ext {
                predicado,
                sujeto,
                constante,
            } => Atomo::Ext {
                predicado: (*predicado).to_string(),
                sujeto: (*sujeto).clone(),
                constante: (*constante).to_string(),
            },
        }
    }

    fn cota_formato(&self) -> u64 {
        match self {
            Self::Type { sujeto, sorte } => {
                suma_largos(&["Type(", sujeto.como_str(), ", ", sorte.como_str(), ")"])
            }
            Self::Edge {
                id,
                origen,
                destino,
                sorte,
            } => cota_edge_id(id, origen, destino, sorte),
            Self::Parent { hijo, padre } => {
                suma_largos(&["Parent(", hijo.como_str(), ", ", padre.como_str(), ")"])
            }
            Self::HasLab { sujeto, etiqueta } => {
                let largo_sujeto = match sujeto {
                    SujetoRef::Nodo(path) => largo(path.como_str().len()),
                    SujetoRef::Arista(id) => {
                        cota_id(&id.origen, &id.sorte, &id.destino, id.ordinal)
                    }
                };
                // Cota segura para la forma escapada: cada byte podría duplicarse.
                largo("HasLab(".len())
                    .saturating_add(largo_sujeto)
                    .saturating_add(largo(", \"".len()))
                    .saturating_add(largo(etiqueta.como_str().len()).saturating_mul(2))
                    .saturating_add(largo("\")".len()))
            }
            Self::Ext {
                predicado,
                sujeto,
                constante,
            } => suma_largos(&[predicado, "(", sujeto.como_str(), ", ", constante, ")"]),
        }
    }
}

fn instanciar_atomo<'a>(
    formula: &'a Formula,
    asignacion: &'a AsignacionCanonica,
    presupuesto: &mut PresupuestoExplicacion,
) -> Result<Option<AtomoRef<'a>>, ErrorPresencia> {
    let resultado = match formula {
        Formula::Type {
            sujeto,
            sorte: TerminoSorte::Literal(sorte),
        } => resolver_nodo(sujeto, asignacion, presupuesto)?
            .map(|sujeto| AtomoRef::Type { sujeto, sorte }),
        Formula::Edge {
            arista,
            origen,
            destino,
            sorte: TerminoSorte::Literal(sorte),
        } => match (
            resolver_arista(arista, asignacion, presupuesto)?,
            resolver_nodo(origen, asignacion, presupuesto)?,
            resolver_nodo(destino, asignacion, presupuesto)?,
        ) {
            (Some(id), Some(origen), Some(destino)) => Some(AtomoRef::Edge {
                id,
                origen,
                destino,
                sorte,
            }),
            _ => None,
        },
        Formula::Parent { hijo, padre } => match (
            resolver_nodo(hijo, asignacion, presupuesto)?,
            resolver_nodo(padre, asignacion, presupuesto)?,
        ) {
            (Some(hijo), Some(padre)) => Some(AtomoRef::Parent { hijo, padre }),
            _ => None,
        },
        Formula::HasLab {
            sujeto,
            etiqueta: TerminoEtiqueta::Literal(etiqueta),
        } => {
            let sujeto = match sujeto {
                SujetoTermino::Nodo(termino) => {
                    resolver_nodo(termino, asignacion, presupuesto)?.map(SujetoRef::Nodo)
                }
                SujetoTermino::Arista(termino) => {
                    resolver_arista(termino, asignacion, presupuesto)?.map(SujetoRef::Arista)
                }
            };
            sujeto.map(|sujeto| AtomoRef::HasLab { sujeto, etiqueta })
        }
        Formula::Ext {
            predicado,
            sujeto,
            constante: TerminoConstExt::Literal(constante),
        } => resolver_nodo(sujeto, asignacion, presupuesto)?.map(|sujeto| AtomoRef::Ext {
            predicado,
            sujeto,
            constante,
        }),
        _ => None,
    };
    Ok(resultado)
}

fn resolver_nodo<'a>(
    termino: &'a TerminoNodo,
    asignacion: &'a AsignacionCanonica,
    presupuesto: &mut PresupuestoExplicacion,
) -> Result<Option<&'a BundlePath>, ErrorPresencia> {
    match termino {
        TerminoNodo::Constante(path) => Ok(Some(path)),
        TerminoNodo::Var(variable) => {
            let mut resultado = None;
            for ligadura in &asignacion.ligaduras {
                comparar_variables(presupuesto, variable, &ligadura.variable)?;
                if ligadura.variable == *variable {
                    match &ligadura.valor {
                        ValorLigadura::Nodo(path) if resultado.is_none() => resultado = Some(path),
                        _ => return Ok(None),
                    }
                }
            }
            Ok(resultado)
        }
    }
}

fn resolver_arista<'a>(
    termino: &'a TerminoArista,
    asignacion: &'a AsignacionCanonica,
    presupuesto: &mut PresupuestoExplicacion,
) -> Result<Option<&'a IdArista>, ErrorPresencia> {
    match termino {
        TerminoArista::Constante(id) => Ok(Some(id)),
        TerminoArista::Var(variable) => {
            let mut resultado = None;
            for ligadura in &asignacion.ligaduras {
                comparar_variables(presupuesto, variable, &ligadura.variable)?;
                if ligadura.variable == *variable {
                    match &ligadura.valor {
                        ValorLigadura::Arista(id) if resultado.is_none() => resultado = Some(id),
                        _ => return Ok(None),
                    }
                }
            }
            Ok(resultado)
        }
    }
}

fn buscar_en_snapshot(
    atomo: &AtomoRef<'_>,
    snapshot: &[Atomo],
    presupuesto: &mut PresupuestoExplicacion,
) -> Result<Option<usize>, ErrorPresencia> {
    let bytes = atomo.cota_formato();
    consumir(presupuesto, 1u64.saturating_add(bytes))?;
    let atomo = atomo.a_atomo();
    let costo_busqueda = bytes.saturating_mul(factor_orden(snapshot.len()));
    consumir(presupuesto, costo_busqueda)?;
    Ok(snapshot.binary_search(&atomo).ok())
}

fn condicion_sigue_falsa(
    requeridos: &[usize],
    subconjunto: &[usize],
    presupuesto: &mut PresupuestoExplicacion,
) -> Result<bool, ErrorPresencia> {
    let costo_por_busqueda = factor_orden(subconjunto.len());
    let costo_total = largo(requeridos.len()).saturating_mul(costo_por_busqueda);
    consumir(presupuesto, costo_total)?;
    Ok(requeridos
        .iter()
        .all(|requerido| subconjunto.binary_search(requerido).is_ok()))
}

fn cargar_snapshot(
    estructura: &Estructura,
    presupuesto: &mut PresupuestoExplicacion,
) -> Result<Vec<Atomo>, ErrorPresencia> {
    let mut cantidad = 0u64;
    let mut bytes = 0u64;

    for (id, nodo) in estructura.nodos() {
        consumir(presupuesto, 2)?;
        cantidad = cantidad.saturating_add(2);
        bytes = bytes
            .saturating_add(cota_type(&nodo.path, &nodo.tipo))
            .saturating_add(cota_haslab_nodo(&nodo.path, &nodo.etiqueta));
        if let Some(padre) = estructura.padre(id) {
            let Some(padre) = estructura.nodo(padre) else {
                return Ok(Vec::new());
            };
            consumir(presupuesto, 1)?;
            cantidad = cantidad.saturating_add(1);
            bytes = bytes.saturating_add(cota_parent(&nodo.path, &padre.path));
        }
        for (predicado, constante) in estructura.atributos(id) {
            consumir(presupuesto, 1)?;
            cantidad = cantidad.saturating_add(1);
            bytes = bytes.saturating_add(cota_ext(predicado, &nodo.path, constante));
        }
    }
    for (_, arista) in estructura.aristas() {
        let (Some(origen), Some(destino)) = (
            estructura.nodo(arista.origen),
            estructura.nodo(arista.destino),
        ) else {
            return Ok(Vec::new());
        };
        consumir(presupuesto, 2)?;
        cantidad = cantidad.saturating_add(2);
        bytes = bytes
            .saturating_add(cota_edge(
                &origen.path,
                &arista.tipo,
                &destino.path,
                arista.ordinal,
            ))
            .saturating_add(cota_haslab_arista(
                &origen.path,
                &arista.tipo,
                &destino.path,
                arista.ordinal,
                &arista.etiqueta,
            ));
    }

    // Cubre ordenamiento y comparaciones de cadenas canónicas antes de que
    // `Estructura::atomos` clone y ordene el snapshot completo.
    let factor = factor_orden(usize::try_from(cantidad).unwrap_or(usize::MAX));
    consumir(
        presupuesto,
        bytes
            .saturating_mul(factor.saturating_add(1))
            .saturating_add(cantidad.saturating_mul(factor)),
    )?;
    Ok(estructura.atomos())
}

fn cota_type(path: &BundlePath, sorte: &Sorte) -> u64 {
    suma_largos(&["Type(", path.como_str(), ", ", sorte.como_str(), ")"])
}

fn cota_parent(hijo: &BundlePath, padre: &BundlePath) -> u64 {
    suma_largos(&["Parent(", hijo.como_str(), ", ", padre.como_str(), ")"])
}

fn cota_haslab_nodo(path: &BundlePath, etiqueta: &Etiqueta) -> u64 {
    cota_haslab_largo(largo(path.como_str().len()), etiqueta)
}

fn cota_haslab_arista(
    origen: &BundlePath,
    sorte: &Sorte,
    destino: &BundlePath,
    ordinal: u32,
    etiqueta: &Etiqueta,
) -> u64 {
    let id = cota_id(origen, sorte, destino, ordinal);
    cota_haslab_largo(id, etiqueta)
}

fn cota_haslab_largo(sujeto: u64, etiqueta: &Etiqueta) -> u64 {
    largo("HasLab(".len())
        .saturating_add(sujeto)
        .saturating_add(largo(", \"".len()))
        .saturating_add(largo(etiqueta.como_str().len()).saturating_mul(2))
        .saturating_add(largo("\")".len()))
}

fn cota_edge(origen: &BundlePath, sorte: &Sorte, destino: &BundlePath, ordinal: u32) -> u64 {
    cota_edge_largo(
        cota_id(origen, sorte, destino, ordinal),
        origen,
        destino,
        sorte,
    )
}

fn cota_edge_id(id: &IdArista, origen: &BundlePath, destino: &BundlePath, sorte: &Sorte) -> u64 {
    cota_edge_largo(
        cota_id(&id.origen, &id.sorte, &id.destino, id.ordinal),
        origen,
        destino,
        sorte,
    )
}

fn cota_edge_largo(id: u64, origen: &BundlePath, destino: &BundlePath, sorte: &Sorte) -> u64 {
    largo("Edge(".len())
        .saturating_add(id)
        .saturating_add(largo(", ".len()))
        .saturating_add(largo(origen.como_str().len()))
        .saturating_add(largo(", ".len()))
        .saturating_add(largo(destino.como_str().len()))
        .saturating_add(largo(", ".len()))
        .saturating_add(largo(sorte.como_str().len()))
        .saturating_add(largo(")".len()))
}

fn cota_ext(predicado: &str, sujeto: &BundlePath, constante: &str) -> u64 {
    suma_largos(&[predicado, "(", sujeto.como_str(), ", ", constante, ")"])
}

fn cota_id(origen: &BundlePath, sorte: &Sorte, destino: &BundlePath, ordinal: u32) -> u64 {
    largo(origen.como_str().len())
        .saturating_add(largo("→".len()))
        .saturating_add(largo(destino.como_str().len()))
        .saturating_add(largo("#".len()))
        .saturating_add(largo(sorte.como_str().len()))
        .saturating_add(largo("#".len()))
        .saturating_add(largo(decimal_u32(ordinal)))
}

fn decimal_u32(mut valor: u32) -> usize {
    let mut largo = 1;
    while valor >= 10 {
        valor /= 10;
        largo += 1;
    }
    largo
}

fn suma_largos(partes: &[&str]) -> u64 {
    partes.iter().fold(0u64, |total, parte| {
        total.saturating_add(largo(parte.len()))
    })
}

fn largo(valor: usize) -> u64 {
    u64::try_from(valor).unwrap_or(u64::MAX)
}

fn comparar_variables(
    presupuesto: &mut PresupuestoExplicacion,
    izquierda: &Var,
    derecha: &Var,
) -> Result<(), ErrorPresencia> {
    let bytes = largo(izquierda.como_str().len()).saturating_add(largo(derecha.como_str().len()));
    consumir(presupuesto, 1u64.saturating_add(bytes))
}

fn factor_orden(cantidad: usize) -> u64 {
    if cantidad <= 1 {
        return 1;
    }
    u64::from(usize::BITS - cantidad.leading_zeros())
}

#[cfg(test)]
#[path = "presencia/tests.rs"]
mod tests;
