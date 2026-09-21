use slotmap::SlotMap;
use universo_core::{AristaId, NodoId};

fn main() {
    let mut nodos: SlotMap<NodoId, ()> = SlotMap::with_key();
    let mut aristas: SlotMap<AristaId, ()> = SlotMap::with_key();
    let _n = nodos.insert(());
    let a = aristas.insert(());
    let _ = nodos.get(a);
}
