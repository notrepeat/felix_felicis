use super::super::super::super::intent_persistido_v2::RETIRE_V2_VERSION;
use super::super::{
    VersionRetiro, clasificar, clasificar_con_cuotas, decodificar, maximo_de_cuotas,
};
use super::{
    ErrorSelectorIntentRetiro, MAX_INTENT_BYTES, MAX_RETIRE_V2_BYTES, MAX_VERSIONED_FRAME_BYTES,
    VERSION, header, record_v1, record_v2,
};

#[test]
fn truncamiento_y_trailing_conservan_la_rama_elegida() {
    let frame_v1 = record_v1().encode().unwrap();
    let frame_v2 = record_v2().encode().unwrap();
    for (frame, error) in [
        (&frame_v1, ErrorSelectorIntentRetiro::DecoderV1),
        (&frame_v2, ErrorSelectorIntentRetiro::DecoderV2),
    ] {
        assert_eq!(decodificar(&frame[..frame.len() - 1]), Err(error));
        let mut trailing = frame.clone();
        trailing.push(0);
        assert_eq!(decodificar(&trailing), Err(error));
    }
}

#[test]
fn cuotas_independientes_se_combinan_con_maximo_y_cap_previo() {
    assert_eq!(maximo_de_cuotas(8, 16), 16);
    assert_eq!(maximo_de_cuotas(16, 8), 16);
    assert_eq!(
        MAX_VERSIONED_FRAME_BYTES,
        maximo_de_cuotas(MAX_INTENT_BYTES, MAX_RETIRE_V2_BYTES)
    );
    assert_eq!(
        clasificar_con_cuotas(&header(RETIRE_V2_VERSION), 8, 16),
        Ok(VersionRetiro::V2)
    );
    assert_eq!(
        clasificar_con_cuotas(&header(VERSION), 16, 8),
        Ok(VersionRetiro::V1)
    );
    let mut v2_en_limite = header(RETIRE_V2_VERSION);
    v2_en_limite.push(0);
    let mut v1_en_limite = header(VERSION);
    v1_en_limite.push(0);
    assert_eq!(v2_en_limite.len(), 16);
    assert_eq!(
        clasificar_con_cuotas(&v2_en_limite, 8, 16),
        Ok(VersionRetiro::V2)
    );
    assert_eq!(v1_en_limite.len(), 16);
    assert_eq!(
        clasificar_con_cuotas(&v1_en_limite, 16, 8),
        Ok(VersionRetiro::V1)
    );
    assert_eq!(
        clasificar_con_cuotas(&[0; 17], 8, 16),
        Err(ErrorSelectorIntentRetiro::TotalExcedeLimite)
    );
    assert_eq!(
        clasificar_con_cuotas(&[0; 17], 16, 8),
        Err(ErrorSelectorIntentRetiro::TotalExcedeLimite)
    );
    assert_eq!(clasificar(&header(VERSION)), Ok(VersionRetiro::V1));
}
