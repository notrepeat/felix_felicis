use universo_core::{BundlePath, Estructura, Etiqueta, Sorte};
pub fn path(value: &str) -> BundlePath {
    BundlePath::nuevo(value).unwrap()
}
pub fn fixture() -> Estructura {
    let mut e = Estructura::nueva(universo_testkit::manifiesto_minimo()).unwrap();
    let root = e
        .insertar_nodo(path("n/root"), Sorte::nuevo("a"), Etiqueta::nuevo("root"))
        .unwrap();
    let middle = e
        .insertar_nodo(
            path("n/middle"),
            Sorte::nuevo("b"),
            Etiqueta::nuevo("middle"),
        )
        .unwrap();
    let leaf = e
        .insertar_nodo(path("n/leaf"), Sorte::nuevo("a"), Etiqueta::nuevo("leaf"))
        .unwrap();
    e.asignar_padre(middle, root).unwrap();
    e.asignar_padre(leaf, middle).unwrap();
    for label in ["edge", "other"] {
        e.insertar_arista(root, middle, Sorte::nuevo("rel"), Etiqueta::nuevo(label))
            .unwrap();
    }
    e.asignar_atributo(root, "Nivel", "alto").unwrap();
    e
}
