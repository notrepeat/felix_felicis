use std::collections::HashMap;

use crate::{AristaId, Estructura, IdArista, NodoId, Sorte};

type GrupoArista = (NodoId, Sorte, NodoId);

#[derive(Default)]
struct IndiceGrupo {
    ordinales: HashMap<u32, AristaId>,
    cantidad: usize,
}

/// Índice efímero para resolver muchos IDs contra el mismo snapshot sin
/// volver a recorrer las aristas salientes por cada operación.
pub(super) struct IndiceResolucion {
    grupos: HashMap<GrupoArista, IndiceGrupo>,
}

impl IndiceResolucion {
    pub(super) fn nuevo(snapshot: &Estructura) -> Self {
        Self::nuevo_observando(snapshot, || {})
    }

    pub(super) fn nuevo_observando(
        snapshot: &Estructura,
        mut observar_arista: impl FnMut(),
    ) -> Self {
        let mut grupos = HashMap::<GrupoArista, IndiceGrupo>::new();
        for (origen, _) in snapshot.nodos() {
            for &handle in snapshot.salientes(origen) {
                observar_arista();
                if let Some(arista) = snapshot.arista(handle) {
                    let grupo = grupos
                        .entry((arista.origen, arista.tipo.clone(), arista.destino))
                        .or_default();
                    grupo.cantidad += 1;
                    grupo.ordinales.entry(arista.ordinal).or_insert(handle);
                }
            }
        }
        Self { grupos }
    }

    pub(super) fn resolver(&self, snapshot: &Estructura, id: &IdArista) -> Option<AristaId> {
        self.resolver_observando(snapshot, id, || {})
    }

    pub(super) fn resolver_observando(
        &self,
        snapshot: &Estructura,
        id: &IdArista,
        mut observar_busqueda: impl FnMut(),
    ) -> Option<AristaId> {
        observar_busqueda();
        let origen = snapshot.por_path(&id.origen)?;
        let destino = snapshot.por_path(&id.destino)?;
        self.resolver_grupo(origen, &id.sorte, destino, id.ordinal)
    }

    pub(super) fn resolver_grupo(
        &self,
        origen: NodoId,
        sorte: &Sorte,
        destino: NodoId,
        ordinal: u32,
    ) -> Option<AristaId> {
        self.grupos
            .get(&(origen, sorte.clone(), destino))?
            .ordinales
            .get(&ordinal)
            .copied()
    }

    pub(super) fn resolver_grupo_observando(
        &self,
        origen: NodoId,
        sorte: &Sorte,
        destino: NodoId,
        ordinal: u32,
        mut observar_busqueda: impl FnMut(),
    ) -> Option<AristaId> {
        observar_busqueda();
        self.resolver_grupo(origen, sorte, destino, ordinal)
    }

    pub(super) fn cantidad_grupo(&self, origen: NodoId, sorte: &Sorte, destino: NodoId) -> usize {
        self.grupos
            .get(&(origen, sorte.clone(), destino))
            .map_or(0, |grupo| grupo.cantidad)
    }

    pub(super) fn cantidad_grupo_observando(
        &self,
        origen: NodoId,
        sorte: &Sorte,
        destino: NodoId,
        mut observar_busqueda: impl FnMut(),
    ) -> usize {
        observar_busqueda();
        self.cantidad_grupo(origen, sorte, destino)
    }

    pub(super) fn registrar(&mut self, estructura: &Estructura, handle: AristaId) {
        let Some(arista) = estructura.arista(handle) else {
            return;
        };
        let grupo = self
            .grupos
            .entry((arista.origen, arista.tipo.clone(), arista.destino))
            .or_default();
        grupo.cantidad += 1;
        grupo.ordinales.entry(arista.ordinal).or_insert(handle);
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BundlePath, Etiqueta};

    fn path(valor: &str) -> BundlePath {
        BundlePath::nuevo(valor).unwrap()
    }

    #[test]
    fn indice_de_grupos_resuelve_un_lote_con_un_scan_y_una_busqueda_por_id() {
        let mut estructura = Estructura::nueva(crate::manifiesto::tests::base()).unwrap();
        let origen = estructura
            .insertar_nodo(path("n/origen"), Sorte::nuevo("a"), Etiqueta::nuevo("o"))
            .unwrap();
        let destino = estructura
            .insertar_nodo(path("n/destino"), Sorte::nuevo("a"), Etiqueta::nuevo("d"))
            .unwrap();
        let cantidad = 2_048;
        let handles: Vec<_> = (0..cantidad)
            .map(|_| {
                estructura
                    .insertar_arista(origen, destino, Sorte::nuevo("rel"), Etiqueta::nuevo(""))
                    .unwrap()
            })
            .collect();
        let ids: Vec<_> = handles
            .iter()
            .map(|handle| identidad(&estructura, *handle).unwrap())
            .collect();

        let mut aristas_visitadas = 0;
        let indice = IndiceResolucion::nuevo_observando(&estructura, || aristas_visitadas += 1);
        assert_eq!(aristas_visitadas, cantidad);

        let mut busquedas = 0;
        for (id, handle) in ids.iter().zip(handles) {
            assert_eq!(
                indice.resolver_observando(&estructura, id, || busquedas += 1),
                Some(handle)
            );
        }
        assert_eq!(busquedas, cantidad);
        assert_eq!(
            indice.resolver(
                &estructura,
                &IdArista {
                    origen: path("n/origen"),
                    sorte: Sorte::nuevo("rel"),
                    destino: path("n/destino"),
                    ordinal: cantidad as u32,
                }
            ),
            None
        );
    }

    fn identidad(estructura: &Estructura, handle: AristaId) -> Option<IdArista> {
        let arista = estructura.arista(handle)?;
        Some(IdArista {
            origen: estructura.nodo(arista.origen)?.path.clone(),
            sorte: arista.tipo.clone(),
            destino: estructura.nodo(arista.destino)?.path.clone(),
            ordinal: arista.ordinal,
        })
    }
}
