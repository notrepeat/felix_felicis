use super::{FamiliaV3, clasificar};
use crate::transaccion::anclado::intent_persistido_v3::{
    IntentRetiroPersistidoV3, tests::committed_fixture,
};
use crate::transaccion::journal::selector::JournalPersistido;

#[test]
fn a_b_c_retienen_el_intent_y_contexto_autocontenido() {
    let (journal, context, metadata) = committed_fixture();
    let intent = IntentRetiroPersistidoV3::decode(
        IntentRetiroPersistidoV3::encode(&metadata, &context).unwrap(),
    )
    .unwrap();
    let journal = JournalPersistido::V2(journal);
    assert!(matches!(
        clasificar(Some(&journal), Some(&context), &intent),
        Ok(FamiliaV3::RetiroA(_))
    ));
    assert!(matches!(
        clasificar(None, Some(&context), &intent),
        Ok(FamiliaV3::RetiroB(_))
    ));
    let FamiliaV3::CConContextoParaReplay(retained) = clasificar(None, None, &intent).unwrap()
    else {
        panic!("rama incorrecta")
    };
    assert!(std::ptr::eq(retained, &intent));
    assert_eq!(retained.contexto_original(), context);
}

#[test]
fn exige_contexto_raw_exacto_no_solo_binding_estructural() {
    let (journal, context, metadata) = committed_fixture();
    let intent = IntentRetiroPersistidoV3::decode(
        IntentRetiroPersistidoV3::encode(&metadata, &context).unwrap(),
    )
    .unwrap();
    let journal = JournalPersistido::V2(journal);
    let mut changed = context;
    let last = changed.len() - 1;
    changed[last] ^= 1;
    assert!(clasificar(Some(&journal), Some(&changed), &intent).is_err());
    assert!(clasificar(None, Some(&changed), &intent).is_err());
    assert!(clasificar(None, Some(&[]), &intent).is_err());
}

#[test]
fn journal_sin_contexto_y_estado_no_committed_fallan() {
    let (journal, context, metadata) = committed_fixture();
    let intent = IntentRetiroPersistidoV3::decode(
        IntentRetiroPersistidoV3::encode(&metadata, &context).unwrap(),
    )
    .unwrap();
    let prepared = crate::transaccion::journal::v2::JournalV2::new(
        journal.transaction_id(),
        journal.parent().clone(),
        journal.old().clone(),
        journal.new_fingerprint().clone(),
    )
    .unwrap();
    let legacy = crate::transaccion::journal::Journal::new(
        journal.transaction_id(),
        journal.parent().clone(),
        journal.old().clone(),
        journal.new_fingerprint().clone(),
    )
    .unwrap();
    assert!(
        clasificar(
            Some(&JournalPersistido::V2(prepared)),
            Some(&context),
            &intent
        )
        .is_err()
    );
    assert!(
        clasificar(
            Some(&JournalPersistido::V1(legacy)),
            Some(&context),
            &intent
        )
        .is_err()
    );
    let journal = JournalPersistido::V2(journal);
    assert!(clasificar(Some(&journal), None, &intent).is_err());
}
