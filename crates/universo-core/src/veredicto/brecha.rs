//! Resumen seguro de una obligación existencial positiva sin testigo.
//!
//! Este extractor no propone reparaciones ni inventa elementos del dominio.
//! Solo reconoce existenciales falsos y el fragmento `forall`/`exists`
//! especificado; el contraejemplo trazable sigue siendo la explicación base.

use std::fmt;

use crate::estructura::Estructura;
use crate::evaluador::{
    AsignacionCanonica, EvidenciaEvaluacion, InstanciaCuantificada, ReglaEvidencia,
    TipoCuantificador, ValorLigadura,
};
use crate::formula::{Formula, SorteVar};

use super::PresupuestoExplicacion;

/// Clase de obligación existencial que no encontró testigo en el snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaseBrecha {
    ExistenciaSinTestigo,
    ParaTodoExisteSinTestigo,
}

/// Constructor atómico allowlisted; no conserva términos, nombres ni literales.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtomoBrecha {
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

/// Forma sintáctica segura de una obligación positiva monotónica.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormaPositiva {
    Atomo(AtomoBrecha),
    Y(Vec<FormaPositiva>),
    O(Vec<FormaPositiva>),
    Existe {
        ligaduras_nodo: u64,
        ligaduras_arista: u64,
        guarda: Option<Box<FormaPositiva>>,
        cuerpo: Box<FormaPositiva>,
    },
}

/// Forma allowlisted del existencial cuya ausencia se resume.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormaBrecha {
    pub ligaduras_nodo: u64,
    pub ligaduras_arista: u64,
    pub guarda: Option<FormaPositiva>,
    pub cuerpo: FormaPositiva,
}

/// Conteos del snapshot y de los cuantificadores principales resumidos.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConteosBrecha {
    pub nodos_disponibles: u64,
    pub aristas_disponibles: u64,
    /// Candidatos evaluados por el existencial que constituye el gap; no suma
    /// cuantificadores anidados ni bindings de otras fórmulas/instancias.
    pub candidatos_existencial_principal_evaluados: u64,
    pub asignaciones_universales_examinadas: u64,
}

/// Brecha estructurada. La asignación universal, cuando existe, es sensible.
#[derive(Clone, PartialEq, Eq)]
pub struct Brecha {
    pub clase: ClaseBrecha,
    pub conteos: ConteosBrecha,
    pub forma: FormaBrecha,
    asignacion_universal: Option<AsignacionCanonica>,
}

impl Brecha {
    /// Devuelve la asignación concreta seleccionada por `ParaTodo`.
    ///
    /// **Dato sensible:** contiene IDs/rutas de nodos y aristas del bundle.
    /// No debe enviarse a logs, telemetría ni salidas por defecto.
    pub fn asignacion_universal_sensible(&self) -> Option<&AsignacionCanonica> {
        self.asignacion_universal.as_ref()
    }
}

impl fmt::Debug for Brecha {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Brecha")
            .field("clase", &self.clase)
            .field("conteos", &self.conteos)
            .field("forma", &self.forma)
            .field(
                "tiene_asignacion_universal_sensible",
                &self.asignacion_universal.is_some(),
            )
            .finish()
    }
}

impl fmt::Display for Brecha {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let clase = match self.clase {
            ClaseBrecha::ExistenciaSinTestigo => "existencia sin testigo",
            ClaseBrecha::ParaTodoExisteSinTestigo => "instancia universal sin testigo",
        };
        write!(f, "brecha: {clase} (datos concretos redactados)")
    }
}

/// Agotamiento del presupuesto compartido; no conserva la fórmula ni los datos.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ErrorBrecha {
    PresupuestoAgotado { limite: u64 },
}

impl fmt::Debug for ErrorBrecha {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PresupuestoAgotado { limite } => f
                .debug_struct("PresupuestoBrechaAgotado")
                .field("limite", limite)
                .finish(),
        }
    }
}

impl fmt::Display for ErrorBrecha {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("se agotó el presupuesto de explicación")
    }
}

impl std::error::Error for ErrorBrecha {}

/// Extrae un resumen cuando la fórmula y su traza prueban la ausencia de un
/// testigo para una obligación positiva monotónica. Las formas fuera del
/// fragmento devuelven `None`; el llamador conserva el contraejemplo.
pub(super) fn extraer_brecha(
    formula: &Formula,
    evidencia: &EvidenciaEvaluacion,
    estructura: &Estructura,
    presupuesto: &mut PresupuestoExplicacion,
) -> Result<Option<Brecha>, ErrorBrecha> {
    consumir(presupuesto, 1)?;
    if evidencia.valor {
        return Ok(None);
    }

    let (clase, formula_existencial, evidencia_existencial, asignacion, universales) =
        match (formula, &evidencia.regla) {
            (
                Formula::Existe { .. },
                ReglaEvidencia::Cuantificador {
                    tipo: TipoCuantificador::Existe,
                    ..
                },
            ) => (
                ClaseBrecha::ExistenciaSinTestigo,
                formula,
                evidencia,
                None,
                0,
            ),
            (
                Formula::ParaTodo {
                    ligaduras,
                    guarda,
                    cuerpo,
                },
                ReglaEvidencia::Cuantificador {
                    tipo: TipoCuantificador::ParaTodo,
                    instancias,
                    ..
                },
            ) => {
                let Formula::Existe { .. } = cuerpo.as_ref() else {
                    return Ok(None);
                };
                let (instancia, examinadas) = match primera_instancia_aplicable_fallida(
                    ligaduras,
                    guarda.is_some(),
                    instancias,
                    presupuesto,
                )? {
                    Some(resultado) => resultado,
                    None => return Ok(None),
                };
                let Some(cuerpo_evidencia) = instancia.cuerpo.as_deref() else {
                    return Ok(None);
                };
                if cuerpo_evidencia.valor {
                    return Ok(None);
                }
                (
                    ClaseBrecha::ParaTodoExisteSinTestigo,
                    cuerpo.as_ref(),
                    cuerpo_evidencia,
                    Some(&instancia.asignacion),
                    examinadas,
                )
            }
            _ => return Ok(None),
        };

    let Formula::Existe {
        ligaduras,
        guarda,
        cuerpo,
    } = formula_existencial
    else {
        return Ok(None);
    };
    let instancias = match cuantificador_existencial_agotado(
        evidencia_existencial,
        ligaduras,
        guarda.is_some(),
        presupuesto,
    )? {
        Some(instancias) => instancias,
        None => return Ok(None),
    };
    let forma = match forma_existencial(ligaduras, guarda.as_deref(), cuerpo, presupuesto)? {
        Some(forma) => forma,
        None => return Ok(None),
    };

    consumir(presupuesto, 1)?;
    let asignacion_universal = match asignacion {
        Some(asignacion) => {
            consumir(presupuesto, costo_clon_asignacion(asignacion, presupuesto)?)?;
            Some(asignacion.clone())
        }
        None => None,
    };
    let nodos_disponibles = costo_usize(estructura.len_nodos(), presupuesto)?;
    let aristas_disponibles = costo_usize(estructura.len_aristas(), presupuesto)?;
    Ok(Some(Brecha {
        clase,
        conteos: ConteosBrecha {
            nodos_disponibles,
            aristas_disponibles,
            candidatos_existencial_principal_evaluados: costo_usize(instancias.len(), presupuesto)?,
            asignaciones_universales_examinadas: universales,
        },
        forma,
        asignacion_universal,
    }))
}

fn primera_instancia_aplicable_fallida<'a>(
    ligaduras: &[(crate::formula::Var, SorteVar)],
    tiene_guarda: bool,
    instancias: &'a [InstanciaCuantificada],
    presupuesto: &mut PresupuestoExplicacion,
) -> Result<Option<(&'a InstanciaCuantificada, u64)>, ErrorBrecha> {
    let mut examinadas = 0u64;
    for instancia in instancias {
        consumir(presupuesto, 1)?;
        examinadas = examinadas
            .checked_add(1)
            .ok_or(ErrorBrecha::PresupuestoAgotado {
                limite: presupuesto.limite,
            })?;
        if !asignacion_corresponde(ligaduras, &instancia.asignacion, presupuesto)? {
            return Ok(None);
        }
        let aplicable = if tiene_guarda {
            consumir(presupuesto, 1)?;
            let Some(guarda) = instancia.guarda.as_deref() else {
                return Ok(None);
            };
            guarda.valor
        } else {
            if instancia.guarda.is_some() {
                return Ok(None);
            }
            true
        };
        if !aplicable {
            consumir(presupuesto, 1)?;
            if !instancia.valor || instancia.cuerpo.is_some() {
                return Ok(None);
            }
            continue;
        }
        consumir(presupuesto, 1)?;
        let Some(cuerpo) = instancia.cuerpo.as_deref() else {
            return Ok(None);
        };
        if instancia.valor {
            if !cuerpo.valor {
                return Ok(None);
            }
            continue;
        }
        if cuerpo.valor {
            return Ok(None);
        }
        return Ok(Some((instancia, examinadas)));
    }
    Ok(None)
}

fn cuantificador_existencial_agotado<'a>(
    evidencia: &'a EvidenciaEvaluacion,
    ligaduras: &[(crate::formula::Var, SorteVar)],
    tiene_guarda: bool,
    presupuesto: &mut PresupuestoExplicacion,
) -> Result<Option<&'a [InstanciaCuantificada]>, ErrorBrecha> {
    if evidencia.valor {
        return Ok(None);
    }
    let ReglaEvidencia::Cuantificador {
        tipo: TipoCuantificador::Existe,
        instancias,
        busqueda_agotada: true,
    } = &evidencia.regla
    else {
        return Ok(None);
    };
    for instancia in instancias {
        consumir(presupuesto, 1)?;
        if instancia.valor
            || !asignacion_corresponde(ligaduras, &instancia.asignacion, presupuesto)?
        {
            return Ok(None);
        }
        match (tiene_guarda, instancia.guarda.as_deref()) {
            (false, None) => {}
            (true, Some(guarda)) => {
                consumir(presupuesto, 1)?;
                if !guarda.valor {
                    consumir(presupuesto, 1)?;
                    if instancia.cuerpo.is_some() {
                        return Ok(None);
                    }
                    continue;
                }
            }
            _ => return Ok(None),
        }
        consumir(presupuesto, 1)?;
        let Some(cuerpo) = instancia.cuerpo.as_deref() else {
            return Ok(None);
        };
        if cuerpo.valor {
            return Ok(None);
        }
    }
    Ok(Some(instancias))
}

fn asignacion_corresponde(
    ligaduras: &[(crate::formula::Var, SorteVar)],
    asignacion: &AsignacionCanonica,
    presupuesto: &mut PresupuestoExplicacion,
) -> Result<bool, ErrorBrecha> {
    consumir(presupuesto, costo_usize(ligaduras.len(), presupuesto)?)?;
    if ligaduras.len() != asignacion.ligaduras.len() {
        return Ok(false);
    }
    for ((variable, sorte), asignada) in ligaduras.iter().zip(&asignacion.ligaduras) {
        consumir(
            presupuesto,
            1u64.saturating_add(costo_usize(variable.como_str().len(), presupuesto)?),
        )?;
        let coincide = matches!(
            (sorte, &asignada.valor),
            (SorteVar::Nodo, ValorLigadura::Nodo(_)) | (SorteVar::Arista, ValorLigadura::Arista(_))
        );
        if variable != &asignada.variable || !coincide {
            return Ok(false);
        }
    }
    Ok(true)
}

fn forma_existencial(
    ligaduras: &[(crate::formula::Var, SorteVar)],
    guarda: Option<&Formula>,
    cuerpo: &Formula,
    presupuesto: &mut PresupuestoExplicacion,
) -> Result<Option<FormaBrecha>, ErrorBrecha> {
    consumir(
        presupuesto,
        costo_usize(ligaduras.len(), presupuesto)?.saturating_add(1),
    )?;
    let (ligaduras_nodo, ligaduras_arista) = contar_sortes(ligaduras);
    let guarda = match guarda {
        Some(guarda) => match forma_positiva(guarda, presupuesto)? {
            Some(forma) => Some(forma),
            None => return Ok(None),
        },
        None => None,
    };
    let Some(cuerpo) = forma_positiva(cuerpo, presupuesto)? else {
        return Ok(None);
    };
    consumir(presupuesto, 3)?;
    Ok(Some(FormaBrecha {
        ligaduras_nodo,
        ligaduras_arista,
        guarda,
        cuerpo,
    }))
}

fn forma_positiva(
    formula: &Formula,
    presupuesto: &mut PresupuestoExplicacion,
) -> Result<Option<FormaPositiva>, ErrorBrecha> {
    consumir(presupuesto, 1)?;
    let atomo = match formula {
        Formula::Type { .. } => Some(AtomoBrecha::Type),
        Formula::Edge { .. } => Some(AtomoBrecha::Edge),
        Formula::Parent { .. } => Some(AtomoBrecha::Parent),
        Formula::HasLab { sujeto, .. } => Some(match sujeto {
            crate::formula::SujetoTermino::Nodo(_) => AtomoBrecha::HasLabNodo,
            crate::formula::SujetoTermino::Arista(_) => AtomoBrecha::HasLabArista,
        }),
        Formula::Ext { .. } => Some(AtomoBrecha::Ext),
        Formula::TcParent { .. } => Some(AtomoBrecha::TcParent),
        Formula::IgualNodo(..) => Some(AtomoBrecha::IgualNodo),
        Formula::IgualArista(..) => Some(AtomoBrecha::IgualArista),
        _ => None,
    };
    if let Some(atomo) = atomo {
        return Ok(Some(FormaPositiva::Atomo(atomo)));
    }

    match formula {
        Formula::Y(partes) | Formula::O(partes) => {
            consumir(presupuesto, costo_usize(partes.len(), presupuesto)?)?;
            let mut formas = Vec::with_capacity(partes.len());
            for parte in partes {
                let Some(forma) = forma_positiva(parte, presupuesto)? else {
                    return Ok(None);
                };
                formas.push(forma);
            }
            Ok(Some(if matches!(formula, Formula::Y(_)) {
                FormaPositiva::Y(formas)
            } else {
                FormaPositiva::O(formas)
            }))
        }
        Formula::Existe {
            ligaduras,
            guarda,
            cuerpo,
        } => {
            consumir(
                presupuesto,
                costo_usize(ligaduras.len(), presupuesto)?.saturating_add(1),
            )?;
            let (ligaduras_nodo, ligaduras_arista) = contar_sortes(ligaduras);
            let guarda = match guarda.as_deref() {
                Some(guarda) => {
                    let Some(forma) = forma_positiva(guarda, presupuesto)? else {
                        return Ok(None);
                    };
                    consumir(presupuesto, 1)?;
                    Some(Box::new(forma))
                }
                None => None,
            };
            let Some(cuerpo) = forma_positiva(cuerpo, presupuesto)? else {
                return Ok(None);
            };
            consumir(presupuesto, 1)?;
            consumir(presupuesto, 2)?;
            Ok(Some(FormaPositiva::Existe {
                ligaduras_nodo,
                ligaduras_arista,
                guarda,
                cuerpo: Box::new(cuerpo),
            }))
        }
        _ => Ok(None),
    }
}

fn contar_sortes(ligaduras: &[(crate::formula::Var, SorteVar)]) -> (u64, u64) {
    ligaduras
        .iter()
        .fold((0u64, 0u64), |(nodos, aristas), (_, sorte)| match sorte {
            SorteVar::Nodo => (nodos.saturating_add(1), aristas),
            SorteVar::Arista => (nodos, aristas.saturating_add(1)),
        })
}

fn costo_clon_asignacion(
    asignacion: &AsignacionCanonica,
    presupuesto: &PresupuestoExplicacion,
) -> Result<u64, ErrorBrecha> {
    let mut costo = costo_usize(asignacion.ligaduras.len(), presupuesto)?;
    for ligadura in &asignacion.ligaduras {
        costo = costo
            .checked_add(1)
            .and_then(|total| {
                total.checked_add(u64::try_from(ligadura.variable.como_str().len()).ok()?)
            })
            .ok_or(ErrorBrecha::PresupuestoAgotado {
                limite: presupuesto.limite,
            })?;
        let bytes_valor = match &ligadura.valor {
            ValorLigadura::Nodo(path) => costo_usize(path.como_str().len(), presupuesto)?,
            ValorLigadura::Arista(id) => {
                let partes = [
                    id.origen.como_str().len(),
                    id.destino.como_str().len(),
                    id.sorte.como_str().len(),
                ];
                partes
                    .into_iter()
                    .try_fold(4u64, |suma, largo| {
                        suma.checked_add(u64::try_from(largo).ok()?)
                    })
                    .ok_or(ErrorBrecha::PresupuestoAgotado {
                        limite: presupuesto.limite,
                    })?
            }
        };
        costo = costo
            .checked_add(bytes_valor)
            .ok_or(ErrorBrecha::PresupuestoAgotado {
                limite: presupuesto.limite,
            })?;
    }
    Ok(costo.max(1))
}

fn costo_usize(valor: usize, presupuesto: &PresupuestoExplicacion) -> Result<u64, ErrorBrecha> {
    u64::try_from(valor).map_err(|_| ErrorBrecha::PresupuestoAgotado {
        limite: presupuesto.limite,
    })
}

fn consumir(presupuesto: &mut PresupuestoExplicacion, unidades: u64) -> Result<(), ErrorBrecha> {
    presupuesto
        .consumir(unidades)
        .map_err(|limite| ErrorBrecha::PresupuestoAgotado { limite })
}

#[cfg(test)]
#[path = "brecha/tests.rs"]
mod tests;
