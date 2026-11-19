//! Diferencia neta entre dos entradas del historial (brecha U4, `universo
//! diff`), compuesta a partir de los deltas que guardó cada apply.
//!
//! Se compone el **efecto** que registró cada apply (los átomos que de verdad
//! añadió y retiró), no el delta pedido: un delta puede pedir añadir algo ya
//! presente, y Felix lo admite como no-op. Como cada efecto es la diferencia
//! exacta entre dos estados consecutivos, componer los de las entradas
//! `desde+1..=hasta` da la diferencia exacta entre el estado tras `desde` y el
//! estado tras `hasta`. Del delta sólo se toman las notas creadas y los
//! cuerpos reemplazados. Las ediciones fuera de Felix no están en el historial
//! (las detecta `universo guard`). Para que la suma de efectos sea la
//! diferencia real, cada entrada guarda los tokens del estado del que partió y
//! del que dejó, y la composición exige que encadenen: un cambio sin entrada
//! (un historial que no se pudo escribir, un `recover`, un `seal` anterior a
//! U5) es un hueco y se informa como error, no se ignora. Desde la fase 10
//! (U5), un `seal` que acepta ediciones externas deja una entrada `seal`
//! (`external_change`) y un cambio de reglas o manifiesto, una `config`, que
//! se compone por su efecto y se lista en `Diferencia::config`.
//!
//! Una nota creada en el rango no existía en el estado base: los átomos que la
//! mencionan van a su propio grupo. Como los efectos son exactos, ese grupo es
//! el conjunto de sus átomos en el estado final (su tipo, su título como
//! `HasLab`, sus atributos y aristas).
use crate::transaccion::historial::Historial;
use std::collections::{BTreeMap, BTreeSet};
use universo_core::{Atomo, BundlePath};

/// Nota creada dentro del rango.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotaCreada {
    pub path: BundlePath,
    /// Entrada (1-based, como en `universo log`) que la creó.
    pub entrada: usize,
    /// Átomos que la mencionan en el estado final.
    pub agregar: Vec<Atomo>,
    /// Siempre vacío con efectos exactos; se conserva por si un efecto
    /// registrado retirara algo que la composición no vio añadir.
    pub retirar: Vec<Atomo>,
}

/// Diferencia neta entre el estado tras `desde` y el estado tras `hasta`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Diferencia {
    pub creadas: Vec<NotaCreada>,
    /// Notas que existían en el estado base y cuyo cuerpo final difiere del
    /// de ese estado (un cambio revertido dentro del rango no cuenta).
    pub cuerpos: Vec<BundlePath>,
    /// Átomos presentes tras `hasta` y no tras `desde` (sin los de notas
    /// creadas en el rango).
    pub agregar: Vec<Atomo>,
    /// Átomos presentes tras `desde` y no tras `hasta`.
    pub retirar: Vec<Atomo>,
    /// Entradas del rango que cambiaron las reglas o el manifiesto.
    pub config: Vec<usize>,
}

/// Por qué no se pudo componer el rango.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ErrorComposicion {
    pub codigo: &'static str,
    /// Entrada que lo impidió, si es una sola.
    pub entrada: Option<usize>,
}

fn error(codigo: &'static str, entrada: Option<usize>) -> ErrorComposicion {
    ErrorComposicion { codigo, entrada }
}

#[derive(Default)]
struct Neto {
    agregar: BTreeSet<Atomo>,
    retirar: BTreeSet<Atomo>,
}

impl Neto {
    fn agregar(&mut self, atomo: &Atomo) {
        if !self.retirar.remove(atomo) {
            self.agregar.insert(atomo.clone());
        }
    }
    fn retirar(&mut self, atomo: &Atomo) {
        if !self.agregar.remove(atomo) {
            self.retirar.insert(atomo.clone());
        }
    }
}

/// Compone los deltas de las entradas `desde+1..=hasta` (1-based; `desde` 0
/// es el estado antes de la primera entrada).
///
/// Errores: `history_incomplete` si el historial tiene líneas descartadas (no
/// se sabe qué cambios faltan), `range_invalid` si `desde > hasta`,
/// `entry_not_found` si `hasta` pasa del final; `effect_omitted`,
/// `effect_not_recorded` o `effect_invalid` si una entrada del rango no guarda
/// un efecto legible (las anteriores a este campo no lo tienen);
/// `state_not_recorded` si no guarda los tokens de estado y `history_gap` si
/// no parte del estado que dejó la anterior; `delta_omitted` o
/// `delta_not_recorded` si no guarda su delta, `history_inconsistent` si
/// crea dos veces la misma nota, `external_change` si cruza una entrada `seal`
/// y `entry_kind_unknown` si cruza una clase de entrada que no conoce.
pub fn componer(
    historial: &Historial,
    desde: usize,
    hasta: usize,
) -> Result<Diferencia, ErrorComposicion> {
    if historial.descartadas > 0 {
        return Err(error("history_incomplete", None));
    }
    if desde > hasta {
        return Err(error("range_invalid", None));
    }
    if hasta > historial.entradas.len() {
        return Err(error("entry_not_found", Some(hasta)));
    }
    let mut global = Neto::default();
    let mut configs = Vec::new();
    let mut creadas: BTreeMap<BundlePath, (NotaCreada, Neto)> = BTreeMap::new();
    // Por path: huella del cuerpo en el estado base y en el último estado.
    let mut cuerpos: BTreeMap<BundlePath, (String, String)> = BTreeMap::new();
    for numero in desde + 1..=hasta {
        let entrada = &historial.entradas[numero - 1];
        // Un `seal` aceptó un cambio hecho fuera de Felix: no se sabe qué
        // cambió, así que la diferencia a través de él no es exacta.
        if entrada.clase.as_deref() == Some("seal") {
            return Err(error("external_change", Some(numero)));
        }
        let config = match entrada.clase.as_deref() {
            None => false,
            // Un cambio de configuración no trae delta; si lo trae, la línea
            // fue alterada.
            Some("config") if entrada.delta.is_some() || entrada.delta_omitido => {
                return Err(error("history_inconsistent", Some(numero)));
            }
            Some("config") => true,
            // Una clase que esta versión no conoce: no se sabe qué cambió.
            Some(_) => return Err(error("entry_kind_unknown", Some(numero))),
        };
        let efecto = match &entrada.efecto {
            Some(efecto) => efecto,
            None if entrada.efecto_invalido => return Err(error("effect_invalid", Some(numero))),
            None if entrada.efecto_omitido => return Err(error("effect_omitted", Some(numero))),
            None => return Err(error("effect_not_recorded", Some(numero))),
        };
        // Sin huecos: la entrada parte del estado que dejó la anterior. Un
        // cambio sin entrada (historial que no se pudo escribir, recover,
        // `seal` de ediciones externas) rompe la cadena.
        let Some(partida) = &entrada.estado_desde else {
            return Err(error("state_not_recorded", Some(numero)));
        };
        if numero >= 2 {
            match &historial.entradas[numero - 2].estado_hasta {
                Some(anterior) if anterior == partida => {}
                Some(_) => return Err(error("history_gap", Some(numero))),
                None => return Err(error("state_not_recorded", Some(numero - 1))),
            }
        }
        // Un cambio de configuración no tiene delta: no crea notas ni cambia
        // cuerpos, y su efecto sobre los átomos está en `effect`.
        let delta = match &entrada.delta {
            _ if config => {
                configs.push(numero);
                universo_core::Delta::default()
            }
            Some(texto) => crate::parsear_delta_json(texto)
                .map_err(|_| error("delta_not_recorded", Some(numero)))?,
            None if entrada.delta_omitido => return Err(error("delta_omitted", Some(numero))),
            None => return Err(error("delta_not_recorded", Some(numero))),
        };
        for nota in delta.crear() {
            if creadas.contains_key(&nota.path) {
                return Err(error("history_inconsistent", Some(numero)));
            }
            let creada = NotaCreada {
                path: nota.path.clone(),
                entrada: numero,
                agregar: Vec::new(),
                retirar: Vec::new(),
            };
            creadas.insert(nota.path.clone(), (creada, Neto::default()));
        }
        // Cuerpos de notas que existían en el estado base; los de notas
        // creadas en el rango son parte de su creación.
        for cambio in &efecto.cuerpos {
            if creadas.contains_key(&cambio.path) {
                continue;
            }
            cuerpos
                .entry(cambio.path.clone())
                .and_modify(|(_, ultimo)| ultimo.clone_from(&cambio.despues))
                .or_insert_with(|| (cambio.antes.clone(), cambio.despues.clone()));
        }
        for (atomo, agregar) in efecto
            .retirar
            .iter()
            .map(|a| (a, false))
            .chain(efecto.agregar.iter().map(|a| (a, true)))
        {
            // Un átomo que menciona una nota creada en el rango no existía en
            // el estado base: va al grupo de la creada más tarde que menciona.
            let grupo = atomo
                .paths()
                .into_iter()
                .filter_map(|p| creadas.get(p).map(|(n, _)| (n.entrada, p.clone())))
                .max();
            let neto = match grupo.and_then(|(_, path)| creadas.get_mut(&path)) {
                Some((_, neto)) => neto,
                None => &mut global,
            };
            if agregar {
                neto.agregar(atomo);
            } else {
                neto.retirar(atomo);
            }
        }
    }
    Ok(Diferencia {
        config: configs,
        creadas: creadas
            .into_values()
            .map(|(mut nota, neto)| {
                nota.agregar = neto.agregar.into_iter().collect();
                nota.retirar = neto.retirar.into_iter().collect();
                nota
            })
            .collect(),
        cuerpos: cuerpos
            .into_iter()
            .filter(|(_, (base, ultimo))| base != ultimo)
            .map(|(path, _)| path)
            .collect(),
        agregar: global.agregar.into_iter().collect(),
        retirar: global.retirar.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaccion::historial::{CambioCuerpo, Efecto, EntradaHistorial};

    /// Entrada cuyo efecto es exactamente el que pide su delta (sin no-op).
    fn entrada(delta: &str) -> EntradaHistorial {
        let d = crate::parsear_delta_json(delta).unwrap();
        EntradaHistorial {
            fecha: "2026-11-18T00:00:00Z".into(),
            autor: None,
            motivo: None,
            sellado: true,
            delta: Some(delta.to_string()),
            delta_omitido: false,
            efecto: Some(Efecto {
                agregar: d.agregar().to_vec(),
                retirar: d.retirar().to_vec(),
                cuerpos: vec![],
            }),
            efecto_omitido: false,
            efecto_invalido: false,
            estado_desde: None,
            estado_hasta: None,
            clase: None,
        }
    }

    fn token(n: usize) -> Option<String> {
        Some(format!("{n:064x}"))
    }

    /// Historial encadenado: la entrada k parte del estado k-1 y deja el k.
    fn historial(deltas: &[&str]) -> Historial {
        Historial {
            entradas: deltas
                .iter()
                .enumerate()
                .map(|(i, d)| EntradaHistorial {
                    estado_desde: token(i),
                    estado_hasta: token(i + 1),
                    ..entrada(d)
                })
                .collect(),
            descartadas: 0,
        }
    }

    const VACIO: &str = r#"{"schema":"felix-delta-v1","add":[],"remove":[]}"#;

    fn cuerpo(path: &str, antes: char, despues: char) -> CambioCuerpo {
        CambioCuerpo {
            path: BundlePath::nuevo(path).unwrap(),
            antes: antes.to_string().repeat(64),
            despues: despues.to_string().repeat(64),
        }
    }

    #[test]
    fn un_hueco_en_la_cadena_de_estados_es_error() {
        let mut h = historial(&[V1_A, V1_B, V1_VUELTA]);
        // Un cambio sin entrada entre la 1 y la 2 (p. ej. un `seal` de
        // ediciones externas): la 2 no parte del estado que dejó la 1.
        h.entradas[1].estado_desde = token(99);
        assert_eq!(
            componer(&h, 0, 3).unwrap_err(),
            ErrorComposicion {
                codigo: "history_gap",
                entrada: Some(2)
            }
        );
        // También si el rango empieza justo después del hueco.
        assert_eq!(componer(&h, 1, 2).unwrap_err().codigo, "history_gap");
        // Un rango que no lo cruza sigue siendo válido.
        assert!(componer(&h, 2, 3).is_ok());
        assert!(componer(&h, 0, 1).is_ok());
        h.entradas[2].estado_desde = None;
        assert_eq!(
            componer(&h, 2, 3).unwrap_err(),
            ErrorComposicion {
                codigo: "state_not_recorded",
                entrada: Some(3)
            }
        );
        h.entradas[2] = historial(&[V1_A, V1_B, V1_VUELTA]).entradas.remove(2);
        h.entradas[1].estado_hasta = None;
        assert_eq!(
            componer(&h, 2, 3).unwrap_err(),
            ErrorComposicion {
                codigo: "state_not_recorded",
                entrada: Some(2)
            }
        );
    }

    #[test]
    fn un_cuerpo_cambiado_y_revertido_no_es_un_cambio_neto() {
        let mut h = historial(&[VACIO, VACIO, VACIO]);
        h.entradas[0].efecto.as_mut().unwrap().cuerpos = vec![cuerpo("n/a", '0', '1')];
        h.entradas[1].efecto.as_mut().unwrap().cuerpos =
            vec![cuerpo("n/a", '1', '0'), cuerpo("n/b", '2', '3')];
        let d = componer(&h, 0, 2).unwrap();
        assert_eq!(
            d.cuerpos
                .iter()
                .map(BundlePath::como_str)
                .collect::<Vec<_>>(),
            vec!["n/b"]
        );
        let d = componer(&h, 0, 1).unwrap();
        assert_eq!(
            d.cuerpos
                .iter()
                .map(BundlePath::como_str)
                .collect::<Vec<_>>(),
            vec!["n/a"]
        );
    }

    #[test]
    fn efecto_ilegible_y_nota_creada_dos_veces_son_errores_propios() {
        let mut h = historial(&[V1_A]);
        h.entradas[0].efecto = None;
        h.entradas[0].efecto_invalido = true;
        assert_eq!(componer(&h, 0, 1).unwrap_err().codigo, "effect_invalid");
        let crear = r#"{"schema":"felix-delta-v2","create":[{"path":"n/x","type":"a","title":"X"}],"update":[],"add":[],"remove":[]}"#;
        let h = historial(&[crear, crear]);
        assert_eq!(
            componer(&h, 0, 2).unwrap_err(),
            ErrorComposicion {
                codigo: "history_inconsistent",
                entrada: Some(2)
            }
        );
    }

    fn textos(atomos: &[Atomo]) -> Vec<String> {
        atomos.iter().map(ToString::to_string).collect()
    }

    const V1_A: &str = r#"{"schema":"felix-delta-v1","add":["Estado(n/a, vigente)"],"remove":["Estado(n/a, borrador)"]}"#;
    const V1_B: &str = r#"{"schema":"felix-delta-v1","add":["Estado(n/a, retirado)"],"remove":["Estado(n/a, vigente)"]}"#;
    const V1_VUELTA: &str = r#"{"schema":"felix-delta-v1","add":["Estado(n/a, borrador)"],"remove":["Estado(n/a, retirado)"]}"#;

    #[test]
    fn compone_el_rango_y_cancela_lo_que_va_y_vuelve() {
        let h = historial(&[V1_A, V1_B, V1_VUELTA]);
        let d = componer(&h, 0, 2).unwrap();
        assert_eq!(textos(&d.agregar), vec!["Estado(n/a, retirado)"]);
        assert_eq!(textos(&d.retirar), vec!["Estado(n/a, borrador)"]);
        // Ida y vuelta: sin diferencia neta.
        assert_eq!(componer(&h, 0, 3).unwrap(), Diferencia::default());
        // Un solo paso es su delta.
        let d = componer(&h, 1, 2).unwrap();
        assert_eq!(textos(&d.agregar), vec!["Estado(n/a, retirado)"]);
        assert_eq!(textos(&d.retirar), vec!["Estado(n/a, vigente)"]);
        // Rango vacío.
        assert_eq!(componer(&h, 2, 2).unwrap(), Diferencia::default());
    }

    #[test]
    fn los_atomos_de_una_nota_creada_en_el_rango_van_a_su_grupo() {
        let crear = r#"{"schema":"felix-delta-v2","create":[{"path":"n/nueva","type":"a","title":"Nueva"}],"update":[],"add":["Estado(n/nueva, borrador)"],"remove":[]}"#;
        let retipar = r#"{"schema":"felix-delta-v2","create":[],"update":[{"path":"n/nueva","body":"Texto.\n"}],"add":["Type(n/nueva, b)","Estado(n/a, vigente)"],"remove":["Type(n/nueva, a)","Estado(n/a, borrador)"]}"#;
        let mut h = historial(&[crear, retipar]);
        // El efecto real de crear incluye los átomos implícitos de la nota.
        let implicito = Atomo::parsear("Type(n/nueva, a)").unwrap();
        h.entradas[0]
            .efecto
            .as_mut()
            .unwrap()
            .agregar
            .push(implicito);
        let d = componer(&h, 0, 2).unwrap();
        assert_eq!(d.creadas.len(), 1);
        let n = &d.creadas[0];
        assert_eq!((n.path.como_str(), n.entrada), ("n/nueva", 1));
        // Tipo cambiado después de crearla: queda sólo el final.
        assert_eq!(
            textos(&n.agregar),
            vec!["Estado(n/nueva, borrador)", "Type(n/nueva, b)"]
        );
        assert!(n.retirar.is_empty());
        assert_eq!(textos(&d.agregar), vec!["Estado(n/a, vigente)"]);
        assert_eq!(textos(&d.retirar), vec!["Estado(n/a, borrador)"]);
        // El cuerpo de una nota creada en el rango es parte de su creación,
        // aunque una entrada posterior lo cambie.
        h.entradas[1].efecto.as_mut().unwrap().cuerpos = vec![CambioCuerpo {
            path: BundlePath::nuevo("n/nueva").unwrap(),
            antes: "0".repeat(64),
            despues: "1".repeat(64),
        }];
        assert!(componer(&h, 0, 2).unwrap().cuerpos.is_empty());
        assert_eq!(
            componer(&h, 1, 2)
                .unwrap()
                .cuerpos
                .iter()
                .map(BundlePath::como_str)
                .collect::<Vec<_>>(),
            vec!["n/nueva"]
        );
        // Desde después de crearla, la nota ya existía: sus átomos son globales.
        let d = componer(&h, 1, 2).unwrap();
        assert!(d.creadas.is_empty());
        assert!(textos(&d.retirar).contains(&"Type(n/nueva, a)".to_string()));
    }

    #[test]
    fn compone_el_efecto_y_no_el_delta_pedido() {
        // El delta pidió añadir algo ya presente: el efecto registrado no lo
        // contiene, y no debe aparecer como cambio.
        let noop = r#"{"schema":"felix-delta-v1","add":["Estado(n/a, borrador)"],"remove":[]}"#;
        let mut h = historial(&[noop, V1_A]);
        h.entradas[0].efecto = Some(Efecto::default());
        assert_eq!(componer(&h, 0, 1).unwrap(), Diferencia::default());
        let d = componer(&h, 0, 2).unwrap();
        assert_eq!(textos(&d.retirar), vec!["Estado(n/a, borrador)"]);
        assert_eq!(textos(&d.agregar), vec!["Estado(n/a, vigente)"]);
    }

    #[test]
    fn errores_por_rango_historial_incompleto_o_delta_ausente() {
        let mut h = historial(&[V1_A, V1_B]);
        assert_eq!(componer(&h, 2, 1).unwrap_err().codigo, "range_invalid");
        assert_eq!(
            componer(&h, 0, 3).unwrap_err(),
            ErrorComposicion {
                codigo: "entry_not_found",
                entrada: Some(3)
            }
        );
        h.entradas[1].efecto = None;
        assert_eq!(
            componer(&h, 0, 2).unwrap_err(),
            ErrorComposicion {
                codigo: "effect_not_recorded",
                entrada: Some(2)
            }
        );
        h.entradas[1].efecto_omitido = true;
        assert_eq!(componer(&h, 1, 2).unwrap_err().codigo, "effect_omitted");
        h.entradas[1] = historial(&[V1_A, V1_B]).entradas.remove(1);
        h.entradas[1].delta = None;
        assert_eq!(
            componer(&h, 0, 2).unwrap_err(),
            ErrorComposicion {
                codigo: "delta_not_recorded",
                entrada: Some(2)
            }
        );
        // Fuera del rango, la entrada sin delta no importa.
        assert!(componer(&h, 0, 1).is_ok());
        h.entradas[1].delta_omitido = true;
        assert_eq!(componer(&h, 1, 2).unwrap_err().codigo, "delta_omitted");
        h.descartadas = 1;
        assert_eq!(componer(&h, 0, 1).unwrap_err().codigo, "history_incomplete");
    }

    #[test]
    fn una_entrada_config_compone_sin_delta_y_un_seal_corta_la_diferencia() {
        let mut h = historial(&[V1_A, VACIO, V1_B]);
        h.entradas[1].clase = Some("config".into());
        h.entradas[1].delta = None;
        let d = componer(&h, 0, 3).unwrap();
        assert_eq!(d.config, vec![2]);
        assert_eq!(textos(&d.agregar), vec!["Estado(n/a, retirado)"]);
        // Una `config` con delta fue alterada.
        let mut alterado = h.clone();
        alterado.entradas[1].delta = historial(&[V1_A]).entradas[0].delta.clone();
        assert_eq!(
            componer(&alterado, 0, 3).unwrap_err().codigo,
            "history_inconsistent"
        );
        h.entradas[1].clase = Some("futura".into());
        assert_eq!(
            componer(&h, 0, 2).unwrap_err(),
            ErrorComposicion {
                codigo: "entry_kind_unknown",
                entrada: Some(2)
            }
        );
        h.entradas[1].clase = Some("seal".into());
        assert_eq!(
            componer(&h, 0, 3).unwrap_err(),
            ErrorComposicion {
                codigo: "external_change",
                entrada: Some(2)
            }
        );
        // Después del seal, la diferencia vuelve a ser exacta.
        assert!(componer(&h, 2, 3).is_ok());
    }
}
