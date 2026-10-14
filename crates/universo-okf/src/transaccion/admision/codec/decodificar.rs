//! Reconstrucción semántica de un contexto desde el wire privado.
//!
//! El preflight valida todo el framing y sus cuotas antes de copiar payloads.
//! Después se vuelve a materializar la base, se comprueba la procedencia raw y
//! se recalculan las identidades derivadas; el wire nunca acredita admisión.

use std::sync::Arc;

use crate::delta::{parsear_delta_json_v1, serializar_delta_json_v1};
use crate::transaccion::captura::{ConfigRawCapturada, materializar_snapshot_desde_entradas};
use crate::transaccion::snapshot::{EntradaSnapshot, TipoEntradaSnapshot};

use super::super::ContextoAdmision;
use super::{ErrorCodecAdmision, lectura::preflight};

/// Decodifica y revalida un contexto privado sin consultar el filesystem.
pub(crate) fn decodificar(bytes: &[u8]) -> Result<ContextoAdmision, ErrorCodecAdmision> {
    let wire = preflight(bytes)?;

    let mut entradas = Vec::with_capacity(wire.entradas.len());
    for entrada in wire.entradas {
        let ruta = entrada.ruta.to_owned();
        let entrada = match (entrada.tipo, entrada.bytes) {
            (TipoEntradaSnapshot::Directorio, None) => EntradaSnapshot::directorio(ruta),
            (TipoEntradaSnapshot::Archivo, Some(raw)) => {
                EntradaSnapshot::archivo(ruta, Arc::<[u8]>::from(raw))
            }
            _ => return Err(ErrorCodecAdmision::ContextoInvalido),
        };
        entradas.push(entrada);
    }

    let configuracion = ConfigRawCapturada::nueva(
        Arc::<[u8]>::from(wire.manifiesto_raw),
        Arc::<[u8]>::from(wire.invariantes_raw),
        wire.ruta_manifiesto_root.map(str::to_owned),
        wire.ruta_invariantes_root.map(str::to_owned),
    )
    .map_err(|_| ErrorCodecAdmision::ContextoInvalido)?;

    let delta = parsear_delta_json_v1(wire.delta_json_v1)
        .map_err(|_| ErrorCodecAdmision::ContextoInvalido)?;
    let delta_canonico =
        serializar_delta_json_v1(&delta).map_err(|_| ErrorCodecAdmision::ContextoInvalido)?;
    if delta_canonico != wire.delta_json_v1 {
        return Err(ErrorCodecAdmision::ContextoInvalido);
    }

    let snapshot = materializar_snapshot_desde_entradas(
        entradas,
        configuracion,
        wire.version_motor,
        wire.policy_id,
    )
    .map_err(|_| ErrorCodecAdmision::ContextoInvalido)?;

    ContextoAdmision::capturar(&snapshot, &delta).map_err(|_| ErrorCodecAdmision::ContextoInvalido)
}

#[cfg(test)]
mod tests {
    use super::ErrorCodecAdmision;
    use crate::transaccion::admision::codec::{codificar, decodificar, tests::contexto_con_rutas};

    fn actualizar_total(bytes: &mut [u8]) {
        let total = u64::try_from(bytes.len()).unwrap().to_be_bytes();
        bytes[14..22].copy_from_slice(&total);
    }

    fn posicion_ultima(bytes: &[u8], needle: &[u8]) -> usize {
        bytes
            .windows(needle.len())
            .rposition(|window| window == needle)
            .unwrap()
    }

    #[test]
    fn roundtrip_reconstruye_contextos_con_rutas_internas_y_externas() {
        for config_root_relative in [true, false] {
            let original = contexto_con_rutas(false, config_root_relative);
            let wire = codificar(&original).unwrap();
            let reconstruido = decodificar(&wire).unwrap();

            assert_eq!(codificar(&reconstruido).unwrap(), wire);
            reconstruido.validar_base().unwrap();
            let configuracion = reconstruido.configuracion();
            if config_root_relative {
                assert_eq!(configuracion.ruta_manifiesto_root(), Some("manifest.toml"));
                assert_eq!(
                    configuracion.ruta_invariantes_root(),
                    Some("invariants.smt2")
                );
            } else {
                assert_eq!(configuracion.ruta_manifiesto_root(), None);
                assert_eq!(configuracion.ruta_invariantes_root(), None);
            }
        }
    }

    #[test]
    fn rechaza_delta_json_parseable_pero_no_canonico() {
        let contexto = contexto_con_rutas(false, true);
        let mut wire = codificar(&contexto).unwrap();
        let delta = contexto.delta_json_v1().as_bytes();
        let posicion = posicion_ultima(&wire, delta);
        let campo_longitud = posicion - 8;
        let longitud = u64::from_be_bytes(wire[campo_longitud..posicion].try_into().unwrap());
        wire.splice(posicion..posicion, [b' ']);
        wire[campo_longitud..campo_longitud + 8].copy_from_slice(&(longitud + 1).to_be_bytes());
        actualizar_total(&mut wire);

        assert!(matches!(
            decodificar(&wire),
            Err(ErrorCodecAdmision::ContextoInvalido)
        ));
    }

    #[test]
    fn rechaza_ruta_de_configuracion_root_que_apunta_a_directorio() {
        let contexto = contexto_con_rutas(false, true);
        let mut wire = codificar(&contexto).unwrap();
        let original = b"manifest.toml";
        let reemplazo = b"knowledge/empty";
        let posicion = posicion_ultima(&wire, original);
        let campo_longitud = posicion - 4;
        wire[campo_longitud..posicion].copy_from_slice(&(reemplazo.len() as u32).to_be_bytes());
        wire.splice(
            posicion..posicion + original.len(),
            reemplazo.iter().copied(),
        );
        actualizar_total(&mut wire);

        assert!(matches!(
            decodificar(&wire),
            Err(ErrorCodecAdmision::ContextoInvalido)
        ));
    }

    #[test]
    fn rechaza_bytes_raw_de_configuracion_distintos_de_su_archivo_root() {
        let contexto = contexto_con_rutas(false, true);
        let mut wire = codificar(&contexto).unwrap();
        let manifiesto = contexto.configuracion().manifiesto_raw().to_vec();
        let posicion = posicion_ultima(&wire, &manifiesto);
        let campo_longitud = posicion - 8;
        let longitud = u64::from_be_bytes(wire[campo_longitud..posicion].try_into().unwrap());
        wire.splice(
            posicion + manifiesto.len()..posicion + manifiesto.len(),
            [b'\n'],
        );
        wire[campo_longitud..campo_longitud + 8].copy_from_slice(&(longitud + 1).to_be_bytes());
        actualizar_total(&mut wire);

        assert!(matches!(
            decodificar(&wire),
            Err(ErrorCodecAdmision::ContextoInvalido)
        ));
    }
}
