//! Coordinador de una transición A/B/C por llamada, sin retry implícito.
use super::DuenioTransaccionAnclado;
use super::observar_siblings_nuevo_flujo::FormaObservadaSlotsRetiroV2;
use super::retiro_c_v3::retirar::{EstadoRetiroCV3, ResultadoRetiroCV3};
use crate::transaccion::anclado::identidad_archivo;
use crate::transaccion::anclado::intent_persistido_v3::{
    IntentRetiroPersistidoV3, archivo::IntentV3EnArchivo,
};
use crate::transaccion::journal::ArtifactFingerprint;
use std::fs::File;

#[path = "retiro_v3/sidecar.rs"]
mod sidecar;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EstadoPasoRetiroV3 {
    AvanzoB,
    AvanzoC,
    Finalizado,
    Ausente,
    Conflicto,
    Indeterminado,
}

pub(crate) struct ResultadoPasoRetiroV3<'owner, 'lock> {
    contenido: ContenidoResultadoPaso<'owner, 'lock>,
}

enum ContenidoResultadoPaso<'owner, 'lock> {
    Final(ResultadoRetiroCV3<'owner, 'lock>),
    Sidecar {
        owner: &'owner mut DuenioTransaccionAnclado<'lock>,
        intent: IntentRetiroPersistidoV3,
        estado: EstadoPasoRetiroV3,
    },
}

pub(crate) struct EvidenciaRetiroPreservada(IntentRetiroPersistidoV3);

impl EvidenciaRetiroPreservada {
    #[cfg(test)]
    pub(crate) fn contexto_para_test(&self) -> &[u8] {
        self.0.contexto_original()
    }
}

impl ResultadoPasoRetiroV3<'_, '_> {
    pub(crate) fn into_evidencia(self) -> EvidenciaRetiroPreservada {
        EvidenciaRetiroPreservada(match self.contenido {
            ContenidoResultadoPaso::Sidecar { intent, .. } => intent,
            ContenidoResultadoPaso::Final(result) => result.into_intent(),
        })
    }

    pub(crate) fn estado(&self) -> EstadoPasoRetiroV3 {
        match &self.contenido {
            ContenidoResultadoPaso::Sidecar { estado, .. } => *estado,
            ContenidoResultadoPaso::Final(result) => match result.estado() {
                EstadoRetiroCV3::Eliminado => EstadoPasoRetiroV3::Finalizado,
                EstadoRetiroCV3::Ausente => EstadoPasoRetiroV3::Ausente,
                EstadoRetiroCV3::Conflicto => EstadoPasoRetiroV3::Conflicto,
                EstadoRetiroCV3::Indeterminado => EstadoPasoRetiroV3::Indeterminado,
            },
        }
    }
}

#[derive(Debug)]
pub(crate) struct ErrorPasoRetiroV3;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fase {
    A,
    B,
    C,
}

struct Observacion {
    fase: Fase,
    intent: IntentV3EnArchivo,
    journal: Option<File>,
    contexto: Option<File>,
}

impl<'lock> DuenioTransaccionAnclado<'lock> {
    pub(crate) fn avanzar_retiro_v3(
        &mut self,
    ) -> Result<ResultadoPasoRetiroV3<'_, 'lock>, ErrorPasoRetiroV3> {
        self.avanzar_retiro_v3_con_hooks(|| {}, super::intent_retiro::retirar_nombre_plataforma)
    }

    fn avanzar_retiro_v3_con_hooks<H, F>(
        &mut self,
        antes: H,
        intento: F,
    ) -> Result<ResultadoPasoRetiroV3<'_, 'lock>, ErrorPasoRetiroV3>
    where
        H: FnOnce(),
        F: FnOnce(&File, &std::ffi::OsStr, File) -> Result<(), ()>,
    {
        let inicial = self.observar_abc_v3()?;
        if inicial.fase == Fase::C {
            drop(inicial);
            let proof = self
                .observar_retiro_c_v3_consistente()
                .map_err(|_| ErrorPasoRetiroV3)?;
            return Ok(ResultadoPasoRetiroV3 {
                contenido: ContenidoResultadoPaso::Final(proof.retirar()),
            });
        }
        let Observacion {
            fase,
            intent: inicial,
            journal,
            contexto,
        } = inicial;
        drop((journal, contexto));
        antes();
        let estado = self.intentar_paso_sidecar(fase, &inicial, intento);
        let IntentV3EnArchivo { archivo, intent } = inicial;
        drop(archivo);
        Ok(ResultadoPasoRetiroV3 {
            contenido: ContenidoResultadoPaso::Sidecar {
                owner: self,
                intent,
                estado,
            },
        })
    }

    fn intentar_paso_sidecar<F>(
        &mut self,
        fase: Fase,
        inicial: &IntentV3EnArchivo,
        intento: F,
    ) -> EstadoPasoRetiroV3
    where
        F: FnOnce(&File, &std::ffi::OsStr, File) -> Result<(), ()>,
    {
        use EstadoPasoRetiroV3::*;
        if crate::transaccion::anclado::journal_temporal::raiz_es_remota(self._lock.raiz_canonica())
        {
            return Conflicto;
        }
        let Ok(actual) = self.observar_abc_v3() else {
            return Conflicto;
        };
        if actual.fase != fase || !mismo_intent(inicial, &actual.intent) {
            return Conflicto;
        }
        let Ok(nombre) = sidecar::nombre(self, fase) else {
            return Conflicto;
        };
        let Ok(parent) = self.parent_revalidado() else {
            return Conflicto;
        };
        let Observacion {
            journal,
            contexto,
            intent: _,
            ..
        } = actual;
        let fuente = if fase == Fase::A {
            drop(contexto);
            journal
        } else {
            drop(journal);
            contexto
        };
        let Some(fuente) = fuente else {
            return Conflicto;
        };
        #[cfg(windows)]
        let fuente = {
            drop(fuente);
            let Ok(file) = super::intent_retiro::abrir_final_con_delete(&parent, &nombre) else {
                return Indeterminado;
            };
            let Ok(file) = sidecar::verificar(file, self, fase, &inicial.intent) else {
                return Conflicto;
            };
            file
        };
        if self.parent_revalidado().is_err() {
            return Conflicto;
        }
        let _resultado_syscall = intento(&parent, &nombre, fuente);
        drop(parent);
        let siguiente = self.observar_abc_v3();
        if self.parent_revalidado().is_err() {
            return Indeterminado;
        }
        match siguiente {
            Ok(next) if !mismo_intent(inicial, &next.intent) => Conflicto,
            Ok(next) if fase == Fase::A && next.fase == Fase::B => AvanzoB,
            Ok(next) if fase == Fase::B && next.fase == Fase::C => AvanzoC,
            _ => Indeterminado,
        }
    }

    fn observar_abc_v3(&mut self) -> Result<Observacion, ErrorPasoRetiroV3> {
        self.parent_revalidado().map_err(|_| ErrorPasoRetiroV3)?;
        let result = self.observar_abc_v3_interno();
        self.parent_revalidado().map_err(|_| ErrorPasoRetiroV3)?;
        result
    }

    fn observar_abc_v3_interno(&mut self) -> Result<Observacion, ErrorPasoRetiroV3> {
        let slots = self
            .observar_siblings_nuevo_flujo()
            .map_err(|_| ErrorPasoRetiroV3)?;
        let fase = match slots
            .clasificar_forma_slots_retiro_v2()
            .map_err(|_| ErrorPasoRetiroV3)?
        {
            FormaObservadaSlotsRetiroV2::RetiroA => Fase::A,
            FormaObservadaSlotsRetiroV2::RetiroB => Fase::B,
            FormaObservadaSlotsRetiroV2::RetiroC => Fase::C,
        };
        let intent = self.observar_intent_v3().map_err(|_| ErrorPasoRetiroV3)?;
        let replay = intent
            .intent
            .calcular_new_esperado()
            .map_err(|_| ErrorPasoRetiroV3)?;
        let raw = self
            .observar_siblings_recovery_v2_raw(replay.journal())
            .map_err(|_| ErrorPasoRetiroV3)?;
        let expected = ArtifactFingerprint::new(
            replay.journal().new_fingerprint().identity().clone(),
            replay.digest().clone(),
        );
        if raw.root() != Some(&expected) || raw.backup().is_some() || raw.stage().is_some() {
            return Err(ErrorPasoRetiroV3);
        }
        let journal = if fase == Fase::A {
            Some(sidecar::abrir(self, Fase::A, &intent.intent)?)
        } else {
            None
        };
        let contexto = if fase != Fase::C {
            Some(sidecar::abrir(self, Fase::B, &intent.intent)?)
        } else {
            None
        };
        if self
            .observar_siblings_nuevo_flujo()
            .map_err(|_| ErrorPasoRetiroV3)?
            != slots
        {
            return Err(ErrorPasoRetiroV3);
        }
        let fresca = self.observar_intent_v3().map_err(|_| ErrorPasoRetiroV3)?;
        if !mismo_intent(&intent, &fresca) {
            return Err(ErrorPasoRetiroV3);
        }
        Ok(Observacion {
            fase,
            intent: fresca,
            journal,
            contexto,
        })
    }
}

fn mismo_intent(a: &IntentV3EnArchivo, b: &IntentV3EnArchivo) -> bool {
    matches!((identidad_archivo(&a.archivo), identidad_archivo(&b.archivo)), (Ok(a), Ok(b)) if a == b)
        && a.intent.metadata() == b.intent.metadata()
        && a.intent.contexto_original() == b.intent.contexto_original()
}

#[cfg(test)]
#[path = "retiro_v3/tests.rs"]
mod tests;
