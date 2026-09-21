//! Identidades generacionales de la arena. `NodoId` y `AristaId` son tipos
//! nominales distintos: la firma bisortida del cap. 01 la verifica el
//! compilador, no una convención (plan 01, S1).
slotmap::new_key_type! {
    pub struct NodoId;
    pub struct AristaId;
}
