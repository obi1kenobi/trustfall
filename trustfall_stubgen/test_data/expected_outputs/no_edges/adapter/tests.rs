use trustfall::provider::check_adapter_invariants;

use super::Adapter;

#[test]
fn adapter_satisfies_trustfall_invariants() {
    let adapter = Adapter::new();
    let schema = Adapter::schema();
    check_adapter_invariants(schema, adapter);
}
