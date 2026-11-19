//! `universo diff <raiz> <desde> [<hasta>]`: diferencia neta entre dos estados
//! del historial. El estado `n` es el que dejó la entrada `n` de `universo
//! log`; el 0, el anterior a la primera. **Su salida incluye datos del
//! bundle** (átomos, rutas y títulos de notas creadas).
use super::{Formato, escribir_salida, historial::visible};
use serde_json::json;
use std::{io::Write, path::PathBuf};
use universo_okf::transaccion::{
    composicion::{Diferencia, ErrorComposicion, componer},
    historial::leer,
};

pub(super) const USO: &str = "Uso: universo diff <raiz> <desde> [<hasta>] [--format text|json]\nCambios netos entre el estado tras la entrada <desde> de `universo log` (0: antes de la primera) y el estado tras <hasta> (por defecto, la última).\n";

pub(super) struct Solicitud {
    raiz: PathBuf,
    desde: usize,
    hasta: Option<usize>,
    formato: Formato,
}

pub(super) fn parsear(args: &[String]) -> Result<Solicitud, String> {
    let fail = || USO.to_string();
    let mut posicionales = Vec::new();
    let mut formato = None;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        let valor = if let Some(v) = arg.strip_prefix("--format=") {
            i += 1;
            Some(v)
        } else if arg == "--format" {
            i += 2;
            Some(args.get(i - 1).ok_or_else(fail)?.as_str())
        } else {
            None
        };
        if let Some(valor) = valor {
            let nuevo = match valor {
                "text" => Formato::Texto,
                "json" => Formato::Json,
                _ => return Err(fail()),
            };
            if formato.replace(nuevo).is_some() {
                return Err(fail());
            }
            continue;
        }
        if arg.is_empty() || arg.starts_with('-') {
            return Err(fail());
        }
        posicionales.push(arg.clone());
        i += 1;
    }
    let numero = |texto: &String| texto.parse::<usize>().map_err(|_| fail());
    let (raiz, desde, hasta) = match posicionales.as_slice() {
        [raiz, desde] => (raiz, numero(desde)?, None),
        [raiz, desde, hasta] => (raiz, numero(desde)?, Some(numero(hasta)?)),
        _ => return Err(fail()),
    };
    Ok(Solicitud {
        raiz: PathBuf::from(raiz),
        desde,
        hasta,
        formato: formato.unwrap_or(Formato::Texto),
    })
}

fn texto(d: &Diferencia, desde: usize, hasta: usize, total: usize) -> String {
    let mut salida = format!("diff: state {desde} -> state {hasta} ({total} entries)\n");
    let vacia = d.creadas.is_empty()
        && d.cuerpos.is_empty()
        && d.agregar.is_empty()
        && d.retirar.is_empty()
        && d.config.is_empty();
    if vacia {
        salida.push_str("no changes\n");
    }
    for n in &d.config {
        salida.push_str(&format!("config changed (entry {n})\n"));
    }
    for nota in &d.creadas {
        salida.push_str(&format!("create {} (entry {})\n", nota.path, nota.entrada));
        for atomo in &nota.retirar {
            salida.push_str(&format!("  - {atomo}\n"));
        }
        for atomo in &nota.agregar {
            salida.push_str(&format!("  + {atomo}\n"));
        }
    }
    for path in &d.cuerpos {
        salida.push_str(&format!("update body {path}\n"));
    }
    for atomo in &d.retirar {
        salida.push_str(&format!("- {atomo}\n"));
    }
    for atomo in &d.agregar {
        salida.push_str(&format!("+ {atomo}\n"));
    }
    visible(&salida)
}

fn textos<T: ToString>(elementos: &[T]) -> Vec<String> {
    elementos.iter().map(ToString::to_string).collect()
}

fn json(d: &Diferencia, desde: usize, hasta: usize, total: usize) -> String {
    let creadas: Vec<_> = d
        .creadas
        .iter()
        .map(|n| {
            json!({"path":n.path.to_string(),"entry":n.entrada,
                   "add":textos(&n.agregar),"remove":textos(&n.retirar)})
        })
        .collect();
    format!(
        "{}\n",
        json!({"schema_version":1,"command":"diff","status":"ok","from":desde,"to":hasta,"entries":total,
               "created":creadas,"body_updated":textos(&d.cuerpos),"add":textos(&d.agregar),
               "remove":textos(&d.retirar),"config_changes":d.config,"errors":[]})
    )
}

pub(super) fn ejecutar(s: Solicitud, salida: &mut impl Write, error: &mut impl Write) -> i32 {
    let resultado = leer(&s.raiz)
        .map_err(|e| ErrorComposicion {
            codigo: e.codigo(),
            entrada: None,
        })
        .and_then(|historial| {
            let total = historial.entradas.len();
            let hasta = s.hasta.unwrap_or(total);
            componer(&historial, s.desde, hasta).map(|d| (d, hasta, total))
        });
    let (payload, codigo) = match (resultado, s.formato) {
        (Ok((d, hasta, total)), Formato::Texto) => (texto(&d, s.desde, hasta, total), 0),
        (Ok((d, hasta, total)), Formato::Json) => (json(&d, s.desde, hasta, total), 0),
        (Err(e), Formato::Texto) => {
            let entrada = e
                .entrada
                .map(|n| format!("entry: {n}\n"))
                .unwrap_or_default();
            let nota = match (e.codigo, e.entrada) {
                ("external_change", Some(n)) => format!(
                    "note: la entrada {n} selló un cambio hecho fuera de Felix; diff no puede componer a través de ella. Usa un <desde> de {n} o mayor.\n"
                ),
                _ => String::new(),
            };
            (
                format!("diff: error\ncode: {}\n{entrada}{nota}", e.codigo),
                1,
            )
        }
        (Err(e), Formato::Json) => {
            let mut detalle = json!({"code": e.codigo});
            if let Some(n) = e.entrada {
                detalle["entry"] = json!(n);
            }
            (
                format!(
                    "{}\n",
                    json!({"schema_version":1,"command":"diff","status":"error","errors":[detalle]})
                ),
                1,
            )
        }
    };
    if !escribir_salida(salida, error, payload.as_bytes(), "stdout") {
        return 1;
    }
    codigo
}
