use crate::{AristaId, Estructura, IdArista};

pub(super) fn resolver(snapshot: &Estructura, id: &IdArista) -> Option<AristaId> {
    let origen = snapshot.por_path(&id.origen)?;
    let destino = snapshot.por_path(&id.destino)?;
    snapshot.salientes(origen).iter().copied().find(|handle| {
        snapshot.arista(*handle).is_some_and(|arista| {
            arista.tipo == id.sorte && arista.destino == destino && arista.ordinal == id.ordinal
        })
    })
}

pub(super) fn retirar_lote(
    estructura: &mut Estructura,
    handles: &[AristaId],
) -> Result<(), crate::ErrorEstructura> {
    estructura.quitar_aristas_en_lote(handles)
}
