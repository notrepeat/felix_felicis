use std::sync::Arc;

use universo_okf::transaccion::snapshot::{EntradaSnapshot, calcular_token_snapshot};

fn token(
    motor: &str,
    policy: &str,
    manifiesto: &[u8],
    invariantes: &[u8],
    entradas: &[EntradaSnapshot],
) -> String {
    calcular_token_snapshot(motor, policy, manifiesto, invariantes, entradas)
        .unwrap()
        .to_hex()
}

#[test]
fn framing_v1_publico_es_determinista_opaco_y_sensible_a_cambios() {
    let archivo = EntradaSnapshot::archivo("n/á.md", Arc::<[u8]>::from(&b"CANARY-CONTENT"[..]));
    let token_base = token(
        "engine/1",
        "policy/1",
        b"manifest",
        b"rules",
        &[archivo.clone()],
    );

    assert_eq!(
        token_base,
        "6cffaa8a4d9cfcf5e298d8f73d72d6ada739b383d4aa9bdf21d30bba5577fb72"
    );
    assert_eq!(
        token(
            "engine/1",
            "policy/1",
            b"manifest",
            b"rules",
            &[archivo.clone()]
        ),
        token_base
    );
    assert_ne!(
        token(
            "engine/2",
            "policy/1",
            b"manifest",
            b"rules",
            &[archivo.clone()]
        ),
        token_base
    );
    assert_ne!(
        token(
            "engine/1",
            "policy/2",
            b"manifest",
            b"rules",
            &[archivo.clone()]
        ),
        token_base
    );
    assert_ne!(
        token(
            "engine/1",
            "policy/1",
            b"manifest ",
            b"rules",
            &[archivo.clone()]
        ),
        token_base
    );
    assert_ne!(
        token(
            "engine/1",
            "policy/1",
            b"manifest",
            b"rules ",
            &[archivo.clone()]
        ),
        token_base
    );
    let hash =
        calcular_token_snapshot("engine/1", "policy/1", b"manifest", b"rules", &[archivo]).unwrap();
    assert!(!format!("{hash:?}").contains(&token_base));
    assert!(!hash.to_string().contains(&token_base));
    assert!(!format!("{hash:?}").contains("CANARY"));
}
