//! Selección cerrada del perfil semántico soportado para replay archivado.
//!
//! Este selector solo agrega compatibilidad a un contexto ya materializado.
//! No convierte las etiquetas en autenticación ni concede autoridad para
//! observar o recuperar estado del filesystem.

use crate::transaccion::admision::ContextoAdmision;
use crate::transaccion::journal::Journal;

use super::ErrorReplay;
use super::new_esperado::{ErrorNewEsperado, NewEsperado, calcular_new_esperado};

pub(crate) const VERSION_MOTOR_V1: &str = "felix_felicis:proposal-semantics/v1";
pub(crate) const POLICY_ID_V1: &str = "felix_felicis:admission-policy/v1";

// El tag del motor cubre materialización/captura, Delta y candidato/conflictos,
// expansión/evaluación/cuotas, serialización, staging e inventario/digest. Si
// cambia un resultado observable de replay, se requiere otro tag; el framing
// wire se versiona por separado. La policy acepta solo `Admitida`: cualquier
// rechazo o error falla cerrado y no deriva `NewEsperado`.
//
// Deltas `felix-delta-v2` (notas nuevas, 2026-11-18): el tag no cambia porque
// forma parte de cada token de snapshot y cambiarlo invalidaría tokens y
// journals existentes. Para todo contexto que un motor anterior podía leer (el
// delta se guarda en v1 si no crea notas, byte a byte igual) el replay es
// idéntico. Un contexto con delta v2 sólo lo lee un motor con v2: un binario
// anterior falla cerrado al decodificarlo y su recovery queda bloqueado hasta
// usar el binario nuevo, sin riesgo de reproducir otro candidato.

#[derive(Clone, Copy)]
enum PerfilSemanticoArchivado {
    PropuestaV1AdmitidaV1,
}

impl PerfilSemanticoArchivado {
    fn resolver(contexto: &ContextoAdmision) -> Result<Self, ErrorNewEsperado> {
        if contexto.version_motor() == VERSION_MOTOR_V1 && contexto.policy_id() == POLICY_ID_V1 {
            Ok(Self::PropuestaV1AdmitidaV1)
        } else {
            Err(ErrorNewEsperado::Replay(
                ErrorReplay::MotorPoliticaNoCoinciden,
            ))
        }
    }

    fn calcular_new_esperado(
        self,
        contexto: &ContextoAdmision,
        journal: &Journal,
    ) -> Result<NewEsperado, ErrorNewEsperado> {
        match self {
            Self::PropuestaV1AdmitidaV1 => {
                calcular_new_esperado(contexto, journal, VERSION_MOTOR_V1, POLICY_ID_V1)
            }
        }
    }
}

pub(crate) fn calcular_new_esperado_soportado(
    contexto: &ContextoAdmision,
    journal: &Journal,
) -> Result<NewEsperado, ErrorNewEsperado> {
    PerfilSemanticoArchivado::resolver(contexto)?.calcular_new_esperado(contexto, journal)
}
