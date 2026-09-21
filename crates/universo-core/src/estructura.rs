//! `Estructura`: arena de nodos y aristas más los índices derivados (spec
//! §3.4). Fuente de verdad: las dos arenas (`SlotMap`) y el mapa `padre`;
//! todo lo demás (`salientes`, `entrantes`, `hijos`, `indice`,
//! `indice_case`) es derivado y se mantiene coherente en cada operación.

use std::collections::{BTreeMap, HashMap, HashSet};

use slotmap::{SecondaryMap, SlotMap};

use crate::anexo::Anexo;
use crate::atomos::{Atomo, IdArista, Sujeto};
use crate::ids::{AristaId, NodoId};
use crate::manifiesto::{ErrorManifiesto, Manifiesto};
use crate::path::{BundlePath, ErrorPath};
use crate::tipos::{Etiqueta, Sorte};

/// Nodo de la estructura: su path (identidad), su sorte (tipo) y su
/// etiqueta ($\lambda$).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Nodo {
    pub path: BundlePath,
    pub tipo: Sorte,
    pub etiqueta: Etiqueta,
}

/// Arista de la estructura: origen, destino, tipo (D5: la clave declarada
/// en el manifiesto ES el tipo), etiqueta y su ordinal dentro del grupo
/// `(origen, tipo, destino)` (multigrafo).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arista {
    pub origen: NodoId,
    pub destino: NodoId,
    pub tipo: Sorte,
    pub etiqueta: Etiqueta,
    pub ordinal: u32,
}

/// Arena de nodos y aristas de una instanciación del motor, con los
/// índices derivados que sirven las consultas de la API pública.
///
/// Fuente de verdad: `nodos`, `aristas` (las dos `SlotMap`) y `padre`; todo
/// lo demás se mantiene incrementalmente en cada operación mutadora, así
/// que puede desincronizarse por un error de esta implementación: por eso
/// existe `verificar_representacion`, que lo comprueba contra la fuente de
/// verdad.
#[derive(Debug)]
pub struct Estructura {
    manifiesto: Manifiesto,
    nodos: SlotMap<NodoId, Nodo>,
    aristas: SlotMap<AristaId, Arista>,
    salientes: SecondaryMap<NodoId, Vec<AristaId>>,
    entrantes: SecondaryMap<NodoId, Vec<AristaId>>,
    padre: SecondaryMap<NodoId, NodoId>,
    hijos: SecondaryMap<NodoId, Vec<NodoId>>,
    indice: HashMap<BundlePath, NodoId>,
    indice_case: HashMap<String, NodoId>,
    atributos: SecondaryMap<NodoId, BTreeMap<String, String>>,
    anexos: SecondaryMap<NodoId, Anexo>,
}

/// Motivo de rechazo de una operación sobre `Estructura`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorEstructura {
    /// La sorte de nodo no está declarada en $K_V$.
    SorteNodoDesconocido(String),
    /// La clave de arista no está declarada en $K_E$.
    SorteAristaDesconocido(String),
    /// Ya existe un nodo con ese path exacto.
    PathDuplicado(String),
    /// El path colisiona en mayúsculas/minúsculas con el de otro nodo
    /// (nuevo path, path existente).
    ColisionDeCase(String, String),
    /// El `NodoId` no resuelve a ningún nodo vivo.
    NodoInexistente,
    /// El `AristaId` no resuelve a ninguna arista viva.
    AristaInexistente,
    /// Se intentó asignar un nodo como padre de sí mismo.
    PadreDeSiMismo,
    /// Asignar ese padre crearía un ciclo en $\iota$.
    CicloEnIota,
    /// El predicado no está declarado en el manifiesto.
    PredicadoDesconocido(String),
    /// La constante no pertenece al dominio cerrado del predicado
    /// (predicado, constante).
    ConstanteFueraDeDominio(String, String),
    /// Una `Etiqueta` contiene `\n` o `\r`: una etiqueta debe caber en una
    /// sola línea, tanto en el texto canónico de un átomo como en el
    /// frontmatter.
    EtiquetaInvalida(String),
    /// Un `BundlePath` inválido.
    Path(ErrorPath),
    /// El manifiesto pasado a `nueva` no es válido.
    Manifiesto(Vec<ErrorManifiesto>),
}

impl std::fmt::Display for ErrorEstructura {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorEstructura::SorteNodoDesconocido(s) => {
                write!(
                    f,
                    "la sorte de nodo {s:?} no está declarada en el manifiesto"
                )
            }
            ErrorEstructura::SorteAristaDesconocido(s) => {
                write!(
                    f,
                    "la clave de arista {s:?} no está declarada en el manifiesto"
                )
            }
            ErrorEstructura::PathDuplicado(p) => write!(f, "ya existe un nodo con el path {p:?}"),
            ErrorEstructura::ColisionDeCase(nuevo, existente) => write!(
                f,
                "el path {nuevo:?} colisiona en mayúsculas/minúsculas con {existente:?}"
            ),
            ErrorEstructura::NodoInexistente => write!(f, "el nodo no existe"),
            ErrorEstructura::AristaInexistente => write!(f, "la arista no existe"),
            ErrorEstructura::PadreDeSiMismo => {
                write!(f, "un nodo no puede ser padre de sí mismo")
            }
            ErrorEstructura::CicloEnIota => {
                write!(f, "esa asignación crearía un ciclo en ι")
            }
            ErrorEstructura::PredicadoDesconocido(p) => {
                write!(f, "el predicado {p:?} no está declarado en el manifiesto")
            }
            ErrorEstructura::ConstanteFueraDeDominio(predicado, constante) => write!(
                f,
                "la constante {constante:?} no está en el dominio del predicado {predicado:?}"
            ),
            ErrorEstructura::EtiquetaInvalida(e) => {
                write!(f, "la etiqueta {e:?} contiene un salto de línea")
            }
            ErrorEstructura::Path(e) => write!(f, "{e}"),
            ErrorEstructura::Manifiesto(errores) => {
                write!(f, "el manifiesto no es válido:")?;
                for e in errores {
                    write!(f, " {e};")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ErrorEstructura {}

impl Estructura {
    /// Construye una `Estructura` vacía a partir de un manifiesto, tras
    /// validarlo.
    pub fn nueva(manifiesto: Manifiesto) -> Result<Self, ErrorEstructura> {
        manifiesto.validar().map_err(ErrorEstructura::Manifiesto)?;
        Ok(Self {
            manifiesto,
            nodos: SlotMap::with_key(),
            aristas: SlotMap::with_key(),
            salientes: SecondaryMap::new(),
            entrantes: SecondaryMap::new(),
            padre: SecondaryMap::new(),
            hijos: SecondaryMap::new(),
            indice: HashMap::new(),
            indice_case: HashMap::new(),
            atributos: SecondaryMap::new(),
            anexos: SecondaryMap::new(),
        })
    }

    /// El manifiesto de instanciación.
    pub fn manifiesto(&self) -> &Manifiesto {
        &self.manifiesto
    }

    /// Cantidad de nodos vivos.
    pub fn len_nodos(&self) -> usize {
        self.nodos.len()
    }

    /// Cantidad de aristas vivas.
    pub fn len_aristas(&self) -> usize {
        self.aristas.len()
    }

    /// Inserta un nodo nuevo. Rechaza sorte ∉ $K_V$, path duplicado o
    /// colisión de case con otro nodo.
    pub fn insertar_nodo(
        &mut self,
        path: BundlePath,
        tipo: Sorte,
        etiqueta: Etiqueta,
    ) -> Result<NodoId, ErrorEstructura> {
        if !self.manifiesto.es_sorte_nodo(&tipo) {
            return Err(ErrorEstructura::SorteNodoDesconocido(
                tipo.como_str().to_string(),
            ));
        }
        if etiqueta.como_str().contains(['\n', '\r']) {
            return Err(ErrorEstructura::EtiquetaInvalida(
                etiqueta.como_str().to_string(),
            ));
        }
        if self.indice.contains_key(&path) {
            return Err(ErrorEstructura::PathDuplicado(path.como_str().to_string()));
        }
        let clave_case = path.clave_case();
        if let Some(&existente) = self.indice_case.get(&clave_case) {
            let path_existente = self.nodos[existente].path.como_str().to_string();
            return Err(ErrorEstructura::ColisionDeCase(
                path.como_str().to_string(),
                path_existente,
            ));
        }
        let id = self.nodos.insert(Nodo {
            path: path.clone(),
            tipo,
            etiqueta,
        });
        self.indice.insert(path, id);
        self.indice_case.insert(clave_case, id);
        self.salientes.insert(id, Vec::new());
        self.entrantes.insert(id, Vec::new());
        self.hijos.insert(id, Vec::new());
        Ok(id)
    }

    /// Inserta una arista nueva. Rechaza ids stale o clave ∉ $K_E$. El
    /// ordinal es la cantidad previa de aristas con el mismo
    /// `(origen, tipo, destino)` (multigrafo; loops permitidos).
    pub fn insertar_arista(
        &mut self,
        origen: NodoId,
        destino: NodoId,
        tipo: Sorte,
        etiqueta: Etiqueta,
    ) -> Result<AristaId, ErrorEstructura> {
        if !self.nodos.contains_key(origen) || !self.nodos.contains_key(destino) {
            return Err(ErrorEstructura::NodoInexistente);
        }
        if !self.manifiesto.es_sorte_arista(tipo.como_str()) {
            return Err(ErrorEstructura::SorteAristaDesconocido(
                tipo.como_str().to_string(),
            ));
        }
        if etiqueta.como_str().contains(['\n', '\r']) {
            return Err(ErrorEstructura::EtiquetaInvalida(
                etiqueta.como_str().to_string(),
            ));
        }
        let ordinal = self.salientes[origen]
            .iter()
            .filter(|id| {
                let a = &self.aristas[**id];
                a.tipo == tipo && a.destino == destino
            })
            .count() as u32;
        let id = self.aristas.insert(Arista {
            origen,
            destino,
            tipo,
            etiqueta,
            ordinal,
        });
        self.salientes[origen].push(id);
        self.entrantes[destino].push(id);
        Ok(id)
    }

    /// Asigna (o reasigna) el padre de `hijo` en $\iota$. Rechaza
    /// `hijo == padre`, ids stale y ciclos.
    pub fn asignar_padre(&mut self, hijo: NodoId, padre: NodoId) -> Result<(), ErrorEstructura> {
        if !self.nodos.contains_key(hijo) || !self.nodos.contains_key(padre) {
            return Err(ErrorEstructura::NodoInexistente);
        }
        if hijo == padre {
            return Err(ErrorEstructura::PadreDeSiMismo);
        }
        let mut actual = Some(padre);
        while let Some(n) = actual {
            if n == hijo {
                return Err(ErrorEstructura::CicloEnIota);
            }
            actual = self.padre.get(n).copied();
        }
        if let Some(anterior) = self.padre.get(hijo).copied() {
            if let Some(vec) = self.hijos.get_mut(anterior) {
                vec.retain(|&h| h != hijo);
            }
        }
        self.padre.insert(hijo, padre);
        self.hijos[padre].push(hijo);
        Ok(())
    }

    /// Quita el padre de `hijo`, si lo tenía.
    pub fn quitar_padre(&mut self, hijo: NodoId) -> Result<(), ErrorEstructura> {
        if !self.nodos.contains_key(hijo) {
            return Err(ErrorEstructura::NodoInexistente);
        }
        if let Some(anterior) = self.padre.remove(hijo) {
            if let Some(vec) = self.hijos.get_mut(anterior) {
                vec.retain(|&h| h != hijo);
            }
        }
        Ok(())
    }

    /// Asigna el valor de un predicado de extensión sobre un nodo.
    /// Rechaza predicado no declarado o constante fuera de su dominio.
    pub fn asignar_atributo(
        &mut self,
        nodo: NodoId,
        predicado: &str,
        constante: &str,
    ) -> Result<(), ErrorEstructura> {
        if !self.nodos.contains_key(nodo) {
            return Err(ErrorEstructura::NodoInexistente);
        }
        let decl = self
            .manifiesto
            .predicado(predicado)
            .ok_or_else(|| ErrorEstructura::PredicadoDesconocido(predicado.to_string()))?;
        if !decl.dominio.contains(constante) {
            return Err(ErrorEstructura::ConstanteFueraDeDominio(
                predicado.to_string(),
                constante.to_string(),
            ));
        }
        if !self.atributos.contains_key(nodo) {
            self.atributos.insert(nodo, BTreeMap::new());
        }
        self.atributos[nodo].insert(predicado.to_string(), constante.to_string());
        Ok(())
    }

    /// Los atributos asignados a un nodo (mapa vacío si no tiene o el id
    /// está stale).
    pub fn atributos(&self, nodo: NodoId) -> &BTreeMap<String, String> {
        static VACIO: BTreeMap<String, String> = BTreeMap::new();
        self.atributos.get(nodo).unwrap_or(&VACIO)
    }

    /// Adjunta (reemplazando) el `Anexo` de un nodo.
    pub fn anexar(&mut self, nodo: NodoId, anexo: Anexo) -> Result<(), ErrorEstructura> {
        if !self.nodos.contains_key(nodo) {
            return Err(ErrorEstructura::NodoInexistente);
        }
        self.anexos.insert(nodo, anexo);
        Ok(())
    }

    /// El `Anexo` de un nodo, si tiene.
    pub fn anexo(&self, nodo: NodoId) -> Option<&Anexo> {
        self.anexos.get(nodo)
    }

    /// Renombra un nodo: reescribe su path y los índices. El `NodoId`, sus
    /// aristas, su padre/hijos, sus atributos y su anexo quedan intactos.
    /// Mismas validaciones de path que `insertar_nodo`, salvo que una
    /// colisión de case consigo mismo (solo cambia el case del propio
    /// path) está permitida.
    pub fn renombrar(&mut self, nodo: NodoId, nuevo: BundlePath) -> Result<(), ErrorEstructura> {
        if !self.nodos.contains_key(nodo) {
            return Err(ErrorEstructura::NodoInexistente);
        }
        let actual = self.nodos[nodo].path.clone();
        if nuevo == actual {
            return Ok(());
        }
        if let Some(&existente) = self.indice.get(&nuevo) {
            if existente != nodo {
                return Err(ErrorEstructura::PathDuplicado(nuevo.como_str().to_string()));
            }
        }
        let clave_nueva = nuevo.clave_case();
        if let Some(&existente) = self.indice_case.get(&clave_nueva) {
            if existente != nodo {
                let path_existente = self.nodos[existente].path.como_str().to_string();
                return Err(ErrorEstructura::ColisionDeCase(
                    nuevo.como_str().to_string(),
                    path_existente,
                ));
            }
        }
        self.indice.remove(&actual);
        self.indice_case.remove(&actual.clave_case());
        self.nodos[nodo].path = nuevo.clone();
        self.indice.insert(nuevo, nodo);
        self.indice_case.insert(clave_nueva, nodo);
        Ok(())
    }

    /// Quita una arista. Renumera los ordinales del grupo
    /// `(origen, tipo, destino)` para que queden `0..n` sin huecos.
    pub fn quitar_arista(&mut self, id: AristaId) -> Result<Arista, ErrorEstructura> {
        if !self.aristas.contains_key(id) {
            return Err(ErrorEstructura::AristaInexistente);
        }
        Ok(self
            .quitar_arista_interna(id)
            .expect("la arista existe: se comprobó justo antes"))
    }

    fn quitar_arista_interna(&mut self, id: AristaId) -> Option<Arista> {
        let arista = self.aristas.remove(id)?;
        if let Some(vec) = self.salientes.get_mut(arista.origen) {
            vec.retain(|&a| a != id);
        }
        if let Some(vec) = self.entrantes.get_mut(arista.destino) {
            vec.retain(|&a| a != id);
        }
        self.renumerar_ordinales(arista.origen, &arista.tipo, arista.destino);
        Some(arista)
    }

    /// Renumera, en el orden de `salientes[origen]`, los ordinales de
    /// todas las aristas del grupo `(origen, tipo, destino)` para que
    /// queden `0..n` sin huecos.
    fn renumerar_ordinales(&mut self, origen: NodoId, tipo: &Sorte, destino: NodoId) {
        let Some(salientes) = self.salientes.get(origen) else {
            return;
        };
        let ids: Vec<AristaId> = salientes
            .iter()
            .copied()
            .filter(|id| {
                let a = &self.aristas[*id];
                &a.tipo == tipo && a.destino == destino
            })
            .collect();
        for (i, id) in ids.into_iter().enumerate() {
            self.aristas[id].ordinal = i as u32;
        }
    }

    /// Quita un nodo en cascada (Def. 3): quita sus aristas incidentes, su
    /// entrada de padre, deja a sus hijos como raíces, borra sus
    /// atributos y su anexo. Un `NodoId` stale nunca vuelve a resolver
    /// (garantía de generación de `slotmap`), aunque el slot se reocupe.
    pub fn quitar_nodo(&mut self, id: NodoId) -> Result<Nodo, ErrorEstructura> {
        let nodo = self
            .nodos
            .remove(id)
            .ok_or(ErrorEstructura::NodoInexistente)?;

        let mut incidentes: Vec<AristaId> = Vec::new();
        if let Some(vec) = self.salientes.get(id) {
            incidentes.extend(vec.iter().copied());
        }
        if let Some(vec) = self.entrantes.get(id) {
            for a in vec.iter().copied() {
                if !incidentes.contains(&a) {
                    incidentes.push(a);
                }
            }
        }
        for aid in incidentes {
            self.quitar_arista_interna(aid);
        }

        if let Some(padre_id) = self.padre.remove(id) {
            if let Some(vec) = self.hijos.get_mut(padre_id) {
                vec.retain(|&h| h != id);
            }
        }
        if let Some(hijos_de) = self.hijos.remove(id) {
            for h in hijos_de {
                self.padre.remove(h);
            }
        }

        self.atributos.remove(id);
        self.anexos.remove(id);
        self.salientes.remove(id);
        self.entrantes.remove(id);
        self.indice.remove(&nodo.path);
        self.indice_case.remove(&nodo.path.clave_case());

        Ok(nodo)
    }

    /// El nodo, si `id` está vivo.
    pub fn nodo(&self, id: NodoId) -> Option<&Nodo> {
        self.nodos.get(id)
    }

    /// La arista, si `id` está viva.
    pub fn arista(&self, id: AristaId) -> Option<&Arista> {
        self.aristas.get(id)
    }

    /// El id del nodo con ese path exacto, si existe.
    pub fn por_path(&self, path: &BundlePath) -> Option<NodoId> {
        self.indice.get(path).copied()
    }

    /// Todos los nodos vivos.
    pub fn nodos(&self) -> impl Iterator<Item = (NodoId, &Nodo)> {
        self.nodos.iter()
    }

    /// Todas las aristas vivas.
    pub fn aristas(&self) -> impl Iterator<Item = (AristaId, &Arista)> {
        self.aristas.iter()
    }

    /// Aristas salientes de un nodo, en orden de inserción.
    pub fn salientes(&self, id: NodoId) -> &[AristaId] {
        self.salientes.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Aristas entrantes de un nodo, en orden de inserción.
    pub fn entrantes(&self, id: NodoId) -> &[AristaId] {
        self.entrantes.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// El padre de un nodo en $\iota$, si tiene.
    pub fn padre(&self, id: NodoId) -> Option<NodoId> {
        self.padre.get(id).copied()
    }

    /// Los hijos de un nodo en $\iota$, en orden de asignación.
    pub fn hijos(&self, id: NodoId) -> &[NodoId] {
        self.hijos.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Enumera todos los átomos que describen esta `Estructura` (spec
    /// §3.5): un `Type` por nodo; un `Edge` por arista (azúcar de
    /// notación: NO se emite un `Type` aparte para la arista); un
    /// `Parent` por entrada de $\iota$; un `HasLab` por nodo **y** por
    /// arista ($\lambda$ es total: la etiqueta vacía se representa como
    /// `""`); un `Ext` por atributo asignado. El resultado queda en el
    /// orden total: lexicográfico por bytes de la forma canónica
    /// (`Atomo::Ord`, spec §3.5-3.6).
    pub fn atomos(&self) -> Vec<Atomo> {
        let mut atomos = Vec::new();
        for (_, nodo) in self.nodos.iter() {
            atomos.push(Atomo::Type {
                sujeto: nodo.path.clone(),
                sorte: nodo.tipo.clone(),
            });
            atomos.push(Atomo::HasLab {
                sujeto: Sujeto::Nodo(nodo.path.clone()),
                etiqueta: nodo.etiqueta.clone(),
            });
        }
        for (_, arista) in self.aristas.iter() {
            let origen = self.nodos[arista.origen].path.clone();
            let destino = self.nodos[arista.destino].path.clone();
            let id = IdArista {
                origen: origen.clone(),
                sorte: arista.tipo.clone(),
                destino: destino.clone(),
                ordinal: arista.ordinal,
            };
            atomos.push(Atomo::Edge {
                id: id.clone(),
                origen,
                destino,
                sorte: arista.tipo.clone(),
            });
            atomos.push(Atomo::HasLab {
                sujeto: Sujeto::Arista(id),
                etiqueta: arista.etiqueta.clone(),
            });
        }
        for (hijo, &padre) in self.padre.iter() {
            atomos.push(Atomo::Parent {
                hijo: self.nodos[hijo].path.clone(),
                padre: self.nodos[padre].path.clone(),
            });
        }
        for (nodo, mapa) in self.atributos.iter() {
            let sujeto = self.nodos[nodo].path.clone();
            for (predicado, constante) in mapa {
                atomos.push(Atomo::Ext {
                    predicado: predicado.clone(),
                    sujeto: sujeto.clone(),
                    constante: constante.clone(),
                });
            }
        }
        atomos.sort();
        atomos
    }

    /// Invariantes de representación, para tests: coherencia de todos los
    /// índices y adyacencias derivados contra la fuente de verdad.
    pub fn verificar_representacion(&self) -> Result<(), Vec<String>> {
        let mut errores = Vec::new();

        // 1. `indice` es biyectivo con `nodos` (mismo cardinal, ambos sentidos).
        if self.indice.len() != self.nodos.len() {
            errores.push(format!(
                "indice tiene {} entradas pero hay {} nodos",
                self.indice.len(),
                self.nodos.len()
            ));
        }
        for (id, nodo) in self.nodos.iter() {
            match self.indice.get(&nodo.path) {
                Some(&encontrado) if encontrado == id => {}
                Some(_) => errores.push(format!(
                    "indice[{}] no apunta al nodo propietario del path",
                    nodo.path
                )),
                None => errores.push(format!("el path {} no está en indice", nodo.path)),
            }
        }
        for (path, &id) in self.indice.iter() {
            match self.nodos.get(id) {
                Some(n) if &n.path == path => {}
                _ => errores.push(format!(
                    "la entrada de indice {path} no corresponde a un nodo vivo con ese path"
                )),
            }
        }

        // 2. `indice_case` tiene exactamente una entrada por nodo.
        if self.indice_case.len() != self.nodos.len() {
            errores.push(format!(
                "indice_case tiene {} entradas pero hay {} nodos",
                self.indice_case.len(),
                self.nodos.len()
            ));
        }
        for (id, nodo) in self.nodos.iter() {
            match self.indice_case.get(&nodo.path.clave_case()) {
                Some(&encontrado) if encontrado == id => {}
                _ => errores.push(format!(
                    "indice_case no resuelve correctamente el path {}",
                    nodo.path
                )),
            }
        }

        // 3. Toda arista aparece exactamente una vez en salientes[origen] y
        //    en entrantes[destino], y nada más aparece ahí.
        for (id, arista) in self.aristas.iter() {
            let en_salientes = self
                .salientes
                .get(arista.origen)
                .map(|v| v.iter().filter(|&&a| a == id).count())
                .unwrap_or(0);
            if en_salientes != 1 {
                errores.push(format!(
                    "la arista {id:?} aparece {en_salientes} veces en salientes de su origen"
                ));
            }
            let en_entrantes = self
                .entrantes
                .get(arista.destino)
                .map(|v| v.iter().filter(|&&a| a == id).count())
                .unwrap_or(0);
            if en_entrantes != 1 {
                errores.push(format!(
                    "la arista {id:?} aparece {en_entrantes} veces en entrantes de su destino"
                ));
            }
        }
        for (n, vec) in self.salientes.iter() {
            for &aid in vec {
                match self.aristas.get(aid) {
                    Some(a) if a.origen == n => {}
                    Some(_) => errores.push(format!(
                        "salientes[{n:?}] contiene una arista cuyo origen no es {n:?}"
                    )),
                    None => errores.push(format!(
                        "salientes[{n:?}] referencia la arista inexistente {aid:?}"
                    )),
                }
            }
        }
        for (n, vec) in self.entrantes.iter() {
            for &aid in vec {
                match self.aristas.get(aid) {
                    Some(a) if a.destino == n => {}
                    Some(_) => errores.push(format!(
                        "entrantes[{n:?}] contiene una arista cuyo destino no es {n:?}"
                    )),
                    None => errores.push(format!(
                        "entrantes[{n:?}] referencia la arista inexistente {aid:?}"
                    )),
                }
            }
        }

        // 4. `padre`/`hijos` mutuamente coherentes.
        for (hijo, &padre_id) in self.padre.iter() {
            if !self.nodos.contains_key(hijo) || !self.nodos.contains_key(padre_id) {
                errores.push(format!("padre[{hijo:?}] referencia un nodo inexistente"));
                continue;
            }
            let veces = self
                .hijos
                .get(padre_id)
                .map(|v| v.iter().filter(|&&h| h == hijo).count())
                .unwrap_or(0);
            if veces != 1 {
                errores.push(format!(
                    "el hijo {hijo:?} aparece {veces} veces en hijos de su padre"
                ));
            }
        }
        for (padre_id, vec) in self.hijos.iter() {
            for &hijo in vec {
                match self.padre.get(hijo) {
                    Some(&p) if p == padre_id => {}
                    _ => errores.push(format!(
                        "hijos[{padre_id:?}] contiene a {hijo:?} sin correspondencia en padre"
                    )),
                }
            }
        }

        // 5. $\iota$ es un bosque: sin ciclos.
        for (id, _) in self.nodos.iter() {
            let mut visto = HashSet::new();
            let mut actual = Some(id);
            while let Some(n) = actual {
                if !visto.insert(n) {
                    errores.push(format!("iota contiene un ciclo que pasa por {n:?}"));
                    break;
                }
                actual = self.padre.get(n).copied();
            }
        }

        // 6. Ordinales por `(origen, tipo, destino)` son `0..n` sin huecos.
        let mut grupos: HashMap<(NodoId, Sorte, NodoId), Vec<u32>> = HashMap::new();
        for (_, a) in self.aristas.iter() {
            grupos
                .entry((a.origen, a.tipo.clone(), a.destino))
                .or_default()
                .push(a.ordinal);
        }
        for ((origen, tipo, destino), mut ordinales) in grupos {
            ordinales.sort_unstable();
            let esperado: Vec<u32> = (0..ordinales.len() as u32).collect();
            if ordinales != esperado {
                errores.push(format!(
                    "el grupo ({origen:?}, {tipo}, {destino:?}) tiene ordinales con huecos o duplicados: {ordinales:?}"
                ));
            }
        }

        // 7. Atributos solo para nodos vivos, con predicado declarado y
        //    constante en dominio.
        for (nodo, mapa) in self.atributos.iter() {
            if !self.nodos.contains_key(nodo) {
                errores.push(format!(
                    "atributos[{nodo:?}] referencia un nodo inexistente"
                ));
                continue;
            }
            for (predicado, constante) in mapa {
                match self.manifiesto.predicado(predicado) {
                    Some(decl) if decl.dominio.contains(constante) => {}
                    Some(_) => errores.push(format!(
                        "el atributo {predicado:?} tiene la constante {constante:?} fuera de dominio"
                    )),
                    None => errores.push(format!(
                        "el atributo {predicado:?} no está declarado en el manifiesto"
                    )),
                }
            }
        }

        // 8. Anexos solo para nodos vivos.
        for (nodo, _) in self.anexos.iter() {
            if !self.nodos.contains_key(nodo) {
                errores.push(format!("anexos[{nodo:?}] referencia un nodo inexistente"));
            }
        }

        if errores.is_empty() {
            Ok(())
        } else {
            Err(errores)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BundlePath, Etiqueta, Sorte};

    fn e() -> Estructura {
        Estructura::nueva(crate::manifiesto::tests::base()).unwrap()
    }
    fn p(s: &str) -> BundlePath {
        BundlePath::nuevo(s).unwrap()
    }
    fn s(x: &str) -> Sorte {
        Sorte::nuevo(x)
    }
    fn l(x: &str) -> Etiqueta {
        Etiqueta::nuevo(x)
    }

    #[test]
    fn nueva_rechaza_manifiesto_invalido() {
        let mut m = crate::manifiesto::tests::base();
        m.version.clear();
        assert!(matches!(
            Estructura::nueva(m),
            Err(ErrorEstructura::Manifiesto(_))
        ));
    }
    #[test]
    fn insertar_nodo_indexa_y_valida_sorte() {
        let mut u = e();
        let v = u.insertar_nodo(p("n/x"), s("a"), l("X")).unwrap();
        assert_eq!(u.por_path(&p("n/x")), Some(v));
        assert_eq!(u.nodo(v).unwrap().etiqueta.como_str(), "X");
        assert_eq!(u.nodo(v).unwrap().tipo, s("a"));
        assert_eq!(
            u.insertar_nodo(p("n/y"), s("zzz"), l("Y")).unwrap_err(),
            ErrorEstructura::SorteNodoDesconocido("zzz".into())
        );
        assert_eq!(
            u.insertar_nodo(p("n/x"), s("a"), l("X")).unwrap_err(),
            ErrorEstructura::PathDuplicado("n/x".into())
        );
        assert_eq!(
            u.insertar_nodo(p("N/X"), s("a"), l("X")).unwrap_err(),
            ErrorEstructura::ColisionDeCase("N/X".into(), "n/x".into())
        );
        assert_eq!(u.len_nodos(), 1);
        u.verificar_representacion().unwrap();
    }
    #[test]
    fn rechaza_etiqueta_con_salto_de_linea() {
        let mut u = e();
        assert_eq!(
            u.insertar_nodo(p("n/x"), s("a"), l("X\nY")).unwrap_err(),
            ErrorEstructura::EtiquetaInvalida("X\nY".into())
        );
        assert_eq!(
            u.insertar_nodo(p("n/y"), s("a"), l("X\rY")).unwrap_err(),
            ErrorEstructura::EtiquetaInvalida("X\rY".into())
        );
        let x = u.insertar_nodo(p("n/x"), s("a"), l("X")).unwrap();
        let y = u.insertar_nodo(p("n/y"), s("a"), l("Y")).unwrap();
        assert_eq!(
            u.insertar_arista(x, y, s("rel"), l("a\nb")).unwrap_err(),
            ErrorEstructura::EtiquetaInvalida("a\nb".into())
        );
        u.verificar_representacion().unwrap();
    }
    #[test]
    fn quitar_nodo_borra_el_anexo() {
        let mut u = e();
        let x = u.insertar_nodo(p("n/x"), s("a"), l("X")).unwrap();
        u.anexar(
            x,
            Anexo {
                cuerpo: "cuerpo".into(),
                passthrough: vec![],
            },
        )
        .unwrap();
        u.quitar_nodo(x).unwrap();
        u.verificar_representacion().unwrap();
        assert!(u.anexo(x).is_none());
        let x2 = u.insertar_nodo(p("n/x"), s("a"), l("X2")).unwrap();
        assert!(u.anexo(x2).is_none());
    }
    #[test]
    fn aristas_multigrafo_con_ordinal() {
        let mut u = e();
        let x = u.insertar_nodo(p("n/x"), s("a"), l("X")).unwrap();
        let y = u.insertar_nodo(p("n/y"), s("a"), l("Y")).unwrap();
        let e0 = u.insertar_arista(x, y, s("rel"), l("")).unwrap();
        let e1 = u.insertar_arista(x, y, s("rel"), l("")).unwrap();
        let e2 = u.insertar_arista(x, y, s("sub"), l("")).unwrap();
        assert_ne!(e0, e1);
        assert_eq!(u.arista(e1).unwrap().ordinal, 1);
        assert_eq!(u.arista(e2).unwrap().ordinal, 0);
        assert_eq!(u.salientes(x), &[e0, e1, e2]);
        assert_eq!(u.entrantes(y), &[e0, e1, e2]);
        assert_eq!(u.salientes(y), &[] as &[AristaId]);
        assert_eq!(
            u.insertar_arista(x, y, s("nope"), l("")).unwrap_err(),
            ErrorEstructura::SorteAristaDesconocido("nope".into())
        );
        let bucle = u.insertar_arista(x, x, s("rel"), l("")).unwrap();
        assert_eq!(u.arista(bucle).unwrap().ordinal, 0);
        assert_eq!(u.salientes(x), &[e0, e1, e2, bucle]);
        assert_eq!(u.entrantes(x), &[bucle]);
        assert_eq!(u.len_aristas(), 4);
        u.verificar_representacion().unwrap();
    }
    #[test]
    fn iota_es_bosque() {
        let mut u = e();
        let a = u.insertar_nodo(p("n/a"), s("a"), l("A")).unwrap();
        let b = u.insertar_nodo(p("n/b"), s("a"), l("B")).unwrap();
        let c = u.insertar_nodo(p("n/c"), s("a"), l("C")).unwrap();
        u.asignar_padre(b, a).unwrap();
        u.asignar_padre(c, b).unwrap();
        assert_eq!(
            u.asignar_padre(a, c).unwrap_err(),
            ErrorEstructura::CicloEnIota
        );
        assert_eq!(
            u.asignar_padre(a, a).unwrap_err(),
            ErrorEstructura::PadreDeSiMismo
        );
        assert_eq!(u.padre(c), Some(b));
        assert_eq!(u.hijos(a), &[b]);
        u.asignar_padre(c, a).unwrap(); // reasignar reemplaza al padre anterior
        assert_eq!(u.hijos(a), &[b, c]);
        assert_eq!(u.hijos(b), &[] as &[NodoId]);
        u.quitar_padre(c).unwrap();
        assert_eq!(u.padre(c), None);
        assert_eq!(u.hijos(a), &[b]);
        u.verificar_representacion().unwrap();
    }
    #[test]
    fn atributos_con_dominio_cerrado() {
        let mut u = e();
        let x = u.insertar_nodo(p("n/x"), s("a"), l("X")).unwrap();
        u.asignar_atributo(x, "Nivel", "alto").unwrap();
        assert_eq!(
            u.asignar_atributo(x, "Nivel", "medio").unwrap_err(),
            ErrorEstructura::ConstanteFueraDeDominio("Nivel".into(), "medio".into())
        );
        assert_eq!(
            u.asignar_atributo(x, "Otro", "alto").unwrap_err(),
            ErrorEstructura::PredicadoDesconocido("Otro".into())
        );
        assert_eq!(
            u.atributos(x).get("Nivel").map(String::as_str),
            Some("alto")
        );
        u.asignar_atributo(x, "Nivel", "bajo").unwrap();
        assert_eq!(
            u.atributos(x).get("Nivel").map(String::as_str),
            Some("bajo")
        );
        u.verificar_representacion().unwrap();
    }
    #[test]
    fn renombrar_conserva_id_aristas_padre_y_anexo() {
        let mut u = e();
        let x = u.insertar_nodo(p("n/x"), s("a"), l("X")).unwrap();
        let y = u.insertar_nodo(p("n/y"), s("a"), l("Y")).unwrap();
        let ex = u.insertar_arista(y, x, s("rel"), l("")).unwrap();
        u.asignar_padre(x, y).unwrap();
        u.anexar(
            x,
            Anexo {
                cuerpo: "cuerpo".into(),
                passthrough: vec![],
            },
        )
        .unwrap();
        u.renombrar(x, p("n/z")).unwrap();
        assert_eq!(u.por_path(&p("n/x")), None);
        assert_eq!(u.por_path(&p("n/z")), Some(x));
        assert_eq!(u.nodo(x).unwrap().path, p("n/z"));
        assert_eq!(u.arista(ex).unwrap().destino, x);
        assert_eq!(u.padre(x), Some(y));
        assert_eq!(u.anexo(x).unwrap().cuerpo, "cuerpo");
        assert_eq!(
            u.renombrar(x, p("n/y")).unwrap_err(),
            ErrorEstructura::PathDuplicado("n/y".into())
        );
        assert_eq!(
            u.renombrar(x, p("n/Y")).unwrap_err(),
            ErrorEstructura::ColisionDeCase("n/Y".into(), "n/y".into())
        );
        u.renombrar(x, p("n/Z")).unwrap(); // solo cambia el case del propio nodo: permitido
        assert_eq!(u.por_path(&p("n/Z")), Some(x));
        u.verificar_representacion().unwrap();
    }
    #[test]
    fn quitar_nodo_cascada_y_id_stale() {
        let mut u = e();
        let x = u.insertar_nodo(p("n/x"), s("a"), l("X")).unwrap();
        let y = u.insertar_nodo(p("n/y"), s("a"), l("Y")).unwrap();
        let h = u.insertar_nodo(p("n/h"), s("a"), l("H")).unwrap();
        let e1 = u.insertar_arista(x, y, s("rel"), l("")).unwrap();
        let e2 = u.insertar_arista(y, x, s("rel"), l("")).unwrap();
        u.asignar_padre(h, x).unwrap();
        u.asignar_padre(x, y).unwrap();
        u.asignar_atributo(x, "Nivel", "alto").unwrap();
        let quitado = u.quitar_nodo(x).unwrap();
        assert_eq!(quitado.path, p("n/x"));
        assert!(u.nodo(x).is_none());
        assert!(u.arista(e1).is_none());
        assert!(u.arista(e2).is_none());
        assert_eq!(u.entrantes(y), &[] as &[AristaId]);
        assert_eq!(u.salientes(y), &[] as &[AristaId]);
        assert_eq!(u.padre(h), None);
        assert_eq!(u.hijos(y), &[] as &[NodoId]);
        assert_eq!(u.por_path(&p("n/x")), None);
        assert_eq!(u.len_nodos(), 2);
        assert_eq!(u.len_aristas(), 0);
        let x2 = u.insertar_nodo(p("n/x"), s("a"), l("X2")).unwrap();
        assert_ne!(x, x2);
        assert!(
            u.nodo(x).is_none(),
            "un id stale jamás resuelve al nodo que reocupa el slot"
        );
        assert!(u.atributos(x).is_empty());
        assert_eq!(
            u.insertar_arista(x, y, s("rel"), l("")).unwrap_err(),
            ErrorEstructura::NodoInexistente
        );
        assert_eq!(
            u.quitar_nodo(x).unwrap_err(),
            ErrorEstructura::NodoInexistente
        );
        assert_eq!(
            u.asignar_padre(h, x).unwrap_err(),
            ErrorEstructura::NodoInexistente
        );
        u.verificar_representacion().unwrap();
    }
    #[test]
    fn quitar_arista_reordena_ordinales() {
        let mut u = e();
        let x = u.insertar_nodo(p("n/x"), s("a"), l("X")).unwrap();
        let y = u.insertar_nodo(p("n/y"), s("a"), l("Y")).unwrap();
        let e0 = u.insertar_arista(x, y, s("rel"), l("")).unwrap();
        let e1 = u.insertar_arista(x, y, s("rel"), l("")).unwrap();
        let e2 = u.insertar_arista(x, y, s("rel"), l("")).unwrap();
        let quitada = u.quitar_arista(e1).unwrap();
        assert_eq!(quitada.ordinal, 1);
        assert!(u.arista(e1).is_none());
        assert_eq!(u.arista(e0).unwrap().ordinal, 0);
        assert_eq!(
            u.arista(e2).unwrap().ordinal,
            1,
            "los ordinales por (origen, tipo, destino) quedan 0..n sin huecos"
        );
        assert_eq!(u.salientes(x), &[e0, e2]);
        assert_eq!(
            u.quitar_arista(e1).unwrap_err(),
            ErrorEstructura::AristaInexistente
        );
        u.verificar_representacion().unwrap();
    }
    #[test]
    fn atomos_de_estructura_enumera_todo_en_orden_total() {
        let mut u = e();
        let x = u.insertar_nodo(p("n/x"), s("a"), l("X")).unwrap();
        let y = u.insertar_nodo(p("n/y"), s("a"), l("Y")).unwrap();
        u.insertar_arista(x, y, s("rel"), l("")).unwrap();
        u.insertar_arista(x, y, s("rel"), l("")).unwrap();
        u.asignar_padre(y, x).unwrap();
        u.asignar_atributo(x, "Nivel", "alto").unwrap();
        let texto: Vec<String> = u.atomos().iter().map(ToString::to_string).collect();
        let esperado = vec![
            "Edge(n/x→n/y#rel#0, n/x, n/y, rel)",
            "Edge(n/x→n/y#rel#1, n/x, n/y, rel)",
            "HasLab(n/x, \"X\")",
            "HasLab(n/x→n/y#rel#0, \"\")",
            "HasLab(n/x→n/y#rel#1, \"\")",
            "HasLab(n/y, \"Y\")",
            "Nivel(n/x, alto)",
            "Parent(n/y, n/x)",
            "Type(n/x, a)",
            "Type(n/y, a)",
        ];
        assert_eq!(texto, esperado);
        let mut ordenado = texto.clone();
        ordenado.sort();
        assert_eq!(texto, ordenado, "orden total = lexicográfico por bytes");
        for a in u.atomos() {
            assert_eq!(Atomo::parsear(&a.to_string()).unwrap(), a);
        }
    }
}
