//! `Manifiesto`: parámetros de instanciación (alfabetos $K_V$, $K_E$,
//! predicados de extensión y layout de disco) y su validación (spec §3.3,
//! entregable 6).

use std::collections::{BTreeMap, BTreeSet};

use crate::tipos::Sorte;

/// Sortes de nodo reservados por el motor (plan 07: `Retirado`, lápida con
/// identidad; plan 05: `Patrón`, nodo de patrón). Todo manifiesto debe
/// declararlos exactamente una vez.
pub const SORTES_RESERVADOS: [&str; 2] = ["Retirado", "Patrón"];

/// Nombres de predicado reservados para los átomos base del motor; un
/// `DeclPredicado` de extensión no puede reutilizarlos.
pub const PREDICADOS_BASE: [&str; 4] = ["Type", "Edge", "Parent", "HasLab"];

/// Claves de frontmatter que el motor interpreta de forma especial y que
/// ninguna clave de arista o de predicado puede reutilizar.
const CLAVES_FRONTMATTER_RESERVADAS: [&str; 2] = ["type", "title"];

/// Parámetros de instanciación del motor sobre un bundle concreto: los
/// alfabetos $K_V$ (sortes de nodo) y $K_E$ (declaraciones de arista), los
/// predicados de extensión y el layout de disco.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifiesto {
    /// Versión del manifiesto; obligatoria, no vacía.
    pub version: String,
    /// $K_V$: alfabeto de sortes de nodo, incluye los reservados.
    pub sortes_nodo: Vec<Sorte>,
    /// Sorte de nodo → valor de `type:` en disco, cuando difiere del
    /// nombre de la sorte.
    pub sortes_nodo_disco: BTreeMap<Sorte, String>,
    /// `(clave, valor)` de frontmatter que marca un nodo como `Retirado`.
    pub retirado_en_disco: Option<(String, String)>,
    /// $K_E$: declaraciones de arista, en el orden canónico de escritura.
    pub aristas: Vec<DeclArista>,
    /// Predicados de extensión declarados por la instanciación.
    pub predicados: Vec<DeclPredicado>,
    /// Layout de disco del bundle.
    pub layout: Layout,
}

/// Declaración de un tipo de arista. `clave` ES el tipo de arista (D5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclArista {
    /// Clave de la arista; también su tipo ($K_E$).
    pub clave: String,
    /// Si es simétrica: no puede tener `inversa`.
    pub simetrica: bool,
    /// Clave de la arista inversa recíproca, si la hay.
    pub inversa: Option<String>,
    /// Clave del campo de destino, para la forma de lista de mappings.
    pub destino: Option<String>,
    /// Clave del campo de etiqueta, para la forma de lista de mappings.
    pub etiqueta: Option<String>,
}

/// Declaración de un predicado de extensión: atributo unario de nodo con
/// dominio cerrado.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclPredicado {
    /// Nombre del predicado tal como aparece en los átomos `Ext`.
    pub nombre: String,
    /// Clave de frontmatter que lo declara en disco.
    pub clave: String,
    /// Dominio cerrado de constantes admitidas.
    pub dominio: BTreeSet<String>,
}

/// Layout de disco del bundle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    /// Directorios del bundle que el motor recorre.
    pub directorios: Vec<String>,
    /// Subconjunto de `directorios` (o subrutas suyas) sin nodo $\iota$
    /// implícito.
    pub sin_iota: Vec<String>,
    /// Rutas ignoradas por el motor.
    pub ignorados: Vec<String>,
}

/// Motivo de rechazo de un `Manifiesto` por `validar`.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ErrorManifiesto {
    /// `version` está vacía.
    VersionVacia,
    /// Falta un sorte reservado (`Retirado`, `Patrón`) en `sortes_nodo`.
    ReservadoAusente(String),
    /// Un sorte reservado aparece más de una vez en `sortes_nodo`.
    ReservadoDuplicado(String),
    /// Una sorte de nodo aparece más de una vez en `sortes_nodo`.
    SorteDuplicado(String),
    /// Una clave de arista aparece más de una vez en `aristas`.
    AristaDuplicada(String),
    /// Un nombre aparece a la vez en $K_V$ y en $K_E$.
    SorteEnAmbosAlfabetos(String),
    /// Una arista simétrica declara `inversa`.
    SimetricaConInversa(String),
    /// `inversa` nombra una clave de arista no declarada.
    InversaDesconocida(String),
    /// `inversa` nombra la propia clave de la arista.
    InversaASiMisma(String),
    /// La arista destino de `inversa` no apunta de vuelta a la original.
    InversaNoReciproca(String),
    /// `etiqueta` declarada sin `destino`.
    EtiquetaSinDestino(String),
    /// `destino` declarada sin `etiqueta`: el cargador lee las listas de
    /// mappings como pares (destino, etiqueta); una forma de mapping sin
    /// campo de etiqueta queda indefinida en v1.
    DestinoSinEtiqueta(String),
    /// Un predicado usa un nombre de `PREDICADOS_BASE`.
    PredicadoReservado(String),
    /// Un nombre de predicado aparece más de una vez.
    PredicadoDuplicado(String),
    /// El dominio de un predicado está vacío.
    DominioVacio(String),
    /// Una clave de arista o de predicado coincide con `type`, `title` o
    /// la clave de `retirado_en_disco`.
    ClaveReservada(String),
    /// Dos declaraciones (arista o predicado) comparten la misma clave.
    ClaveDuplicada(String),
    /// Una entrada de `sin_iota` no está contenida en `directorios`.
    SinIotaFueraDeLayout(String),
    /// `directorios` está vacío.
    DirectoriosVacio,
    /// Un valor de `sortes_nodo_disco` coincide con el nombre de otro
    /// sorte de `sortes_nodo` distinto de su propia clave.
    AliasDeDiscoColisiona(String),
    /// Una clave de `sortes_nodo_disco` no está declarada en `sortes_nodo`.
    AliasDeSorteDesconocida(String),
    /// Un nombre de sorte, clave de arista, nombre o clave de predicado,
    /// constante de dominio, o alias de disco no es válido: está vacío o
    /// contiene un blanco, un carácter de control, o uno de
    /// `, ( ) " \ # → : [ ]`.
    NombreInvalido(String),
}

impl std::fmt::Display for ErrorManifiesto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorManifiesto::VersionVacia => write!(f, "la versión del manifiesto está vacía"),
            ErrorManifiesto::ReservadoAusente(s) => {
                write!(f, "falta el sorte reservado {s:?} en sortes_nodo")
            }
            ErrorManifiesto::ReservadoDuplicado(s) => {
                write!(f, "el sorte reservado {s:?} está duplicado en sortes_nodo")
            }
            ErrorManifiesto::SorteDuplicado(s) => {
                write!(f, "el sorte de nodo {s:?} está duplicado")
            }
            ErrorManifiesto::AristaDuplicada(c) => {
                write!(f, "la clave de arista {c:?} está duplicada")
            }
            ErrorManifiesto::SorteEnAmbosAlfabetos(n) => {
                write!(f, "{n:?} es a la vez sorte de nodo y clave de arista")
            }
            ErrorManifiesto::SimetricaConInversa(c) => {
                write!(f, "la arista simétrica {c:?} declara una inversa")
            }
            ErrorManifiesto::InversaDesconocida(c) => {
                write!(f, "la inversa {c:?} no está declarada")
            }
            ErrorManifiesto::InversaASiMisma(c) => {
                write!(f, "la arista {c:?} declara su propia clave como inversa")
            }
            ErrorManifiesto::InversaNoReciproca(c) => {
                write!(f, "la inversa de {c:?} no es recíproca")
            }
            ErrorManifiesto::EtiquetaSinDestino(c) => {
                write!(f, "la arista {c:?} declara etiqueta sin destino")
            }
            ErrorManifiesto::DestinoSinEtiqueta(c) => {
                write!(f, "la arista {c:?} declara destino sin etiqueta")
            }
            ErrorManifiesto::PredicadoReservado(n) => {
                write!(f, "{n:?} es un nombre de predicado base reservado")
            }
            ErrorManifiesto::PredicadoDuplicado(n) => {
                write!(f, "el predicado {n:?} está duplicado")
            }
            ErrorManifiesto::DominioVacio(n) => {
                write!(f, "el predicado {n:?} tiene el dominio vacío")
            }
            ErrorManifiesto::ClaveReservada(c) => {
                write!(f, "la clave {c:?} está reservada por el frontmatter")
            }
            ErrorManifiesto::ClaveDuplicada(c) => {
                write!(f, "la clave {c:?} está duplicada entre declaraciones")
            }
            ErrorManifiesto::SinIotaFueraDeLayout(p) => {
                write!(f, "{p:?} en sin_iota no está contenido en directorios")
            }
            ErrorManifiesto::DirectoriosVacio => write!(f, "layout.directorios está vacío"),
            ErrorManifiesto::AliasDeDiscoColisiona(v) => write!(
                f,
                "el alias de disco {v:?} coincide con el nombre de otro sorte declarado"
            ),
            ErrorManifiesto::AliasDeSorteDesconocida(s) => write!(
                f,
                "sortes_nodo_disco declara un alias para la sorte desconocida {s:?}"
            ),
            ErrorManifiesto::NombreInvalido(n) => write!(
                f,
                "{n:?} no es un nombre válido: no puede estar vacío ni contener un blanco, \
                 un carácter de control, o uno de `, ( ) \" \\ # → : [ ]`"
            ),
        }
    }
}

impl std::error::Error for ErrorManifiesto {}

/// `true` si `nombre` está vacío o contiene un blanco, un carácter de
/// control, o uno de `, ( ) " \ # → : [ ]`: cualquiera de esos caracteres
/// rompería el texto canónico de un átomo (`Nombre(sujeto, constante)`) o
/// el id de una arista (`origen→destino#sorte#ordinal`).
fn nombre_invalido(nombre: &str) -> bool {
    nombre.is_empty()
        || nombre.chars().any(|c| {
            c.is_whitespace()
                || c.is_control()
                || matches!(
                    c,
                    ',' | '(' | ')' | '"' | '\\' | '#' | '→' | ':' | '[' | ']'
                )
        })
}

impl Manifiesto {
    /// Valida el manifiesto, acumulando todos los errores encontrados en
    /// vez de detenerse en el primero.
    pub fn validar(&self) -> Result<(), Vec<ErrorManifiesto>> {
        let mut errores = Vec::new();

        if self.version.is_empty() {
            errores.push(ErrorManifiesto::VersionVacia);
        }

        for reservado in SORTES_RESERVADOS {
            let apariciones = self
                .sortes_nodo
                .iter()
                .filter(|s| s.como_str() == reservado)
                .count();
            if apariciones == 0 {
                errores.push(ErrorManifiesto::ReservadoAusente(reservado.to_string()));
            } else if apariciones > 1 {
                errores.push(ErrorManifiesto::ReservadoDuplicado(reservado.to_string()));
            }
        }

        let mut vistos_nodo: BTreeSet<&str> = BTreeSet::new();
        let mut reportados_nodo: BTreeSet<&str> = BTreeSet::new();
        for s in &self.sortes_nodo {
            if !vistos_nodo.insert(s.como_str()) && reportados_nodo.insert(s.como_str()) {
                errores.push(ErrorManifiesto::SorteDuplicado(s.como_str().to_string()));
            }
        }

        let mut vistas_arista: BTreeSet<&str> = BTreeSet::new();
        let mut reportadas_arista: BTreeSet<&str> = BTreeSet::new();
        for a in &self.aristas {
            if !vistas_arista.insert(&a.clave) && reportadas_arista.insert(&a.clave) {
                errores.push(ErrorManifiesto::AristaDuplicada(a.clave.clone()));
            }
        }

        let claves_arista: BTreeSet<&str> = self.aristas.iter().map(|a| a.clave.as_str()).collect();
        let mut reportados_ambos: BTreeSet<&str> = BTreeSet::new();
        for s in &self.sortes_nodo {
            if claves_arista.contains(s.como_str()) && reportados_ambos.insert(s.como_str()) {
                errores.push(ErrorManifiesto::SorteEnAmbosAlfabetos(
                    s.como_str().to_string(),
                ));
            }
        }

        for a in &self.aristas {
            if a.simetrica && a.inversa.is_some() {
                errores.push(ErrorManifiesto::SimetricaConInversa(a.clave.clone()));
            }
            if let Some(inversa) = &a.inversa {
                if inversa == &a.clave {
                    errores.push(ErrorManifiesto::InversaASiMisma(a.clave.clone()));
                } else if let Some(destino) = self.aristas.iter().find(|b| &b.clave == inversa) {
                    if destino.inversa.as_deref() != Some(a.clave.as_str()) {
                        errores.push(ErrorManifiesto::InversaNoReciproca(a.clave.clone()));
                    }
                } else {
                    errores.push(ErrorManifiesto::InversaDesconocida(inversa.clone()));
                }
            }
            if a.etiqueta.is_some() && a.destino.is_none() {
                errores.push(ErrorManifiesto::EtiquetaSinDestino(a.clave.clone()));
            }
            if a.destino.is_some() && a.etiqueta.is_none() {
                errores.push(ErrorManifiesto::DestinoSinEtiqueta(a.clave.clone()));
            }
        }

        let mut vistos_predicado: BTreeSet<&str> = BTreeSet::new();
        let mut reportados_predicado: BTreeSet<&str> = BTreeSet::new();
        for p in &self.predicados {
            if PREDICADOS_BASE.contains(&p.nombre.as_str()) {
                errores.push(ErrorManifiesto::PredicadoReservado(p.nombre.clone()));
            }
            if !vistos_predicado.insert(&p.nombre) && reportados_predicado.insert(&p.nombre) {
                errores.push(ErrorManifiesto::PredicadoDuplicado(p.nombre.clone()));
            }
            if p.dominio.is_empty() {
                errores.push(ErrorManifiesto::DominioVacio(p.nombre.clone()));
            }
        }

        let clave_retirado = self
            .retirado_en_disco
            .as_ref()
            .map(|(clave, _)| clave.as_str());
        let mut reportadas_reservadas: BTreeSet<&str> = BTreeSet::new();
        let claves_declaradas = self
            .aristas
            .iter()
            .map(|a| a.clave.as_str())
            .chain(self.predicados.iter().map(|p| p.clave.as_str()));
        for clave in claves_declaradas {
            let es_reservada =
                CLAVES_FRONTMATTER_RESERVADAS.contains(&clave) || clave_retirado == Some(clave);
            if es_reservada && reportadas_reservadas.insert(clave) {
                errores.push(ErrorManifiesto::ClaveReservada(clave.to_string()));
            }
        }

        let mut vistas_clave: BTreeSet<&str> = BTreeSet::new();
        let mut reportadas_clave: BTreeSet<&str> = BTreeSet::new();
        let todas_las_claves = self
            .aristas
            .iter()
            .map(|a| a.clave.as_str())
            .chain(self.predicados.iter().map(|p| p.clave.as_str()));
        for clave in todas_las_claves {
            if !vistas_clave.insert(clave) && reportadas_clave.insert(clave) {
                errores.push(ErrorManifiesto::ClaveDuplicada(clave.to_string()));
            }
        }

        for entrada in &self.layout.sin_iota {
            let contenido = self.layout.directorios.iter().any(|dir| {
                entrada == dir
                    || entrada
                        .strip_prefix(dir)
                        .is_some_and(|resto| resto.starts_with('/'))
            });
            if !contenido {
                errores.push(ErrorManifiesto::SinIotaFueraDeLayout(entrada.clone()));
            }
        }

        if self.layout.directorios.is_empty() {
            errores.push(ErrorManifiesto::DirectoriosVacio);
        }

        let mut reportados_alias_colision: BTreeSet<&str> = BTreeSet::new();
        for (sorte, valor) in &self.sortes_nodo_disco {
            if !self.sortes_nodo.contains(sorte) {
                errores.push(ErrorManifiesto::AliasDeSorteDesconocida(
                    sorte.como_str().to_string(),
                ));
            }
            let colisiona = self
                .sortes_nodo
                .iter()
                .any(|s| s != sorte && s.como_str() == valor.as_str());
            if colisiona && reportados_alias_colision.insert(valor.as_str()) {
                errores.push(ErrorManifiesto::AliasDeDiscoColisiona(valor.clone()));
            }
        }

        let mut nombres_a_validar: Vec<&str> = Vec::new();
        for s in &self.sortes_nodo {
            nombres_a_validar.push(s.como_str());
        }
        for a in &self.aristas {
            nombres_a_validar.push(a.clave.as_str());
        }
        for p in &self.predicados {
            nombres_a_validar.push(p.nombre.as_str());
            nombres_a_validar.push(p.clave.as_str());
            for c in &p.dominio {
                nombres_a_validar.push(c.as_str());
            }
        }
        for v in self.sortes_nodo_disco.values() {
            nombres_a_validar.push(v.as_str());
        }
        let mut reportados_nombre: BTreeSet<&str> = BTreeSet::new();
        for nombre in nombres_a_validar {
            if nombre_invalido(nombre) && reportados_nombre.insert(nombre) {
                errores.push(ErrorManifiesto::NombreInvalido(nombre.to_string()));
            }
        }

        if errores.is_empty() {
            Ok(())
        } else {
            Err(errores)
        }
    }

    /// `true` si `sorte` está declarada en `sortes_nodo` ($K_V$).
    pub fn es_sorte_nodo(&self, sorte: &Sorte) -> bool {
        self.sortes_nodo.contains(sorte)
    }

    /// `true` si `clave` está declarada como clave de arista ($K_E$).
    pub fn es_sorte_arista(&self, clave: &str) -> bool {
        self.aristas.iter().any(|a| a.clave == clave)
    }

    /// Declaración de la arista de clave `clave`, si existe.
    pub fn arista(&self, clave: &str) -> Option<&DeclArista> {
        self.aristas.iter().find(|a| a.clave == clave)
    }

    /// Declaración del predicado de nombre `nombre`, si existe.
    pub fn predicado(&self, nombre: &str) -> Option<&DeclPredicado> {
        self.predicados.iter().find(|p| p.nombre == nombre)
    }

    /// Declaración del predicado cuya clave de frontmatter es `clave`, si
    /// existe.
    pub fn predicado_por_clave(&self, clave: &str) -> Option<&DeclPredicado> {
        self.predicados.iter().find(|p| p.clave == clave)
    }

    /// Sorte de nodo correspondiente a un valor de `type:` en disco: si
    /// `valor` es un alias declarado en `sortes_nodo_disco`, devuelve la
    /// sorte que lo tiene como alias; si no, y `valor` nombra una sorte
    /// declarada, la devuelve tal cual; si no, `None`.
    pub fn sorte_desde_disco(&self, valor: &str) -> Option<Sorte> {
        if let Some((sorte, _)) = self
            .sortes_nodo_disco
            .iter()
            .find(|(_, v)| v.as_str() == valor)
        {
            return Some(sorte.clone());
        }
        self.sortes_nodo
            .iter()
            .find(|s| s.como_str() == valor)
            .cloned()
    }

    /// Valor de `type:` en disco para `sorte`: su alias si lo tiene, si no
    /// el propio nombre de la sorte.
    pub fn sorte_a_disco<'a>(&'a self, sorte: &'a Sorte) -> &'a str {
        self.sortes_nodo_disco
            .get(sorte)
            .map(String::as_str)
            .unwrap_or_else(|| sorte.como_str())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn base() -> Manifiesto {
        Manifiesto {
            version: "test/1".into(),
            sortes_nodo: vec![
                Sorte::nuevo("a"),
                Sorte::nuevo("Retirado"),
                Sorte::nuevo("Patrón"),
            ],
            sortes_nodo_disco: Default::default(),
            retirado_en_disco: None,
            aristas: vec![
                DeclArista {
                    clave: "rel".into(),
                    simetrica: true,
                    inversa: None,
                    destino: None,
                    etiqueta: None,
                },
                DeclArista {
                    clave: "sub".into(),
                    simetrica: false,
                    inversa: Some("sup".into()),
                    destino: None,
                    etiqueta: None,
                },
                DeclArista {
                    clave: "sup".into(),
                    simetrica: false,
                    inversa: Some("sub".into()),
                    destino: None,
                    etiqueta: None,
                },
            ],
            predicados: vec![DeclPredicado {
                nombre: "Nivel".into(),
                clave: "nivel".into(),
                dominio: ["bajo", "alto"].into_iter().map(String::from).collect(),
            }],
            layout: Layout {
                directorios: vec!["n".into()],
                sin_iota: vec![],
                ignorados: vec!["log.md".into()],
            },
        }
    }

    #[test]
    fn manifiesto_valido_pasa() {
        base().validar().unwrap();
    }

    #[test]
    fn rechaza_version_vacia() {
        let mut m = base();
        m.version.clear();
        assert!(
            m.validar()
                .unwrap_err()
                .contains(&ErrorManifiesto::VersionVacia)
        );
    }
    #[test]
    fn rechaza_sorte_reservado_ausente() {
        let mut m = base();
        m.sortes_nodo.retain(|s| s.como_str() != "Patrón");
        assert!(
            m.validar()
                .unwrap_err()
                .contains(&ErrorManifiesto::ReservadoAusente("Patrón".into()))
        );
    }
    #[test]
    fn rechaza_kv_y_ke_no_disjuntos() {
        let mut m = base();
        m.sortes_nodo.push(Sorte::nuevo("rel"));
        assert!(
            m.validar()
                .unwrap_err()
                .contains(&ErrorManifiesto::SorteEnAmbosAlfabetos("rel".into()))
        );
    }
    #[test]
    fn rechaza_inversa_no_reciproca() {
        let mut m = base();
        m.aristas[2].inversa = None;
        assert!(
            m.validar()
                .unwrap_err()
                .contains(&ErrorManifiesto::InversaNoReciproca("sub".into()))
        );
    }
    #[test]
    fn rechaza_simetrica_con_inversa() {
        let mut m = base();
        m.aristas[0].inversa = Some("sub".into());
        assert!(
            m.validar()
                .unwrap_err()
                .contains(&ErrorManifiesto::SimetricaConInversa("rel".into()))
        );
    }
    #[test]
    fn rechaza_etiqueta_sin_destino() {
        let mut m = base();
        m.aristas[0].etiqueta = Some("scope".into());
        assert!(
            m.validar()
                .unwrap_err()
                .contains(&ErrorManifiesto::EtiquetaSinDestino("rel".into()))
        );
    }
    #[test]
    fn rechaza_destino_sin_etiqueta() {
        let mut m = base();
        m.aristas[0].destino = Some("with".into());
        assert!(
            m.validar()
                .unwrap_err()
                .contains(&ErrorManifiesto::DestinoSinEtiqueta("rel".into()))
        );
    }
    #[test]
    fn rechaza_predicado_base_o_dominio_vacio() {
        let mut m = base();
        m.predicados[0].nombre = "Type".into();
        assert!(
            m.validar()
                .unwrap_err()
                .contains(&ErrorManifiesto::PredicadoReservado("Type".into()))
        );
        let mut m = base();
        m.predicados[0].dominio.clear();
        assert!(
            m.validar()
                .unwrap_err()
                .contains(&ErrorManifiesto::DominioVacio("Nivel".into()))
        );
    }
    #[test]
    fn rechaza_clave_de_frontmatter_en_conflicto() {
        let mut m = base();
        m.predicados[0].clave = "title".into();
        assert!(
            m.validar()
                .unwrap_err()
                .contains(&ErrorManifiesto::ClaveReservada("title".into()))
        );
        let mut m = base();
        m.predicados[0].clave = "rel".into();
        assert!(
            m.validar()
                .unwrap_err()
                .contains(&ErrorManifiesto::ClaveDuplicada("rel".into()))
        );
    }
    #[test]
    fn rechaza_sin_iota_fuera_de_directorios() {
        let mut m = base();
        m.layout.sin_iota.push("otro".into());
        assert!(
            m.validar()
                .unwrap_err()
                .contains(&ErrorManifiesto::SinIotaFueraDeLayout("otro".into()))
        );
    }
    #[test]
    fn rechaza_duplicados_y_directorios_vacio() {
        let mut m = base();
        m.sortes_nodo.push(Sorte::nuevo("a"));
        assert!(
            m.validar()
                .unwrap_err()
                .contains(&ErrorManifiesto::SorteDuplicado("a".into()))
        );
        let mut m = base();
        m.aristas.push(m.aristas[0].clone());
        assert!(
            m.validar()
                .unwrap_err()
                .contains(&ErrorManifiesto::AristaDuplicada("rel".into()))
        );
        let mut m = base();
        m.layout.directorios.clear();
        assert!(
            m.validar()
                .unwrap_err()
                .contains(&ErrorManifiesto::DirectoriosVacio)
        );
        let mut m = base();
        m.aristas[1].inversa = Some("sub".into());
        assert!(
            m.validar()
                .unwrap_err()
                .contains(&ErrorManifiesto::InversaASiMisma("sub".into()))
        );
        let mut m = base();
        m.aristas[1].inversa = Some("nope".into());
        assert!(
            m.validar()
                .unwrap_err()
                .contains(&ErrorManifiesto::InversaDesconocida("nope".into()))
        );
    }
    #[test]
    fn rechaza_alias_de_disco_colisionante() {
        let mut m = base();
        m.sortes_nodo_disco
            .insert(Sorte::nuevo("Patrón"), "a".into());
        assert!(
            m.validar()
                .unwrap_err()
                .contains(&ErrorManifiesto::AliasDeDiscoColisiona("a".into()))
        );
    }
    #[test]
    fn rechaza_alias_de_sorte_desconocida() {
        let mut m = base();
        m.sortes_nodo_disco.insert(Sorte::nuevo("zzz"), "z".into());
        assert!(
            m.validar()
                .unwrap_err()
                .contains(&ErrorManifiesto::AliasDeSorteDesconocida("zzz".into()))
        );
    }
    #[test]
    fn rechaza_nombres_invalidos() {
        let mut m = base();
        m.sortes_nodo.push(Sorte::nuevo("a b"));
        m.aristas.push(DeclArista {
            clave: "re,l".into(),
            simetrica: false,
            inversa: None,
            destino: None,
            etiqueta: None,
        });
        m.predicados.push(DeclPredicado {
            nombre: "Ni(vel".into(),
            clave: "otro".into(),
            dominio: ["alto, medio"].into_iter().map(String::from).collect(),
        });
        let errores = m.validar().unwrap_err();
        for esperado in [
            ErrorManifiesto::NombreInvalido("a b".into()),
            ErrorManifiesto::NombreInvalido("re,l".into()),
            ErrorManifiesto::NombreInvalido("Ni(vel".into()),
            ErrorManifiesto::NombreInvalido("alto, medio".into()),
        ] {
            assert!(
                errores.contains(&esperado),
                "falta {esperado:?} en {errores:?}"
            );
        }
    }
    #[test]
    fn rechaza_reservado_duplicado() {
        let mut m = base();
        m.sortes_nodo.push(Sorte::nuevo("Retirado"));
        assert!(
            m.validar()
                .unwrap_err()
                .contains(&ErrorManifiesto::ReservadoDuplicado("Retirado".into()))
        );
    }
    #[test]
    fn rechaza_predicado_duplicado() {
        let mut m = base();
        m.predicados.push(DeclPredicado {
            nombre: "Nivel".into(),
            clave: "nivel2".into(),
            dominio: ["bajo", "alto"].into_iter().map(String::from).collect(),
        });
        assert!(
            m.validar()
                .unwrap_err()
                .contains(&ErrorManifiesto::PredicadoDuplicado("Nivel".into()))
        );
    }
    #[test]
    fn rechaza_clave_de_retirado_en_disco() {
        let mut m = base();
        m.retirado_en_disco = Some(("nivel".into(), "x".into()));
        assert!(
            m.validar()
                .unwrap_err()
                .contains(&ErrorManifiesto::ClaveReservada("nivel".into()))
        );
    }
    #[test]
    fn consultas() {
        let mut m = base();
        m.sortes_nodo_disco
            .insert(Sorte::nuevo("Patrón"), "pattern".into());
        assert!(m.es_sorte_nodo(&Sorte::nuevo("a")));
        assert!(!m.es_sorte_nodo(&Sorte::nuevo("rel")));
        assert!(m.es_sorte_arista("rel"));
        assert_eq!(m.arista("sub").unwrap().inversa.as_deref(), Some("sup"));
        assert_eq!(m.predicado("Nivel").unwrap().clave, "nivel");
        assert_eq!(m.predicado_por_clave("nivel").unwrap().nombre, "Nivel");
        assert_eq!(m.sorte_desde_disco("a").unwrap().como_str(), "a");
        assert_eq!(m.sorte_desde_disco("pattern").unwrap().como_str(), "Patrón");
        assert_eq!(m.sorte_desde_disco("nope"), None);
        assert_eq!(m.sorte_a_disco(&Sorte::nuevo("Patrón")), "pattern");
        assert_eq!(m.sorte_a_disco(&Sorte::nuevo("a")), "a");
    }
}
