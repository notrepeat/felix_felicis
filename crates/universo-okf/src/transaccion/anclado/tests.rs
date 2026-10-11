use super::*;
use tempfile::tempdir;

#[test]
fn root_ausente_conserva_parent_y_se_abre_despues_de_recovery() {
    let temp = tempdir().unwrap();
    let ruta = temp.path().join("bundle");
    let ancla = DirectorioAnclado::desde_ubicacion(&UbicacionRaiz::nueva(&ruta).unwrap()).unwrap();

    assert!(ancla.identidad_raiz().is_none());
    assert!(ancla.abrir_raiz().is_err());
    std::fs::create_dir(&ruta).unwrap();
    let raiz = ancla.abrir_raiz().unwrap();
    assert!(raiz.identidad_raiz().is_some());
    assert_eq!(
        raiz.identidad_raiz(),
        ancla.abrir_raiz().unwrap().identidad_raiz()
    );
}
