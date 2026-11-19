use universo_core::{Atomo, BundlePath, Delta, ErrorDelta, LadoDelta};

fn atomo(texto: &str) -> Atomo {
    Atomo::parsear(texto).unwrap()
}

fn textos(atomos: &[Atomo]) -> Vec<String> {
    atomos.iter().map(ToString::to_string).collect()
}

#[test]
fn conserva_add_y_remove_como_conjuntos_tipados_en_orden_canonico() {
    let delta = Delta::nuevo(
        [
            atomo("Type(n/b, a)"),
            atomo("Parent(n/b, n/a)"),
            atomo("Type(n/a, a)"),
        ],
        [atomo("HasLab(n/a, \"vieja\")")],
    )
    .unwrap();

    assert_eq!(
        textos(delta.agregar()),
        ["Parent(n/b, n/a)", "Type(n/a, a)", "Type(n/b, a)"]
    );
    assert_eq!(textos(delta.retirar()), ["HasLab(n/a, \"vieja\")"]);
}

#[test]
fn canonicaliza_y_deduplica_atomos_tipados() {
    let a = atomo("Type(n/a, a)");
    let delta = Delta::nuevo([atomo("Parent(n/a, n/root)"), a.clone(), a.clone()], []).unwrap();

    assert_eq!(delta.agregar().len(), 2);
    assert_eq!(
        Delta::nuevo(delta.agregar().to_vec(), delta.retirar().to_vec()).unwrap(),
        delta
    );
}

#[test]
fn rechaza_solapamiento_entre_add_y_remove_despues_de_canonicalizar() {
    let a = atomo("Type(n/a, a)");
    let resultado = Delta::nuevo([a.clone(), a.clone()], [a]);

    let error = resultado.unwrap_err();
    assert!(matches!(error, ErrorDelta::Solapamiento));
    let redactado = format!("{error} {error:?}");
    assert!(!redactado.contains("n/a"));
}

#[test]
fn rechaza_atomo_typed_sin_roundtrip_y_redacta_su_identidad() {
    let atomo = Atomo::Ext {
        predicado: "Type".to_string(),
        sujeto: BundlePath::nuevo("n/PRIVATE_PATH").unwrap(),
        constante: "a".to_string(),
    };

    let error = Delta::nuevo([atomo], []).unwrap_err();
    assert!(matches!(
        error,
        ErrorDelta::AtomoNoRepresentable {
            lado: LadoDelta::Add,
            indice: 0
        }
    ));
    assert!(!format!("{error} {error:?}").contains("PRIVATE_PATH"));
}

#[test]
fn rechaza_ext_con_predicados_reservados_y_redacta_su_identidad() {
    let sujeto = BundlePath::nuevo("n/PRIVATE_PATH").unwrap();
    let casos = [
        ("Type", "PRIVATE_SORTE"),
        (
            "Edge",
            "n/private-target#rel#0, n/private-source, n/private-target, private-rel",
        ),
        ("Parent", "n/PRIVATE_PARENT"),
        ("HasLab", "\"PRIVATE_LABEL\""),
    ];

    for (predicado, constante) in casos {
        let atomo = Atomo::Ext {
            predicado: predicado.to_string(),
            sujeto: sujeto.clone(),
            constante: constante.to_string(),
        };
        let error = Delta::nuevo([atomo], []).unwrap_err();

        assert!(matches!(
            error,
            ErrorDelta::AtomoNoRepresentable {
                lado: LadoDelta::Add,
                indice: 0
            }
        ));
        let redactado = format!("{error} {error:?}");
        for dato_privado in [
            "PRIVATE_PATH",
            "PRIVATE_SORTE",
            "private-target",
            "private-source",
            "private-rel",
            "PRIVATE_PARENT",
            "PRIVATE_LABEL",
        ] {
            assert!(!redactado.contains(dato_privado));
        }
    }
}

#[test]
fn debug_de_delta_muestra_cantidades_sin_atom_values() {
    let delta = Delta::nuevo([atomo("Type(n/PRIVATE_PATH, a)")], []).unwrap();
    let debug = format!("{delta:?}");

    assert!(debug.contains("add_count: 1"));
    assert!(debug.contains("remove_count: 0"));
    assert!(!debug.contains("PRIVATE_PATH"));
}
