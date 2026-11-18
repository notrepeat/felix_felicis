//! `universo search`: búsqueda léxica sobre las notas del bundle. **Su salida
//! incluye datos del bundle** (rutas, títulos y fragmentos del cuerpo).
use super::{Formato, escribir_salida, historial::visible, reportar_fallo_escritura};
use serde_json::json;
use std::{io::Write, path::PathBuf};
use universo_okf::{
    busqueda::{ResultadoBusqueda, buscar},
    cargar_bundle, cargar_manifiesto,
};

pub(super) const USO: &str = "Uso: universo search <raiz> --manifiesto <m.toml> <términos...> [--limit <n>] [--format <text|json>]\nNotas donde cada término empieza alguna palabra, sin distinguir mayúsculas ni tildes (su salida incluye datos del bundle).\nUsa `--` antes de los términos que empiezan con '-'.\n";

const LIMITE_POR_DEFECTO: usize = 20;

pub(super) struct Solicitud {
    raiz: PathBuf,
    manifiesto: PathBuf,
    terminos: Vec<String>,
    limite: usize,
    formato: Formato,
}

pub(super) fn parsear(args: &[String]) -> Result<Solicitud, String> {
    let fail = || USO.to_string();
    let (mut manifiesto, mut limite, mut formato) = (None, None, None);
    let mut posicionales = Vec::new();
    let mut solo_posicionales = false;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--" && !solo_posicionales {
            solo_posicionales = true;
            i += 1;
            continue;
        }
        if solo_posicionales || !arg.starts_with('-') {
            if arg.is_empty() {
                return Err(fail());
            }
            posicionales.push(arg.clone());
            i += 1;
            continue;
        }
        let (nombre, valor, consumidos) = if let Some((nombre, valor)) = arg.split_once('=') {
            (nombre, valor, 1)
        } else {
            let valor = args.get(i + 1).ok_or_else(fail)?;
            if valor.starts_with('-') {
                return Err(fail());
            }
            (arg.as_str(), valor.as_str(), 2)
        };
        if valor.is_empty() {
            return Err(fail());
        }
        match nombre {
            "--manifiesto" if manifiesto.is_none() => manifiesto = Some(PathBuf::from(valor)),
            "--limit" if limite.is_none() => {
                limite = Some(
                    valor
                        .parse::<usize>()
                        .ok()
                        .filter(|n| *n >= 1)
                        .ok_or_else(fail)?,
                )
            }
            "--format" if formato.is_none() => {
                formato = Some(match valor {
                    "text" => Formato::Texto,
                    "json" => Formato::Json,
                    _ => return Err(fail()),
                })
            }
            _ => return Err(fail()),
        }
        i += consumidos;
    }
    let Some((raiz, terminos)) = posicionales.split_first() else {
        return Err(fail());
    };
    if terminos.iter().all(|t| t.trim().is_empty()) {
        return Err(fail());
    }
    Ok(Solicitud {
        raiz: PathBuf::from(raiz),
        manifiesto: manifiesto.ok_or_else(fail)?,
        terminos: terminos.to_vec(),
        limite: limite.unwrap_or(LIMITE_POR_DEFECTO),
        formato: formato.unwrap_or(Formato::Texto),
    })
}

fn texto(resultado: &ResultadoBusqueda) -> String {
    let mut salida = format!(
        "search: {} of {} matches\n",
        resultado.coincidencias.len(),
        resultado.total
    );
    for c in &resultado.coincidencias {
        let campos: Vec<&str> = c.campos.iter().map(|campo| campo.nombre()).collect();
        salida.push_str(&format!(
            "{} [{}] {} ({})\n",
            visible(&c.path.to_string()),
            visible(&c.tipo),
            visible(&c.titulo),
            campos.join(", ")
        ));
        if let Some(fragmento) = &c.fragmento {
            salida.push_str(&format!("  {}\n", visible(fragmento)));
        }
    }
    salida
}

fn json(consulta: &str, resultado: &ResultadoBusqueda) -> String {
    let coincidencias: Vec<_> = resultado
        .coincidencias
        .iter()
        .map(|c| {
            json!({
                "path": c.path.to_string(),
                "type": c.tipo,
                "title": c.titulo,
                "fields": c.campos.iter().map(|campo| campo.nombre()).collect::<Vec<_>>(),
                "score": c.puntaje,
                "snippet": c.fragmento,
            })
        })
        .collect();
    let valor = json!({
        "schema_version": 1,
        "command": "search",
        "query": consulta,
        "total": resultado.total,
        "matches": coincidencias,
    });
    format!("{valor}\n")
}

pub(super) fn ejecutar(s: Solicitud, salida: &mut impl Write, error: &mut impl Write) -> i32 {
    let manifiesto = match cargar_manifiesto(&s.manifiesto) {
        Ok(m) => m,
        Err(e) => {
            if let Err(causa) = error.write_all(format!("{e}\n").as_bytes()) {
                reportar_fallo_escritura(error, "stderr", &causa);
            }
            return 1;
        }
    };
    let estructura = match cargar_bundle(&s.raiz, manifiesto) {
        Ok(e) => e,
        Err(errores) => {
            let mut buffer = String::new();
            for e in errores {
                buffer.push_str(&format!("{e}\n"));
            }
            if let Err(causa) = error.write_all(buffer.as_bytes()) {
                reportar_fallo_escritura(error, "stderr", &causa);
            }
            return 1;
        }
    };
    let consulta = s.terminos.join(" ");
    let resultado = buscar(&estructura, &consulta, s.limite);
    let bytes = match s.formato {
        Formato::Texto => texto(&resultado),
        Formato::Json => json(&consulta, &resultado),
    };
    if escribir_salida(salida, error, bytes.as_bytes(), "stdout") {
        0
    } else {
        1
    }
}
