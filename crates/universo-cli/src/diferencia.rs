//! `universo delta`: delta canónico desde un borrador editado.
use super::escribir_salida;
use std::{io::Write, path::PathBuf};
use universo_okf::{cargar_manifiesto, delta_desde_borrador, serializar_delta_json_canonico};

pub(super) const USO: &str = "Uso: universo delta <raiz> --desde <borrador> --manifiesto <m.toml>\nEscribe en stdout el delta que lleva el root admitido al borrador (felix-delta-v1/v2).\n";

pub(super) struct Solicitud {
    raiz: PathBuf,
    borrador: PathBuf,
    manifiesto: PathBuf,
}

pub(super) fn parsear(args: &[String]) -> Result<Solicitud, String> {
    let fail = || USO.to_string();
    let (mut raiz, mut borrador, mut manifiesto) = (None, None, None);
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if !arg.starts_with('-') {
            if arg.is_empty() || raiz.replace(PathBuf::from(arg)).is_some() {
                return Err(fail());
            }
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
            "--desde" if borrador.is_none() => borrador = Some(PathBuf::from(valor)),
            "--manifiesto" if manifiesto.is_none() => manifiesto = Some(PathBuf::from(valor)),
            _ => return Err(fail()),
        }
        i += consumidos;
    }
    Ok(Solicitud {
        raiz: raiz.ok_or_else(fail)?,
        borrador: borrador.ok_or_else(fail)?,
        manifiesto: manifiesto.ok_or_else(fail)?,
    })
}

pub(super) fn ejecutar(s: Solicitud, salida: &mut impl Write, error: &mut impl Write) -> i32 {
    let resultado = cargar_manifiesto(&s.manifiesto)
        .map_err(|_| "manifest_invalid")
        .and_then(|m| delta_desde_borrador(&s.raiz, &s.borrador, m).map_err(|e| e.codigo()))
        .and_then(|d| serializar_delta_json_canonico(&d).map_err(|_| "delta_invalid"));
    match resultado {
        Ok(json) => {
            if escribir_salida(salida, error, format!("{json}\n").as_bytes(), "stdout") {
                0
            } else {
                1
            }
        }
        Err(code) => {
            let _ = error.write_all(format!("delta: error\ncode: {code}\n").as_bytes());
            1
        }
    }
}
