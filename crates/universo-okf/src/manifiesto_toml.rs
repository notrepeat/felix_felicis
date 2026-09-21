//! Carga de un `Manifiesto` desde TOML (spec §4.5): `parsear_manifiesto`
//! sobre un texto ya leído, `cargar_manifiesto` sobre un archivo en disco.
//! Tras deserializar se llama `validar()` y todo error se reporta.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use universo_core::{DeclArista, DeclPredicado, Layout, Manifiesto, Sorte};

use crate::errores::ErrorManifiestoArchivo;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifiestoToml {
    version: String,
    nodos: NodosToml,
    #[serde(default)]
    aristas: Vec<AristaToml>,
    #[serde(default)]
    predicados: Vec<PredicadoToml>,
    layout: LayoutToml,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NodosToml {
    sortes: Vec<String>,
    #[serde(default)]
    disco: BTreeMap<String, String>,
    #[serde(default)]
    retirado_en_disco: Option<RetiradoEnDiscoToml>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RetiradoEnDiscoToml {
    clave: String,
    valor: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AristaToml {
    clave: String,
    #[serde(default)]
    simetrica: bool,
    #[serde(default)]
    inversa: Option<String>,
    #[serde(default)]
    destino: Option<String>,
    #[serde(default)]
    etiqueta: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PredicadoToml {
    nombre: String,
    clave: String,
    dominio: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LayoutToml {
    directorios: Vec<String>,
    #[serde(default)]
    sin_iota: Vec<String>,
    #[serde(default)]
    ignorados: Vec<String>,
}

impl From<ManifiestoToml> for Manifiesto {
    fn from(m: ManifiestoToml) -> Self {
        Manifiesto {
            version: m.version,
            sortes_nodo: m.nodos.sortes.iter().map(|s| Sorte::nuevo(s)).collect(),
            sortes_nodo_disco: m
                .nodos
                .disco
                .into_iter()
                .map(|(sorte, valor)| (Sorte::nuevo(&sorte), valor))
                .collect(),
            retirado_en_disco: m.nodos.retirado_en_disco.map(|r| (r.clave, r.valor)),
            aristas: m
                .aristas
                .into_iter()
                .map(|a| DeclArista {
                    clave: a.clave,
                    simetrica: a.simetrica,
                    inversa: a.inversa,
                    destino: a.destino,
                    etiqueta: a.etiqueta,
                })
                .collect(),
            predicados: m
                .predicados
                .into_iter()
                .map(|p| DeclPredicado {
                    nombre: p.nombre,
                    clave: p.clave,
                    dominio: p.dominio.into_iter().collect(),
                })
                .collect(),
            layout: Layout {
                directorios: m.layout.directorios,
                sin_iota: m.layout.sin_iota,
                ignorados: m.layout.ignorados,
            },
        }
    }
}

/// Parsea un manifiesto desde su texto TOML (spec §4.5): deserializa contra
/// el schema esperado (un campo desconocido es `Sintaxis`, no se ignora en
/// silencio) y llama a `Manifiesto::validar()`.
pub fn parsear_manifiesto(texto: &str) -> Result<Manifiesto, ErrorManifiestoArchivo> {
    let crudo: ManifiestoToml =
        toml::from_str(texto).map_err(|e| ErrorManifiestoArchivo::Sintaxis(e.to_string()))?;
    let manifiesto: Manifiesto = crudo.into();
    manifiesto
        .validar()
        .map_err(ErrorManifiestoArchivo::Validacion)?;
    Ok(manifiesto)
}

/// Lee `ruta` y llama a `parsear_manifiesto` sobre su contenido.
pub fn cargar_manifiesto(ruta: &Path) -> Result<Manifiesto, ErrorManifiestoArchivo> {
    let texto =
        std::fs::read_to_string(ruta).map_err(|e| ErrorManifiestoArchivo::Io(e.to_string()))?;
    parsear_manifiesto(&texto)
}

#[cfg(test)]
mod tests {
    use crate::{ErrorManifiestoArchivo, cargar_manifiesto, parsear_manifiesto};
    use universo_core::ErrorManifiesto;

    #[test]
    fn carga_el_manifiesto_publico_y_valida() {
        let m = cargar_manifiesto(&universo_testkit::ejemplo::manifiesto()).unwrap();
        assert_eq!(m.version, "example/1");
        assert_eq!(
            m.aristas
                .iter()
                .map(|a| a.clave.as_str())
                .collect::<Vec<_>>(),
            ["supports", "related"]
        );
        assert!(m.arista("related").unwrap().simetrica);
        assert!(!m.arista("supports").unwrap().simetrica);
        assert_eq!(m.sorte_desde_disco("pattern").unwrap().como_str(), "Patrón");
        assert_eq!(
            m.retirado_en_disco,
            Some(("status".into(), "deprecated".into()))
        );
        assert_eq!(m.predicado("Maturity").unwrap().dominio.len(), 2);
        assert_eq!(m.predicado("Maturity").unwrap().clave, "maturity");
        assert_eq!(m.layout.ignorados, ["README.md"]);
        assert!(m.layout.sin_iota.is_empty());
    }

    #[test]
    fn manifiesto_invalido_reporta_validacion() {
        let texto =
            "version = \"x/1\"\n[nodos]\nsortes = [\"a\"]\n[layout]\ndirectorios = [\"n\"]\n";
        let e = parsear_manifiesto(texto).unwrap_err();
        assert!(
            matches!(e, ErrorManifiestoArchivo::Validacion(ref v) if v.contains(&ErrorManifiesto::ReservadoAusente("Retirado".into()))),
            "{e}"
        );
        assert!(matches!(
            parsear_manifiesto("esto no es toml = ["),
            Err(ErrorManifiestoArchivo::Sintaxis(_))
        ));
        assert!(
            matches!(
                parsear_manifiesto(
                    "version = \"x/1\"\nclave_desconocida = 1\n[nodos]\nsortes = [\"a\"]\n[layout]\ndirectorios = [\"n\"]\n"
                ),
                Err(ErrorManifiestoArchivo::Sintaxis(_))
            ),
            "campo desconocido = error de sintaxis"
        );
        assert!(matches!(
            cargar_manifiesto(std::path::Path::new("no/existe.toml")),
            Err(ErrorManifiestoArchivo::Io(_))
        ));
    }
}
