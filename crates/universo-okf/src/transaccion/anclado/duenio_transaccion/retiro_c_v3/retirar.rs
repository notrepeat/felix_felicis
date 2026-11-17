//! Una tentativa de retiro después de reproducir y revalidar C.
use std::ffi::OsStr;
use std::fs::File;

use super::{DuenioTransaccionAnclado, ObservacionRetiroCV3};
use crate::transaccion::anclado::duenio_transaccion::observar_siblings_nuevo_flujo::{
    PresenciaSiblingNuevoFlujo::Ausente, nombre_intent_retiro,
};
use crate::transaccion::anclado::duenio_transaccion::{intent_retiro, lector_intent_canonico};
use crate::transaccion::anclado::identidad_archivo;
use crate::transaccion::anclado::intent_persistido_v3::{
    IntentRetiroPersistidoV3, archivo::IntentV3EnArchivo,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::transaccion::anclado::duenio_transaccion) enum EstadoRetiroCV3 {
    Eliminado,
    Ausente,
    Conflicto,
    Indeterminado,
}

pub(in crate::transaccion::anclado::duenio_transaccion) struct ResultadoRetiroCV3<'owner, 'lock> {
    owner: &'owner mut DuenioTransaccionAnclado<'lock>,
    intent: IntentRetiroPersistidoV3,
    estado: EstadoRetiroCV3,
}

impl ResultadoRetiroCV3<'_, '_> {
    pub(in crate::transaccion::anclado::duenio_transaccion) fn estado(&self) -> EstadoRetiroCV3 {
        self.estado
    }
}

impl<'owner, 'lock> ObservacionRetiroCV3<'owner, 'lock> {
    pub(in crate::transaccion::anclado::duenio_transaccion) fn retirar(
        self,
    ) -> ResultadoRetiroCV3<'owner, 'lock> {
        self.retirar_con_intento(intent_retiro::retirar_nombre_plataforma)
    }

    #[cfg(test)]
    pub(in crate::transaccion::anclado::duenio_transaccion) fn retirar_con_intento_para_test<F>(
        self,
        intento: F,
    ) -> ResultadoRetiroCV3<'owner, 'lock>
    where
        F: FnOnce(&File, &OsStr, File) -> Result<(), ()>,
    {
        self.retirar_con_intento(intento)
    }

    fn retirar_con_intento<F>(self, intento: F) -> ResultadoRetiroCV3<'owner, 'lock>
    where
        F: FnOnce(&File, &OsStr, File) -> Result<(), ()>,
    {
        let Self { owner, observado } = self;
        let IntentV3EnArchivo { archivo, intent } = observado;
        let id = identidad_archivo(&archivo);
        // Windows requiere soltar lectores anteriores antes del delete por handle.
        drop(archivo);
        let estado = match id {
            Ok(id) => intentar(owner, &intent, id, intento),
            Err(_) => EstadoRetiroCV3::Indeterminado,
        };
        ResultadoRetiroCV3 {
            owner,
            intent,
            estado,
        }
    }
}

fn intentar<F>(
    owner: &mut DuenioTransaccionAnclado<'_>,
    esperado: &IntentRetiroPersistidoV3,
    id: crate::transaccion::anclado::IdentidadArchivo,
    intento: F,
) -> EstadoRetiroCV3
where
    F: FnOnce(&File, &OsStr, File) -> Result<(), ()>,
{
    use EstadoRetiroCV3::*;
    if crate::transaccion::anclado::journal_temporal::raiz_es_remota(owner._lock.raiz_canonica()) {
        return Conflicto;
    }
    let Some(basename) = owner._lock.ubicacion().ruta().file_name() else {
        return Conflicto;
    };
    let Ok(nombre) = nombre_intent_retiro(basename) else {
        return Conflicto;
    };
    let Ok(parent) = owner.parent_revalidado() else {
        return Conflicto;
    };
    match lector_intent_canonico::abrir_intent_canonico(&parent, &nombre) {
        Ok(None) => {
            return if final_ausente(owner, esperado) {
                Ausente
            } else {
                Conflicto
            };
        }
        Ok(Some(file)) => drop(file),
        Err(_) => return Indeterminado,
    }
    drop(parent);
    let Ok(fresca) = owner.observar_retiro_c_v3_datos() else {
        return Conflicto;
    };
    if !coincide(&fresca, esperado, id) {
        return Conflicto;
    }
    let Ok(parent) = owner.parent_revalidado() else {
        return Conflicto;
    };
    #[cfg(windows)]
    let fuente = {
        drop(fresca);
        let Ok(file) = intent_retiro::abrir_final_con_delete(&parent, &nombre) else {
            return Indeterminado;
        };
        let Some(parent_id) = owner.identidad_parent.journal_identity() else {
            return Conflicto;
        };
        let Ok(final_file) =
            crate::transaccion::anclado::intent_persistido_v3::archivo::leer_archivo(
                file,
                owner.scope_parent,
                &parent_id,
            )
        else {
            return Conflicto;
        };
        if !coincide(&final_file, esperado, id) {
            return Conflicto;
        }
        final_file
    };
    #[cfg(not(windows))]
    let fuente = fresca;
    if owner.exigir_slots_c_v3().is_err() || owner.parent_revalidado().is_err() {
        return Conflicto;
    }
    let resultado = intento(&parent, &nombre, fuente.archivo);
    drop(parent);
    // El resultado del syscall jamás se interpreta como ausencia de efecto.
    if owner.parent_revalidado().is_err() {
        return Indeterminado;
    }
    if final_ausente(owner, esperado) {
        return if resultado.is_ok() {
            Eliminado
        } else {
            Ausente
        };
    }
    if owner.exigir_slots_c_v3().is_err() {
        return Indeterminado;
    }
    let reemplazado = owner
        .parent_revalidado()
        .ok()
        .and_then(|parent| {
            lector_intent_canonico::abrir_intent_canonico(&parent, &nombre)
                .ok()
                .flatten()
        })
        .and_then(|file| identidad_archivo(&file).ok())
        .is_some_and(|actual| actual != id);
    if owner.parent_revalidado().is_err() {
        return Indeterminado;
    }
    if reemplazado {
        return Conflicto;
    }
    let actual = owner.observar_intent_v3();
    if owner.parent_revalidado().is_err() {
        return Indeterminado;
    }
    match actual {
        Ok(actual) if !coincide(&actual, esperado, id) => Conflicto,
        _ => Indeterminado,
    }
}

fn coincide(
    actual: &IntentV3EnArchivo,
    esperado: &IntentRetiroPersistidoV3,
    id: crate::transaccion::anclado::IdentidadArchivo,
) -> bool {
    identidad_archivo(&actual.archivo).is_ok_and(|actual| actual == id)
        && actual.intent.metadata() == esperado.metadata()
        && actual.intent.contexto_original() == esperado.contexto_original()
}

fn final_ausente(
    owner: &mut DuenioTransaccionAnclado<'_>,
    esperado: &IntentRetiroPersistidoV3,
) -> bool {
    let resultado = (|| {
        let slots = owner.observar_siblings_nuevo_flujo().ok()?;
        if [
            slots.journal(),
            slots.contexto(),
            slots.intent_retiro(),
            slots.journal_temporal(),
            slots.contexto_temporal(),
            slots.intent_retiro_temporal(),
        ]
        .iter()
        .any(|slot| *slot != Ausente)
        {
            return None;
        }
        let replay = esperado.calcular_new_esperado().ok()?;
        let raw = owner
            .observar_siblings_recovery_v2_raw(replay.journal())
            .ok()?;
        let fingerprint = crate::transaccion::journal::ArtifactFingerprint::new(
            replay.journal().new_fingerprint().identity().clone(),
            replay.digest().clone(),
        );
        if raw.root() != Some(&fingerprint) || raw.backup().is_some() || raw.stage().is_some() {
            return None;
        }
        let slots_finales = owner.observar_siblings_nuevo_flujo().ok()?;
        (slots == slots_finales).then_some(())
    })();
    owner.parent_revalidado().is_ok() && resultado.is_some()
}

#[cfg(test)]
#[path = "retirar_tests.rs"]
mod tests;
