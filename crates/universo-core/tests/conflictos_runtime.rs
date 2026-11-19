use std::time::Instant;

use universo_core::{
    Atomo, BundlePath, ClaseConflicto, Delta, Estructura, Etiqueta, Sorte, Sujeto,
    detectar_conflictos_funcionales,
};

const TYPE_CONFLICTS: usize = 334;
const HASLAB_CONFLICTS: usize = 333;
const PARENT_CONFLICTS: usize = 333;
const TOTAL_CONFLICTS: usize = TYPE_CONFLICTS + HASLAB_CONFLICTS + PARENT_CONFLICTS;

fn path(texto: &str) -> BundlePath {
    BundlePath::nuevo(texto).expect("path sintetico valido")
}

fn nodo(estructura: &mut Estructura, path: BundlePath, etiqueta: &str) {
    estructura
        .insertar_nodo(path, Sorte::nuevo("a"), Etiqueta::nuevo(etiqueta))
        .expect("nodo sintetico valido");
}

fn fixture() -> (Estructura, Delta, Delta) {
    let mut snapshot =
        Estructura::nueva(universo_testkit::manifiesto_minimo()).expect("manifiesto valido");
    let padre_anterior = path("n/parent-old");
    let padre_nuevo = path("n/parent-new");
    nodo(&mut snapshot, padre_anterior.clone(), "parent-label");
    nodo(&mut snapshot, padre_nuevo.clone(), "parent-label");
    let handle_padre_anterior = snapshot
        .por_path(&padre_anterior)
        .expect("padre anterior existente");

    let mut altas = Vec::with_capacity(TOTAL_CONFLICTS);
    for indice in 0..TYPE_CONFLICTS {
        let sujeto = path(&format!("n/type-{indice:04}"));
        nodo(&mut snapshot, sujeto.clone(), "type-label");
        altas.push(Atomo::Type {
            sujeto,
            sorte: Sorte::nuevo("b"),
        });
    }
    for indice in 0..HASLAB_CONFLICTS {
        let sujeto = path(&format!("n/label-{indice:04}"));
        nodo(&mut snapshot, sujeto.clone(), "runtime-label-old");
        altas.push(Atomo::HasLab {
            sujeto: Sujeto::Nodo(sujeto),
            etiqueta: Etiqueta::nuevo("runtime-label-new"),
        });
    }
    for indice in 0..PARENT_CONFLICTS {
        let sujeto = path(&format!("n/child-{indice:04}"));
        nodo(&mut snapshot, sujeto.clone(), "child-label");
        let handle = snapshot
            .por_path(&sujeto)
            .expect("hijo sintetico existente");
        snapshot
            .asignar_padre(handle, handle_padre_anterior)
            .expect("bosque sintetico valido");
        altas.push(Atomo::Parent {
            hijo: sujeto,
            padre: padre_nuevo.clone(),
        });
    }

    let delta = Delta::nuevo(altas.clone(), []).expect("delta sintetico valido");
    let delta_reordenado = Delta::nuevo(altas.into_iter().rev(), []).expect("delta valido");
    (snapshot, delta, delta_reordenado)
}

fn huella_snapshot_fnv1a64(snapshot: &Estructura) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for atomo in snapshot.atomos() {
        for byte in atomo.to_string().bytes().chain(std::iter::once(b'\n')) {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

#[test]
#[ignore = "harness de runtime; ejecutar en release con --ignored --nocapture"]
fn detector_mide_1000_conflictos_funcionales() {
    let (snapshot, delta, delta_reordenado) = fixture();
    let antes = snapshot.atomos();
    let huella_antes = huella_snapshot_fnv1a64(&snapshot);

    let inicio = Instant::now();
    let resultado =
        detectar_conflictos_funcionales(&snapshot, &delta).expect("topologia Edge valida");
    let duracion = inicio.elapsed();
    let replay = detectar_conflictos_funcionales(&snapshot, &delta_reordenado)
        .expect("replay con topologia Edge valida");

    assert_eq!(
        resultado, replay,
        "resultado determinista al reordenar altas"
    );
    assert_eq!(resultado.total_conflictos(), TOTAL_CONFLICTS);
    assert_eq!(
        resultado
            .diagnosticos()
            .iter()
            .map(|diagnostico| (diagnostico.clase(), diagnostico.cantidad()))
            .collect::<Vec<_>>(),
        [
            (ClaseConflicto::Type, TYPE_CONFLICTS),
            (ClaseConflicto::HasLab, HASLAB_CONFLICTS),
            (ClaseConflicto::Parent, PARENT_CONFLICTS)
        ]
    );
    assert_eq!(snapshot.atomos(), antes, "snapshot intacto");
    let huella_despues = huella_snapshot_fnv1a64(&snapshot);
    assert_eq!(huella_antes, huella_despues, "huella del snapshot intacta");

    let salida = format!("{resultado} {resultado:?}");
    for canario in ["type-0000", "label-0000", "child-0000", "runtime-label-"] {
        assert!(!salida.contains(canario), "se filtró un canario: {salida}");
    }

    let segundos = duracion.as_secs_f64().max(f64::MIN_POSITIVE);
    let conflictos_s = resultado.total_conflictos() as f64 / segundos;
    println!(
        "runtime conflicts={} elapsed_ms={:.3} conflictos_s={:.1} summary_classes={} snapshot_unchanged=true",
        resultado.total_conflictos(),
        duracion.as_secs_f64() * 1_000.0,
        conflictos_s,
        resultado.diagnosticos().len(),
    );
}
