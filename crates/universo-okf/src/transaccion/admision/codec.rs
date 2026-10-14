//! Encoder binario privado del contexto de admisión.
//!
//! `FELIX-ADMCTX` v1 es framing interno experimental, no contrato público ni
//! prueba de admisión. El decoder deberá validar el framing y reconstruir las
//! identidades derivadas antes de evaluar otra vez la propuesta.

#[allow(dead_code)]
pub(crate) mod cursor;
#[allow(dead_code)]
pub(crate) mod lectura;

use std::fmt;

use crate::transaccion::snapshot::TipoEntradaSnapshot;

use super::ContextoAdmision;

const MAGIC: &[u8; 12] = b"FELIX-ADMCTX";
const VERSION: u16 = 1;
const HEADER_BYTES: usize = MAGIC.len() + 2 + 8 + 4;
const MAX_CODEC_BYTES: usize = 512 * 1024 * 1024;

/// Codifica un contexto válido en un framing privado y determinista.
///
/// Las entradas se ordenan por bytes UTF-8 de path. Se conservan los bytes
/// raw y los marcadores de procedencia de configuración; no se incluyen
/// identidad de filesystem, token ni digest, que se recalculan al reconstruir.
pub(crate) fn codificar(contexto: &ContextoAdmision) -> Result<Vec<u8>, ErrorCodecAdmision> {
    let (longitud, entradas) = precalcular_tamano(contexto, MAX_CODEC_BYTES)?;
    let longitud_u64 = u64::try_from(longitud).map_err(|_| ErrorCodecAdmision::LongitudExcedida)?;
    let count = u32::try_from(entradas.len()).map_err(|_| ErrorCodecAdmision::LongitudExcedida)?;
    let configuracion = contexto.configuracion();
    let mut salida = Vec::new();
    salida
        .try_reserve_exact(longitud)
        .map_err(|_| ErrorCodecAdmision::ReservaFallida)?;

    salida.extend_from_slice(MAGIC);
    salida.extend_from_slice(&VERSION.to_be_bytes());
    salida.extend_from_slice(&longitud_u64.to_be_bytes());
    salida.extend_from_slice(&count.to_be_bytes());

    for entrada in entradas {
        let tag = match (entrada.tipo(), entrada.bytes()) {
            (TipoEntradaSnapshot::Directorio, None) => 0,
            (TipoEntradaSnapshot::Archivo, Some(_)) => 1,
            _ => return Err(ErrorCodecAdmision::ContextoInvalido),
        };
        salida.push(tag);
        escribir_u32(&mut salida, entrada.ruta().len())?;
        salida.extend_from_slice(entrada.ruta().as_bytes());
        if let Some(bytes) = entrada.bytes() {
            escribir_u64(&mut salida, bytes.len())?;
            salida.extend_from_slice(bytes);
        }
    }

    escribir_bytes_u64(&mut salida, configuracion.manifiesto_raw())?;
    escribir_ruta_root(&mut salida, configuracion.ruta_manifiesto_root())?;
    escribir_bytes_u64(&mut salida, configuracion.invariantes_raw())?;
    escribir_ruta_root(&mut salida, configuracion.ruta_invariantes_root())?;
    escribir_bytes_u64(&mut salida, contexto.delta_json_v1().as_bytes())?;
    escribir_bytes_u32(&mut salida, contexto.version_motor().as_bytes())?;
    escribir_bytes_u32(&mut salida, contexto.policy_id().as_bytes())?;

    if salida.len() != longitud {
        return Err(ErrorCodecAdmision::LongitudExcedida);
    }
    Ok(salida)
}

fn precalcular_tamano(
    contexto: &ContextoAdmision,
    limite: usize,
) -> Result<(usize, Vec<&crate::transaccion::snapshot::EntradaSnapshot>), ErrorCodecAdmision> {
    contexto
        .validar_base()
        .map_err(|_| ErrorCodecAdmision::ContextoInvalido)?;

    let mut entradas: Vec<_> = contexto.entradas_base.iter().collect();
    entradas.sort_unstable_by(|izquierda, derecha| {
        izquierda.ruta().as_bytes().cmp(derecha.ruta().as_bytes())
    });
    let mut total = HEADER_BYTES;
    validar_limite_total(total, limite)?;

    for entrada in &entradas {
        sumar(&mut total, 1, limite)?;
        sumar_longitud_u32(&mut total, entrada.ruta().as_bytes(), limite)?;
        match (entrada.tipo(), entrada.bytes()) {
            (TipoEntradaSnapshot::Directorio, None) => {}
            (TipoEntradaSnapshot::Archivo, Some(bytes)) => {
                sumar_longitud_u64(&mut total, bytes, limite)?;
            }
            _ => return Err(ErrorCodecAdmision::ContextoInvalido),
        }
    }

    let configuracion = contexto.configuracion();
    sumar_longitud_u64(&mut total, configuracion.manifiesto_raw(), limite)?;
    sumar_ruta_root(&mut total, configuracion.ruta_manifiesto_root(), limite)?;
    sumar_longitud_u64(&mut total, configuracion.invariantes_raw(), limite)?;
    sumar_ruta_root(&mut total, configuracion.ruta_invariantes_root(), limite)?;
    sumar_longitud_u64(&mut total, contexto.delta_json_v1().as_bytes(), limite)?;
    sumar_longitud_u32(&mut total, contexto.version_motor().as_bytes(), limite)?;
    sumar_longitud_u32(&mut total, contexto.policy_id().as_bytes(), limite)?;
    Ok((total, entradas))
}

fn sumar_longitud_u32(
    total: &mut usize,
    bytes: &[u8],
    limite: usize,
) -> Result<(), ErrorCodecAdmision> {
    u32::try_from(bytes.len()).map_err(|_| ErrorCodecAdmision::LongitudExcedida)?;
    sumar(total, 4, limite)?;
    sumar(total, bytes.len(), limite)
}

fn sumar_longitud_u64(
    total: &mut usize,
    bytes: &[u8],
    limite: usize,
) -> Result<(), ErrorCodecAdmision> {
    u64::try_from(bytes.len()).map_err(|_| ErrorCodecAdmision::LongitudExcedida)?;
    sumar(total, 8, limite)?;
    sumar(total, bytes.len(), limite)
}

fn sumar_ruta_root(
    total: &mut usize,
    ruta: Option<&str>,
    limite: usize,
) -> Result<(), ErrorCodecAdmision> {
    sumar(total, 1, limite)?;
    if let Some(ruta) = ruta {
        sumar_longitud_u32(total, ruta.as_bytes(), limite)?;
    }
    Ok(())
}

fn sumar(total: &mut usize, bytes: usize, limite: usize) -> Result<(), ErrorCodecAdmision> {
    *total = total
        .checked_add(bytes)
        .ok_or(ErrorCodecAdmision::LongitudExcedida)?;
    validar_limite_total(*total, limite)
}

fn validar_limite_total(total: usize, limite: usize) -> Result<(), ErrorCodecAdmision> {
    if total <= limite {
        Ok(())
    } else {
        Err(ErrorCodecAdmision::TotalExcedeLimite)
    }
}

fn escribir_bytes_u32(salida: &mut Vec<u8>, bytes: &[u8]) -> Result<(), ErrorCodecAdmision> {
    escribir_u32(salida, bytes.len())?;
    salida.extend_from_slice(bytes);
    Ok(())
}

fn escribir_bytes_u64(salida: &mut Vec<u8>, bytes: &[u8]) -> Result<(), ErrorCodecAdmision> {
    escribir_u64(salida, bytes.len())?;
    salida.extend_from_slice(bytes);
    Ok(())
}

fn escribir_u32(salida: &mut Vec<u8>, longitud: usize) -> Result<(), ErrorCodecAdmision> {
    let longitud = u32::try_from(longitud).map_err(|_| ErrorCodecAdmision::LongitudExcedida)?;
    salida.extend_from_slice(&longitud.to_be_bytes());
    Ok(())
}

fn escribir_u64(salida: &mut Vec<u8>, longitud: usize) -> Result<(), ErrorCodecAdmision> {
    let longitud = u64::try_from(longitud).map_err(|_| ErrorCodecAdmision::LongitudExcedida)?;
    salida.extend_from_slice(&longitud.to_be_bytes());
    Ok(())
}

fn escribir_ruta_root(salida: &mut Vec<u8>, ruta: Option<&str>) -> Result<(), ErrorCodecAdmision> {
    match ruta {
        None => salida.push(0),
        Some(ruta) => {
            salida.push(1);
            escribir_bytes_u32(salida, ruta.as_bytes())?;
        }
    }
    Ok(())
}

/// Fallos fijos y redactados del codec privado.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorCodecAdmision {
    ContextoInvalido,
    FramingInvalido,
    VersionNoSoportada,
    LimiteComponenteExcedido,
    TotalExcedeLimite,
    LongitudExcedida,
    ReservaFallida,
}

impl ErrorCodecAdmision {
    pub(crate) const fn codigo(self) -> &'static str {
        match self {
            Self::ContextoInvalido => "admission_codec_context_invalid",
            Self::FramingInvalido => "admission_codec_framing_invalid",
            Self::VersionNoSoportada => "admission_codec_version_unsupported",
            Self::LimiteComponenteExcedido => "admission_codec_component_limit",
            Self::TotalExcedeLimite => "admission_codec_limit",
            Self::LongitudExcedida => "admission_codec_length_overflow",
            Self::ReservaFallida => "admission_codec_allocation_failed",
        }
    }
}

impl fmt::Debug for ErrorCodecAdmision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ErrorCodecAdmision")
            .field(&self.codigo())
            .finish()
    }
}

impl fmt::Display for ErrorCodecAdmision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorCodecAdmision {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaccion::captura::{ConfigRawCapturada, materializar_snapshot_desde_entradas};
    use crate::transaccion::snapshot::EntradaSnapshot;
    use universo_core::Delta;

    const MANIFIESTO: &str = r#"version = "admission-codec-test/1"

[nodos]
sortes = ["a", "Retirado", "Patrón"]

[nodos.disco]
"Patrón" = "pattern"

[layout]
directorios = ["knowledge"]
sin_iota = []
ignorados = []
"#;

    fn contexto(reverso: bool) -> ContextoAdmision {
        let manifiesto = MANIFIESTO.as_bytes();
        let invariantes = b"\n";
        let mut entradas = vec![
            EntradaSnapshot::archivo("manifest.toml", manifiesto),
            EntradaSnapshot::archivo("invariants.smt2", invariantes.as_slice()),
            EntradaSnapshot::directorio("knowledge"),
            EntradaSnapshot::directorio("knowledge/empty"),
            EntradaSnapshot::archivo(
                "knowledge/a.md",
                b"---\ntype: a\ntitle: A\n---\n".as_slice(),
            ),
            EntradaSnapshot::archivo("zero.bin", b"".as_slice()),
            EntradaSnapshot::archivo("opaque.bin", [0x00_u8, 0xff, 0x80].as_slice()),
        ];
        if reverso {
            entradas.reverse();
        }
        let configuracion = ConfigRawCapturada::nueva(
            manifiesto,
            invariantes.as_slice(),
            Some("manifest.toml".to_owned()),
            Some("invariants.smt2".to_owned()),
        )
        .unwrap();
        let snapshot = materializar_snapshot_desde_entradas(
            entradas,
            configuracion,
            "engine-codec/1",
            "policy-codec/1",
        )
        .unwrap();
        ContextoAdmision::capturar(&snapshot, &Delta::default()).unwrap()
    }

    fn contiene(haystack: &[u8], needle: &[u8]) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    }

    #[test]
    fn codifica_determinista_raw_y_framing_de_version_unica() {
        let primero = codificar(&contexto(false)).unwrap();
        let permutado = codificar(&contexto(true)).unwrap();

        assert_eq!(primero, permutado);
        assert_eq!(&primero[..MAGIC.len()], MAGIC);
        assert_eq!(u16::from_be_bytes([primero[12], primero[13]]), VERSION);
        let longitud = u64::from_be_bytes(primero[14..22].try_into().unwrap());
        assert_eq!(longitud, primero.len() as u64);

        let ruta_directorio = b"knowledge/empty";
        let mut directorio_vacio = vec![0];
        directorio_vacio.extend_from_slice(&(ruta_directorio.len() as u32).to_be_bytes());
        directorio_vacio.extend_from_slice(ruta_directorio);
        assert!(contiene(&primero, &directorio_vacio));

        let ruta_archivo = b"zero.bin";
        let mut archivo_vacio = vec![1];
        archivo_vacio.extend_from_slice(&(ruta_archivo.len() as u32).to_be_bytes());
        archivo_vacio.extend_from_slice(ruta_archivo);
        archivo_vacio.extend_from_slice(&0_u64.to_be_bytes());
        assert!(contiene(&primero, &archivo_vacio));
        assert!(contiene(&primero, &[0x00, 0xff, 0x80]));
        assert!(contiene(&primero, &[1, 0, 0, 0, 13, b'm']));
    }

    #[test]
    fn preflight_total_es_inclusivo_y_detecta_overflow_sin_reservar() {
        let contexto = contexto(false);
        let (longitud, _) = precalcular_tamano(&contexto, MAX_CODEC_BYTES).unwrap();
        assert_eq!(precalcular_tamano(&contexto, longitud).unwrap().0, longitud);
        assert_eq!(
            precalcular_tamano(&contexto, longitud - 1).unwrap_err(),
            ErrorCodecAdmision::TotalExcedeLimite
        );
        assert!(validar_limite_total(MAX_CODEC_BYTES, MAX_CODEC_BYTES).is_ok());
        assert_eq!(
            validar_limite_total(MAX_CODEC_BYTES + 1, MAX_CODEC_BYTES),
            Err(ErrorCodecAdmision::TotalExcedeLimite)
        );

        let mut total = usize::MAX;
        assert_eq!(
            sumar(&mut total, 1, MAX_CODEC_BYTES),
            Err(ErrorCodecAdmision::LongitudExcedida)
        );
    }
}
