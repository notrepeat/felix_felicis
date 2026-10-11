use std::time::Instant;

use universo_core::{
    AristaId, Atomo, BundlePath, Delta, Estructura, Etiqueta, IdArista, NodoId, Sorte, Sujeto,
    construir_candidato,
};

const CAMBIOS_EXT: usize = 1_000;
const ARISTAS_BASE: usize = 3_500;
const GRUPOS_ARISTAS: [usize; 3] = [1, 100, 10_000];

struct EntradaSintetica {
    estructura: Estructura,
    aristas: Vec<AristaId>,
    nodos_ext: Vec<NodoId>,
}

fn path(texto: &str) -> BundlePath {
    BundlePath::nuevo(texto).expect("path sintetico valido")
}

fn entrada_sintetica(cantidad_aristas: usize, cantidad_ext: usize) -> EntradaSintetica {
    let mut estructura =
        Estructura::nueva(universo_testkit::manifiesto_minimo()).expect("manifiesto valido");
    let origen = estructura
        .insertar_nodo(path("n/origen"), Sorte::nuevo("a"), Etiqueta::nuevo("n"))
        .expect("nodo origen valido");
    let destino = estructura
        .insertar_nodo(path("n/destino"), Sorte::nuevo("b"), Etiqueta::nuevo("n"))
        .expect("nodo destino valido");

    let mut nodos_ext = Vec::with_capacity(cantidad_ext);
    for indice in 0..cantidad_ext {
        let path =
            BundlePath::nuevo(&format!("n/valor-{indice:04}")).expect("path sintetico valido");
        let nodo = estructura
            .insertar_nodo(path, Sorte::nuevo("a"), Etiqueta::nuevo("n"))
            .expect("nodo sintetico valido");
        estructura
            .asignar_atributo(nodo, "Nivel", "alto")
            .expect("atributo sintetico valido");
        nodos_ext.push(nodo);
    }

    let mut aristas = Vec::with_capacity(cantidad_aristas);
    for _ in 0..cantidad_aristas {
        let arista = estructura
            .insertar_arista(origen, destino, Sorte::nuevo("rel"), Etiqueta::nuevo("e"))
            .expect("arista sintetica valida");
        aristas.push(arista);
    }

    EntradaSintetica {
        estructura,
        aristas,
        nodos_ext,
    }
}

fn atoms_retiro_arista(estructura: &Estructura, handle: AristaId) -> [Atomo; 2] {
    let arista = estructura.arista(handle).expect("arista existente");
    let origen = estructura
        .nodo(arista.origen)
        .expect("origen existente")
        .path
        .clone();
    let destino = estructura
        .nodo(arista.destino)
        .expect("destino existente")
        .path
        .clone();
    let id = IdArista {
        origen: origen.clone(),
        sorte: arista.tipo.clone(),
        destino: destino.clone(),
        ordinal: arista.ordinal,
    };

    [
        Atomo::Edge {
            id: id.clone(),
            origen,
            destino,
            sorte: arista.tipo.clone(),
        },
        Atomo::HasLab {
            sujeto: Sujeto::Arista(id),
            etiqueta: arista.etiqueta.clone(),
        },
    ]
}

fn medir_representacion(estructura: &Estructura) -> (usize, usize) {
    let atomos = estructura.atomos();
    let bytes_canonicos = atomos.iter().map(|atomo| atomo.to_string().len()).sum();
    (atomos.len(), bytes_canonicos)
}

fn medir_grupo_aristas(cantidad: usize) {
    let entrada = entrada_sintetica(cantidad, 0);
    let snapshot = &entrada.estructura;
    let handle = *entrada.aristas.last().expect("grupo con arista");
    let antes = snapshot.atomos();
    let delta = Delta::nuevo([], atoms_retiro_arista(snapshot, handle)).expect("delta valido");

    let inicio = Instant::now();
    let resultado = construir_candidato(snapshot, &delta);
    let duracion = inicio.elapsed();
    let Ok(candidato) = resultado else {
        panic!("construccion del candidato rechazada");
    };

    assert!(
        snapshot.arista(handle).is_some(),
        "el snapshot conserva la arista"
    );
    assert!(
        candidato.estructura().arista(handle).is_none(),
        "el candidato retira la arista seleccionada"
    );
    assert!(snapshot.atomos() == antes, "la entrada permanece intacta");
    assert!(
        snapshot.verificar_representacion().is_ok(),
        "snapshot consistente"
    );
    assert!(
        candidato.estructura().verificar_representacion().is_ok(),
        "candidato consistente"
    );

    let (candidate_atoms, canonical_string_bytes) = medir_representacion(candidato.estructura());
    assert_eq!(candidate_atoms + 2, antes.len(), "retiro Edge y HasLab");
    println!(
        "runtime group_edges={cantidad} changes=1 delta_atoms={} elapsed_ms={:.3} candidate_atoms={candidate_atoms} canonical_string_bytes={canonical_string_bytes}",
        delta.agregar().len() + delta.retirar().len(),
        duracion.as_secs_f64() * 1_000.0,
    );
}

fn medir_cambios_ext() {
    let entrada = entrada_sintetica(ARISTAS_BASE, CAMBIOS_EXT);
    let snapshot = &entrada.estructura;
    let antes = snapshot.atomos();
    let mut agregar = Vec::with_capacity(CAMBIOS_EXT);
    let mut retirar = Vec::with_capacity(CAMBIOS_EXT);
    for nodo in &entrada.nodos_ext {
        let sujeto = snapshot.nodo(*nodo).expect("nodo existente").path.clone();
        agregar.push(Atomo::Ext {
            predicado: "Nivel".into(),
            sujeto: sujeto.clone(),
            constante: "bajo".into(),
        });
        retirar.push(Atomo::Ext {
            predicado: "Nivel".into(),
            sujeto,
            constante: "alto".into(),
        });
    }
    let delta = Delta::nuevo(agregar, retirar).expect("delta valido");
    assert_eq!(antes.len(), 10_004, "tamaño base esperado");

    let inicio = Instant::now();
    let resultado = construir_candidato(snapshot, &delta);
    let duracion = inicio.elapsed();
    let Ok(candidato) = resultado else {
        panic!("construccion del candidato rechazada");
    };

    assert!(snapshot.atomos() == antes, "la entrada permanece intacta");
    assert!(
        snapshot.verificar_representacion().is_ok(),
        "snapshot consistente"
    );
    assert!(
        candidato.estructura().verificar_representacion().is_ok(),
        "candidato consistente"
    );
    assert!(entrada.nodos_ext.iter().all(|nodo| {
        candidato
            .estructura()
            .atributos(*nodo)
            .get("Nivel")
            .is_some_and(|valor| valor == "bajo")
    }));
    assert!(entrada.nodos_ext.iter().all(|nodo| {
        snapshot
            .atributos(*nodo)
            .get("Nivel")
            .is_some_and(|valor| valor == "alto")
    }));

    let (candidate_atoms, canonical_string_bytes) = medir_representacion(candidato.estructura());
    assert_eq!(
        candidate_atoms,
        antes.len(),
        "reemplazos conservan el conteo"
    );
    println!(
        "runtime base_atoms={} changes={CAMBIOS_EXT} delta_atoms={} elapsed_ms={:.3} candidate_atoms={candidate_atoms} canonical_string_bytes={canonical_string_bytes}",
        antes.len(),
        delta.agregar().len() + delta.retirar().len(),
        duracion.as_secs_f64() * 1_000.0,
    );
}

#[test]
#[ignore = "harness de runtime; ejecutar en release con --ignored --nocapture"]
fn candidato_mide_escala_de_ext_y_grupos_edge() {
    medir_cambios_ext();
    for cantidad in GRUPOS_ARISTAS {
        medir_grupo_aristas(cantidad);
    }
}
