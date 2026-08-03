//! Fails if `docs/PROJECT_FACTS.md` drifts from the constants that actually
//! define the numbers it claims to be the single source of truth for.

use hammurabi_handoff::vault::CREDENTIAL_TTL_SECS;
use hammurabi_handoff::x402::EXECUTION_FEE_MICRO_USDC;

fn project_facts() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/PROJECT_FACTS.md"
    ))
    .expect("docs/PROJECT_FACTS.md must exist")
}

fn with_thousands_separators(n: u64) -> String {
    n.to_string()
        .as_bytes()
        .rchunks(3)
        .rev()
        .map(std::str::from_utf8)
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
        .join(",")
}

#[test]
fn ttl_matches_project_facts() {
    let expected = format!("**{CREDENTIAL_TTL_SECS} seconds**");
    assert!(
        project_facts().contains(&expected),
        "vault::CREDENTIAL_TTL_SECS is {CREDENTIAL_TTL_SECS}, but docs/PROJECT_FACTS.md's \
         Credential TTL row no longer says \"{expected}\" — update the doc to match the code"
    );
}

#[test]
fn execution_fee_matches_project_facts() {
    let dollars = EXECUTION_FEE_MICRO_USDC as f64 / 1_000_000.0;
    let expected_display = format!("**{dollars:.2} USDC**");
    let expected_micro = format!(
        "{} micro-USDC",
        with_thousands_separators(EXECUTION_FEE_MICRO_USDC)
    );
    let doc = project_facts();
    assert!(
        doc.contains(&expected_display) && doc.contains(&expected_micro),
        "x402::EXECUTION_FEE_MICRO_USDC is {EXECUTION_FEE_MICRO_USDC}, but docs/PROJECT_FACTS.md's \
         Execution fee row no longer says \"{expected_display}\" / \"{expected_micro}\" — update the doc to match the code"
    );
}
