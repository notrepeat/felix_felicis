//! Preflight borrowed del framing binario; no crea Snapshot ni copia payloads.

use crate::transaccion::limites::{LimitesInventario, PRESUPUESTO_BYTES_RAW};
use crate::transaccion::snapshot::{ErrorSnapshot, TipoEntradaSnapshot, validar_ruta_snapshot};

use super::super::{MAX_DELTA_BYTES, MAX_METADATA_BYTES};
use super::cursor::Cursor;
use super::{ErrorCodecAdmision, MAGIC, MAX_CODEC_BYTES, VERSION};

const HEADER_BYTES: usize = MAGIC.len() + 2 + 8 + 4;

/// Vistas prestadas de los valores raw admitidos por el framing.
pub(super) struct ContextoWire<'a> {
    pub(super) entradas: Vec<EntradaWire<'a>>,
    pub(super) manifiesto_raw: &'a [u8],
    pub(super) ruta_manifiesto_root: Option<&'a str>,
    pub(super) invariantes_raw: &'a [u8],
    pub(super) ruta_invariantes_root: Option<&'a str>,
    pub(super) delta_json_v1: &'a str,
    pub(super) version_motor: &'a str,
    pub(super) policy_id: &'a str,
}

pub(super) struct EntradaWire<'a> {
    pub(super) tipo: TipoEntradaSnapshot,
    pub(super) ruta: &'a str,
    pub(super) bytes: Option<&'a [u8]>,
}

#[derive(Clone, Copy)]
struct CuotasLectura {
    total: usize,
    entradas: usize,
    arbol_raw: usize,
    configuracion_raw: usize,
    delta: usize,
    rutas_arbol: usize,
    rutas_configuracion: usize,
    profundidad: usize,
    metadata: usize,
}

impl CuotasLectura {
    const CONTRATO: Self = Self {
        total: MAX_CODEC_BYTES,
        entradas: LimitesInventario::CONTRATO.entradas,
        arbol_raw: LimitesInventario::CONTRATO.bytes_raw,
        configuracion_raw: PRESUPUESTO_BYTES_RAW,
        delta: MAX_DELTA_BYTES,
        rutas_arbol: LimitesInventario::CONTRATO.bytes_rutas,
        rutas_configuracion: LimitesInventario::CONTRATO.bytes_rutas,
        profundidad: LimitesInventario::CONTRATO.profundidad,
        metadata: MAX_METADATA_BYTES,
    };
}

/// Valida framing, marcadores de procedencia y cuotas antes de materializar.
pub(super) fn preflight(bytes: &[u8]) -> Result<ContextoWire<'_>, ErrorCodecAdmision> {
    parsear_con_limites(bytes, CuotasLectura::CONTRATO)
}

fn parsear_con_limites(
    bytes: &[u8],
    cuotas: CuotasLectura,
) -> Result<ContextoWire<'_>, ErrorCodecAdmision> {
    if bytes.len() > cuotas.total {
        return Err(ErrorCodecAdmision::TotalExcedeLimite);
    }
    if bytes.len() < HEADER_BYTES {
        return Err(ErrorCodecAdmision::FramingInvalido);
    }

    let mut cursor = Cursor::nuevo(bytes);
    if cursor.take(MAGIC.len())? != MAGIC {
        return Err(ErrorCodecAdmision::FramingInvalido);
    }
    if cursor.u16()? != VERSION {
        return Err(ErrorCodecAdmision::VersionNoSoportada);
    }
    let total = usize::try_from(cursor.u64()?).map_err(|_| ErrorCodecAdmision::LongitudExcedida)?;
    if total > cuotas.total {
        return Err(ErrorCodecAdmision::TotalExcedeLimite);
    }
    if total != bytes.len() {
        return Err(ErrorCodecAdmision::FramingInvalido);
    }

    let count = usize::try_from(cursor.u32()?).map_err(|_| ErrorCodecAdmision::LongitudExcedida)?;
    if count > cuotas.entradas {
        return Err(ErrorCodecAdmision::LimiteComponenteExcedido);
    }
    validar_capacidad_minima(count, cursor.posicion(), bytes.len())?;
    let mut entradas = Vec::new();
    entradas
        .try_reserve_exact(count)
        .map_err(|_| ErrorCodecAdmision::ReservaFallida)?;

    let mut bytes_arbol = 0;
    let mut bytes_rutas = 0;
    let mut previa: Option<&str> = None;
    for _ in 0..count {
        let tipo = match cursor.u8()? {
            0 => TipoEntradaSnapshot::Directorio,
            1 => TipoEntradaSnapshot::Archivo,
            _ => return Err(ErrorCodecAdmision::FramingInvalido),
        };
        let ruta = cursor.utf8_u32_bounded(&mut bytes_rutas, cuotas.rutas_arbol)?;
        validar_ruta(ruta)?;
        if previa.is_some_and(|anterior| anterior.as_bytes() >= ruta.as_bytes()) {
            return Err(ErrorCodecAdmision::FramingInvalido);
        }
        previa = Some(ruta);
        let profundidad = ruta
            .bytes()
            .filter(|byte| *byte == b'/')
            .count()
            .checked_add(1)
            .ok_or(ErrorCodecAdmision::LongitudExcedida)?;
        if profundidad > cuotas.profundidad {
            return Err(ErrorCodecAdmision::LimiteComponenteExcedido);
        }

        let contenido = match tipo {
            TipoEntradaSnapshot::Directorio => None,
            TipoEntradaSnapshot::Archivo => {
                let longitud = cursor.longitud_u64()?;
                cobrar(&mut bytes_arbol, longitud, cuotas.arbol_raw)?;
                Some(cursor.take(longitud)?)
            }
        };
        entradas.push(EntradaWire {
            tipo,
            ruta,
            bytes: contenido,
        });
    }

    let mut bytes_configuracion = 0;
    let manifiesto_raw = cursor.bytes_u64(&mut bytes_configuracion, cuotas.configuracion_raw)?;
    let mut bytes_rutas_configuracion = 0;
    let ruta_manifiesto_root = ruta_root(&mut cursor, &mut bytes_rutas_configuracion, cuotas)?;
    let invariantes_raw = cursor.bytes_u64(&mut bytes_configuracion, cuotas.configuracion_raw)?;
    let ruta_invariantes_root = ruta_root(&mut cursor, &mut bytes_rutas_configuracion, cuotas)?;
    if ruta_manifiesto_root.is_some() && ruta_manifiesto_root == ruta_invariantes_root {
        return Err(ErrorCodecAdmision::FramingInvalido);
    }

    let delta_json_v1 = cursor.utf8_u64(cuotas.delta)?;
    let mut metadata = 0;
    let version_motor = cursor.utf8_u32_bounded(&mut metadata, cuotas.metadata)?;
    let policy_id = cursor.utf8_u32_bounded(&mut metadata, cuotas.metadata)?;
    cursor.terminar()?;

    Ok(ContextoWire {
        entradas,
        manifiesto_raw,
        ruta_manifiesto_root,
        invariantes_raw,
        ruta_invariantes_root,
        delta_json_v1,
        version_motor,
        policy_id,
    })
}

fn validar_capacidad_minima(
    count: usize,
    posicion: usize,
    total: usize,
) -> Result<(), ErrorCodecAdmision> {
    let bytes_minimos = count
        .checked_mul(6)
        .ok_or(ErrorCodecAdmision::LongitudExcedida)?;
    if bytes_minimos > total.saturating_sub(posicion) {
        return Err(ErrorCodecAdmision::FramingInvalido);
    }
    Ok(())
}

fn validar_ruta(ruta: &str) -> Result<(), ErrorCodecAdmision> {
    validar_ruta_snapshot(ruta).map_err(|_: ErrorSnapshot| ErrorCodecAdmision::FramingInvalido)
}

fn cobrar(total: &mut usize, incremento: usize, limite: usize) -> Result<(), ErrorCodecAdmision> {
    let nuevo_total = total
        .checked_add(incremento)
        .ok_or(ErrorCodecAdmision::LongitudExcedida)?;
    if nuevo_total > limite {
        return Err(ErrorCodecAdmision::LimiteComponenteExcedido);
    }
    *total = nuevo_total;
    Ok(())
}

fn ruta_root<'a>(
    cursor: &mut Cursor<'a>,
    usados: &mut usize,
    cuotas: CuotasLectura,
) -> Result<Option<&'a str>, ErrorCodecAdmision> {
    match cursor.u8()? {
        0 => Ok(None),
        1 => {
            let ruta = cursor.utf8_u32_bounded(usados, cuotas.rutas_configuracion)?;
            validar_ruta(ruta)?;
            Ok(Some(ruta))
        }
        _ => Err(ErrorCodecAdmision::FramingInvalido),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod robustez_tests;

    #[derive(Clone, Copy)]
    struct EntradaFixture<'a> {
        tag: u8,
        ruta: &'a [u8],
        contenido: Option<&'a [u8]>,
    }

    struct ConfigFixture<'a> {
        manifiesto: &'a [u8],
        ruta_manifiesto: Option<&'a str>,
        invariantes: &'a [u8],
        ruta_invariantes: Option<&'a str>,
        delta: &'a [u8],
        motor: &'a [u8],
        policy: &'a [u8],
    }

    fn entrada<'a>(tag: u8, ruta: &'a [u8], contenido: Option<&'a [u8]>) -> EntradaFixture<'a> {
        EntradaFixture {
            tag,
            ruta,
            contenido,
        }
    }

    fn directorio(ruta: &[u8]) -> EntradaFixture<'_> {
        entrada(0, ruta, None)
    }

    fn archivo<'a>(ruta: &'a [u8], contenido: &'a [u8]) -> EntradaFixture<'a> {
        entrada(1, ruta, Some(contenido))
    }

    fn config<'a>(
        manifiesto: &'a [u8],
        ruta_manifiesto: Option<&'a str>,
        invariantes: &'a [u8],
        ruta_invariantes: Option<&'a str>,
        delta: &'a [u8],
        motor: &'a [u8],
        policy: &'a [u8],
    ) -> ConfigFixture<'a> {
        ConfigFixture {
            manifiesto,
            ruta_manifiesto,
            invariantes,
            ruta_invariantes,
            delta,
            motor,
            policy,
        }
    }

    fn wire(entradas: &[EntradaFixture<'_>], configuracion: ConfigFixture<'_>) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&VERSION.to_be_bytes());
        bytes.extend_from_slice(&0_u64.to_be_bytes());
        bytes.extend_from_slice(&(entradas.len() as u32).to_be_bytes());
        for entrada in entradas {
            bytes.push(entrada.tag);
            agregar_u32_bytes(&mut bytes, entrada.ruta);
            if entrada.tag == 1 {
                agregar_u64_bytes(&mut bytes, entrada.contenido.unwrap_or_default());
            }
        }
        agregar_u64_bytes(&mut bytes, configuracion.manifiesto);
        agregar_ruta_root(&mut bytes, configuracion.ruta_manifiesto);
        agregar_u64_bytes(&mut bytes, configuracion.invariantes);
        agregar_ruta_root(&mut bytes, configuracion.ruta_invariantes);
        agregar_u64_bytes(&mut bytes, configuracion.delta);
        agregar_u32_bytes(&mut bytes, configuracion.motor);
        agregar_u32_bytes(&mut bytes, configuracion.policy);
        set_total(&mut bytes);
        bytes
    }

    fn agregar_u32_bytes(bytes: &mut Vec<u8>, contenido: &[u8]) {
        bytes.extend_from_slice(&(contenido.len() as u32).to_be_bytes());
        bytes.extend_from_slice(contenido);
    }

    fn agregar_u64_bytes(bytes: &mut Vec<u8>, contenido: &[u8]) {
        bytes.extend_from_slice(&(contenido.len() as u64).to_be_bytes());
        bytes.extend_from_slice(contenido);
    }

    fn agregar_ruta_root(bytes: &mut Vec<u8>, ruta: Option<&str>) {
        match ruta {
            None => bytes.push(0),
            Some(ruta) => {
                bytes.push(1);
                agregar_u32_bytes(bytes, ruta.as_bytes());
            }
        }
    }

    fn set_total(bytes: &mut [u8]) {
        let total = (bytes.len() as u64).to_be_bytes();
        bytes[14..22].copy_from_slice(&total);
    }

    fn fixture_minimo() -> Vec<u8> {
        wire(&[], config(b"", None, b"", None, b"{}", b"e", b"p"))
    }

    fn error(bytes: &[u8]) -> ErrorCodecAdmision {
        preflight(bytes).err().unwrap()
    }

    fn rechaza(bytes: &[u8], esperado: ErrorCodecAdmision) {
        assert_eq!(error(bytes), esperado);
    }

    #[test]
    fn preflight_devuelve_vistas_prestadas_y_preserva_directorio_y_bytes_raw() {
        let entries = [
            directorio(b"a"),
            archivo(b"b", b""),
            archivo(b"c", &[0xff, 0]),
        ];
        let bytes = wire(
            &entries,
            config(
                b"M",
                Some("cfg/manifest.toml"),
                b"IV",
                Some("cfg/invariants.toml"),
                b"{}",
                b"engine",
                b"pol",
            ),
        );
        let vista = preflight(&bytes).unwrap();
        assert_eq!(vista.entradas[0].tipo, TipoEntradaSnapshot::Directorio);
        assert_eq!(vista.entradas[0].bytes, None);
        assert_eq!(vista.entradas[1].tipo, TipoEntradaSnapshot::Archivo);
        assert_eq!(vista.entradas[1].bytes, Some(&[][..]));
        let raw_pos = bytes.windows(2).position(|pair| pair == [0xff, 0]).unwrap();
        assert_eq!(
            vista.entradas[2].bytes.unwrap().as_ptr(),
            bytes[raw_pos..].as_ptr()
        );
        assert_eq!(vista.manifiesto_raw, b"M");
        assert_eq!(vista.ruta_manifiesto_root, Some("cfg/manifest.toml"));
        assert_eq!(vista.invariantes_raw, b"IV");
        assert_eq!(vista.ruta_invariantes_root, Some("cfg/invariants.toml"));
        assert_eq!(vista.delta_json_v1, "{}");
        assert_eq!(vista.version_motor, "engine");
        assert_eq!(vista.policy_id, "pol");
    }

    #[test]
    fn preflight_rechaza_truncamiento_version_total_y_trailing() {
        let bytes = fixture_minimo();
        for longitud in 0..bytes.len() {
            assert!(preflight(&bytes[..longitud]).is_err(), "prefijo {longitud}");
        }
        let mut firma = bytes.clone();
        firma[0] ^= 1;
        rechaza(&firma, ErrorCodecAdmision::FramingInvalido);
        let mut version = bytes.clone();
        version[13] = 2;
        rechaza(&version, ErrorCodecAdmision::VersionNoSoportada);
        let mut total = bytes.clone();
        total[21] = total[21].wrapping_sub(1);
        rechaza(&total, ErrorCodecAdmision::FramingInvalido);
        let mut trailing = bytes;
        trailing.push(0);
        set_total(&mut trailing);
        rechaza(&trailing, ErrorCodecAdmision::FramingInvalido);
    }
}
