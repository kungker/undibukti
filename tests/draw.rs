//! Engine tests for the UndiBukti `Draw` template.
//!
//! These run against the real Tari engine (in-process) via
//! `tari_template_test_tooling`. The rejection list from the design brief is
//! covered one-for-one:
//!
//! (a) double entry by one account is rejected      -> `double_entry_is_rejected`
//! (b) entry after `close_epoch` is rejected        -> `entry_after_close_is_rejected`
//! (c) draw before `draw_epoch` is rejected         -> `draw_before_draw_epoch_is_rejected`
//! (d) the winner matches an independent off-chain  -> `paid_draw_pays_the_exact_pool_to_the_winner`,
//!     recomputation from the injected epoch hash       `free_draw_full_flow`
//! (e) the payout is exact                          -> `paid_draw_pays_the_exact_pool_to_the_winner`
//! (f) refund when there are no entries             -> `refund_with_no_entries_returns_the_prize`

use tari_template_test_tooling::{
    TemplateTest,
    byte_type::ToByteType,
    crypto::RistrettoSecretKey,
    engine_types::virtual_substate::{VirtualSubstate, VirtualSubstateId},
    support::assert_error::assert_reject_reason,
    template_lib_types::{
        Amount, ComponentAddress, Hash32, NonFungibleAddress, TemplateAddress,
        constants::TARI_TOKEN, crypto::RistrettoPublicKeyBytes,
    },
    transaction::args,
};

const PRIZE: u64 = 50_000;
const TICKET: u64 = 1_000;
const CLOSE_EPOCH: u64 = 10;
const DRAW_EPOCH: u64 = 20;
const INITIAL: u64 = TemplateTest::FUNDED_ACCOUNT_INITIAL_BALANCE;

/// A funded test account with everything a transaction needs.
struct Acct {
    address: ComponentAddress,
    proof: NonFungibleAddress,
    secret: RistrettoSecretKey,
    /// The identity the template sees: the transaction signer's public key.
    pubkey: RistrettoPublicKeyBytes,
}

fn funded(test: &mut TemplateTest) -> Acct {
    let (address, proof, secret, public_key) = test.create_funded_account_with_keypair();
    Acct {
        address,
        proof,
        secret,
        pubkey: public_key.to_byte_type(),
    }
}

/// Current TARI_TOKEN balance of an account, read from the state store.
fn balance(test: &TemplateTest, acct: &Acct) -> Amount {
    let vaults = test
        .read_only_state_store()
        .get_vaults_for_account(acct.address)
        .unwrap();
    vaults
        .get(&TARI_TOKEN)
        .map(|vault| vault.balance())
        .unwrap_or(Amount::ZERO)
}

fn set_epoch(test: &mut TemplateTest, epoch: u64) {
    test.set_virtual_substate(
        VirtualSubstateId::CurrentEpoch,
        VirtualSubstate::CurrentEpoch(epoch),
    );
}

fn set_epoch_hash(test: &mut TemplateTest, hash: Hash32) {
    test.set_virtual_substate(
        VirtualSubstateId::CurrentEpochHash,
        VirtualSubstate::CurrentEpochHash(hash),
    );
}

/// A fixed, non-trivial epoch hash used by the settlement tests.
fn test_epoch_hash() -> Hash32 {
    let mut bytes = [0u8; 32];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = (i as u8).wrapping_mul(37).wrapping_add(11);
    }
    Hash32::from_array(bytes)
}

/// Independent off-chain recomputation of the winner index. This deliberately
/// duplicates the documented formula instead of calling template code: the
/// whole point is that anyone (a judge, the verification web page) can
/// recompute the winner from the public entry list and the epoch hash.
fn expected_winner_index(epoch_hash: &Hash32, entry_count: u64) -> u64 {
    u64::from_le_bytes(epoch_hash.leading_bytes::<8>()) % entry_count
}

/// Create a draw funded by `organizer` and return its component address.
fn create_draw(
    test: &mut TemplateTest,
    template: TemplateAddress,
    organizer: &Acct,
    prize: u64,
    ticket_price: u64,
) -> ComponentAddress {
    test.execute_expect_success(
        test.transaction()
            .call_method(organizer.address, "withdraw", args![TARI_TOKEN, prize])
            .put_last_instruction_output_on_workspace("prize")
            .call_function(
                template,
                "new",
                args![Workspace("prize"), ticket_price, CLOSE_EPOCH, DRAW_EPOCH],
            )
            .build_and_seal(&organizer.secret),
        vec![organizer.proof.clone()],
    );
    let components = test
        .read_only_state_store()
        .get_component_addresses_by_template_address(template)
        .unwrap();
    assert_eq!(components.len(), 1, "expected exactly one Draw component");
    components[0]
}

/// Enter a paid draw, expecting the transaction to succeed.
fn enter_paid(test: &mut TemplateTest, draw: ComponentAddress, acct: &Acct, amount: u64) {
    test.execute_expect_success(
        test.transaction()
            .call_method(acct.address, "withdraw", args![TARI_TOKEN, amount])
            .put_last_instruction_output_on_workspace("ticket")
            .call_method(draw, "enter", args![Workspace("ticket")])
            .build_and_seal(&acct.secret),
        vec![acct.proof.clone()],
    );
}

/// Enter a free draw, expecting the transaction to succeed.
fn enter_free(test: &mut TemplateTest, draw: ComponentAddress, acct: &Acct) {
    test.execute_expect_success(
        test.transaction()
            .call_method(draw, "enter_free", args![])
            .build_and_seal(&acct.secret),
        vec![acct.proof.clone()],
    );
}

/// Call a view method and decode its return value.
fn view<T>(test: &mut TemplateTest, draw: ComponentAddress, method: &str, caller: &Acct) -> T
where
    T: for<'b> tari_bor::Decode<'b, ()>,
{
    test.execute_expect_success(
        test.transaction()
            .call_method(draw, method, args![])
            .build_and_seal(&caller.secret),
        vec![caller.proof.clone()],
    );
    test.expect_return(0)
}

/// Settle the draw (callable by anyone) and deposit nothing anywhere.
fn settle(test: &mut TemplateTest, draw: ComponentAddress, caller: &Acct) {
    test.execute_expect_success(
        test.transaction()
            .call_method(draw, "draw", args![])
            .build_and_seal(&caller.secret),
        vec![caller.proof.clone()],
    );
}

/// Claim the prize as `winner` and deposit it into the winner's account.
fn claim_and_deposit(test: &mut TemplateTest, draw: ComponentAddress, winner: &Acct) {
    test.execute_expect_success(
        test.transaction()
            .call_method(draw, "claim", args![])
            .put_last_instruction_output_on_workspace("prize")
            .call_method(winner.address, "deposit", args![Workspace("prize")])
            .build_and_seal(&winner.secret),
        vec![winner.proof.clone()],
    );
}

// ---------------------------------------------------------------------------
// (d) + (e): full paid flow — the winner is exactly the account an independent
// off-chain computation predicts from the injected epoch hash, and the winner
// receives exactly the sponsor prize plus all ticket payments.
// ---------------------------------------------------------------------------
#[test]
fn paid_draw_pays_the_exact_pool_to_the_winner() {
    let mut test = TemplateTest::my_crate();
    let template = test.get_template_address("Draw");

    let organizer = funded(&mut test);
    let draw = create_draw(&mut test, template, &organizer, PRIZE, TICKET);
    assert_eq!(balance(&test, &organizer), Amount::from(INITIAL - PRIZE));

    let entrants: Vec<Acct> = (0..3).map(|_| funded(&mut test)).collect();
    for entrant in &entrants {
        enter_paid(&mut test, draw, entrant, TICKET);
    }

    // The on-chain entry list is in entry order and keyed by signer public key.
    let entries: Vec<RistrettoPublicKeyBytes> = view(&mut test, draw, "entries", &organizer);
    assert_eq!(entries.len(), 3);
    for (entry, entrant) in entries.iter().zip(&entrants) {
        assert_eq!(*entry, entrant.pubkey);
    }
    let pool: Amount = view(&mut test, draw, "prize_balance", &organizer);
    assert_eq!(pool, Amount::from(PRIZE + 3 * TICKET));

    // Move to the draw epoch and inject a known epoch hash.
    let hash = test_epoch_hash();
    set_epoch(&mut test, DRAW_EPOCH);
    set_epoch_hash(&mut test, hash);
    let expected_index = expected_winner_index(&hash, entries.len() as u64);

    // Anyone may settle the draw — use the organizer, who is not an entrant.
    settle(&mut test, draw, &organizer);

    let winner_index: Option<u64> = view(&mut test, draw, "winner_index", &organizer);
    assert_eq!(winner_index, Some(expected_index));
    let winner: Option<RistrettoPublicKeyBytes> = view(&mut test, draw, "winner", &organizer);
    assert_eq!(winner, Some(entries[expected_index as usize]));

    // The winner claims; everyone else's balance must be untouched by the claim.
    let winner_acct = &entrants[expected_index as usize];
    claim_and_deposit(&mut test, draw, winner_acct);
    assert_eq!(
        balance(&test, winner_acct),
        Amount::from(INITIAL - TICKET + PRIZE + 3 * TICKET),
        "winner must receive exactly the prize plus all ticket payments"
    );
    for (i, entrant) in entrants.iter().enumerate() {
        if i as u64 != expected_index {
            assert_eq!(balance(&test, entrant), Amount::from(INITIAL - TICKET));
        }
    }
    assert_eq!(balance(&test, &organizer), Amount::from(INITIAL - PRIZE));

    // A second claim is rejected.
    let reason = test.execute_expect_failure(
        test.transaction()
            .call_method(draw, "claim", args![])
            .build_and_seal(&winner_acct.secret),
        vec![winner_acct.proof.clone()],
    );
    assert_reject_reason(reason, "The prize has already been claimed");
}

// ---------------------------------------------------------------------------
// (a): one entry per account — a second entry by the same signer is rejected.
// ---------------------------------------------------------------------------
#[test]
fn double_entry_is_rejected() {
    let mut test = TemplateTest::my_crate();
    let template = test.get_template_address("Draw");
    let organizer = funded(&mut test);
    let draw = create_draw(&mut test, template, &organizer, PRIZE, TICKET);

    let entrant = funded(&mut test);
    enter_paid(&mut test, draw, &entrant, TICKET);

    let reason = test.execute_expect_failure(
        test.transaction()
            .call_method(entrant.address, "withdraw", args![TARI_TOKEN, TICKET])
            .put_last_instruction_output_on_workspace("ticket")
            .call_method(draw, "enter", args![Workspace("ticket")])
            .build_and_seal(&entrant.secret),
        vec![entrant.proof.clone()],
    );
    assert_reject_reason(reason, "This account has already entered the draw");

    // The failed transaction changed nothing: still one entry, one ticket paid.
    let count: u64 = view(&mut test, draw, "entry_count", &organizer);
    assert_eq!(count, 1);
    assert_eq!(balance(&test, &entrant), Amount::from(INITIAL - TICKET));
}

// ---------------------------------------------------------------------------
// (b): entries close at close_epoch.
// ---------------------------------------------------------------------------
#[test]
fn entry_after_close_is_rejected() {
    let mut test = TemplateTest::my_crate();
    let template = test.get_template_address("Draw");
    let organizer = funded(&mut test);
    let draw = create_draw(&mut test, template, &organizer, PRIZE, TICKET);

    set_epoch(&mut test, CLOSE_EPOCH);
    let entrant = funded(&mut test);
    let reason = test.execute_expect_failure(
        test.transaction()
            .call_method(entrant.address, "withdraw", args![TARI_TOKEN, TICKET])
            .put_last_instruction_output_on_workspace("ticket")
            .call_method(draw, "enter", args![Workspace("ticket")])
            .build_and_seal(&entrant.secret),
        vec![entrant.proof.clone()],
    );
    assert_reject_reason(reason, "Entries are closed");
}

// ---------------------------------------------------------------------------
// (c): the draw cannot be settled before draw_epoch.
// ---------------------------------------------------------------------------
#[test]
fn draw_before_draw_epoch_is_rejected() {
    let mut test = TemplateTest::my_crate();
    let template = test.get_template_address("Draw");
    let organizer = funded(&mut test);
    let draw = create_draw(&mut test, template, &organizer, PRIZE, TICKET);

    let entrant = funded(&mut test);
    enter_paid(&mut test, draw, &entrant, TICKET);

    // Entries are closed (epoch >= close) but the draw epoch is not reached.
    set_epoch(&mut test, DRAW_EPOCH - 1);
    let reason = test.execute_expect_failure(
        test.transaction()
            .call_method(draw, "draw", args![])
            .build_and_seal(&organizer.secret),
        vec![organizer.proof.clone()],
    );
    assert_reject_reason(reason, "The draw epoch has not been reached");
}

// ---------------------------------------------------------------------------
// Only the winner can claim: claiming before settlement and claiming as a
// non-winner are both rejected.
// ---------------------------------------------------------------------------
#[test]
fn claim_by_non_winner_is_rejected() {
    let mut test = TemplateTest::my_crate();
    let template = test.get_template_address("Draw");
    let organizer = funded(&mut test);
    let draw = create_draw(&mut test, template, &organizer, PRIZE, TICKET);

    let entrants: Vec<Acct> = (0..3).map(|_| funded(&mut test)).collect();
    for entrant in &entrants {
        enter_paid(&mut test, draw, entrant, TICKET);
    }

    // Too early: no winner exists yet.
    let reason = test.execute_expect_failure(
        test.transaction()
            .call_method(draw, "claim", args![])
            .build_and_seal(&entrants[0].secret),
        vec![entrants[0].proof.clone()],
    );
    assert_reject_reason(reason, "The draw has not been settled yet");

    let hash = test_epoch_hash();
    set_epoch(&mut test, DRAW_EPOCH);
    set_epoch_hash(&mut test, hash);
    settle(&mut test, draw, &organizer);
    let expected_index = expected_winner_index(&hash, entrants.len() as u64);

    // Every non-winner entrant is refused.
    for (i, entrant) in entrants.iter().enumerate() {
        if i as u64 != expected_index {
            let reason = test.execute_expect_failure(
                test.transaction()
                    .call_method(draw, "claim", args![])
                    .build_and_seal(&entrant.secret),
                vec![entrant.proof.clone()],
            );
            assert_reject_reason(reason, "Only the winner can claim the prize");
        }
    }
    // The organizer is not the winner either.
    let reason = test.execute_expect_failure(
        test.transaction()
            .call_method(draw, "claim", args![])
            .build_and_seal(&organizer.secret),
        vec![organizer.proof.clone()],
    );
    assert_reject_reason(reason, "Only the winner can claim the prize");
}

// ---------------------------------------------------------------------------
// (f): with no entries, the organizer recovers the full prize — but only the
// organizer, and only once the draw epoch has passed.
// ---------------------------------------------------------------------------
#[test]
fn refund_with_no_entries_returns_the_prize() {
    let mut test = TemplateTest::my_crate();
    let template = test.get_template_address("Draw");
    let organizer = funded(&mut test);
    let draw = create_draw(&mut test, template, &organizer, PRIZE, TICKET);
    assert_eq!(balance(&test, &organizer), Amount::from(INITIAL - PRIZE));

    // Too early: entries could still arrive.
    let reason = test.execute_expect_failure(
        test.transaction()
            .call_method(draw, "refund_all", args![])
            .build_and_seal(&organizer.secret),
        vec![organizer.proof.clone()],
    );
    assert_reject_reason(reason, "The draw epoch has not been reached");

    set_epoch(&mut test, DRAW_EPOCH);

    // A stranger cannot take the refund.
    let stranger = funded(&mut test);
    let reason = test.execute_expect_failure(
        test.transaction()
            .call_method(draw, "refund_all", args![])
            .build_and_seal(&stranger.secret),
        vec![stranger.proof.clone()],
    );
    assert_reject_reason(reason, "Only the organizer can refund an empty draw");

    // The organizer recovers the prize in full.
    test.execute_expect_success(
        test.transaction()
            .call_method(draw, "refund_all", args![])
            .put_last_instruction_output_on_workspace("refund")
            .call_method(organizer.address, "deposit", args![Workspace("refund")])
            .build_and_seal(&organizer.secret),
        vec![organizer.proof.clone()],
    );
    assert_eq!(balance(&test, &organizer), Amount::from(INITIAL));
}

// ---------------------------------------------------------------------------
// Settling a draw with no entries is rejected (refund is the path instead).
// ---------------------------------------------------------------------------
#[test]
fn draw_with_no_entries_is_rejected() {
    let mut test = TemplateTest::my_crate();
    let template = test.get_template_address("Draw");
    let organizer = funded(&mut test);
    let draw = create_draw(&mut test, template, &organizer, PRIZE, TICKET);

    set_epoch(&mut test, DRAW_EPOCH);
    let reason = test.execute_expect_failure(
        test.transaction()
            .call_method(draw, "draw", args![])
            .build_and_seal(&organizer.secret),
        vec![organizer.proof.clone()],
    );
    assert_reject_reason(reason, "Cannot draw with no entries");
}

// ---------------------------------------------------------------------------
// A ticket bucket that does not contain exactly the ticket price is rejected.
// ---------------------------------------------------------------------------
#[test]
fn wrong_ticket_amount_is_rejected() {
    let mut test = TemplateTest::my_crate();
    let template = test.get_template_address("Draw");
    let organizer = funded(&mut test);
    let draw = create_draw(&mut test, template, &organizer, PRIZE, TICKET);

    let entrant = funded(&mut test);
    let reason = test.execute_expect_failure(
        test.transaction()
            .call_method(entrant.address, "withdraw", args![TARI_TOKEN, TICKET - 1])
            .put_last_instruction_output_on_workspace("ticket")
            .call_method(draw, "enter", args![Workspace("ticket")])
            .build_and_seal(&entrant.secret),
        vec![entrant.proof.clone()],
    );
    assert_reject_reason(reason, "Ticket bucket must contain exactly the ticket price");
}

// ---------------------------------------------------------------------------
// Free draw (the demo's main path): five entries, no payments, the winner is
// again exactly the off-chain prediction and receives the full sponsor prize.
// ---------------------------------------------------------------------------
#[test]
fn free_draw_full_flow() {
    let mut test = TemplateTest::my_crate();
    let template = test.get_template_address("Draw");
    let organizer = funded(&mut test);
    let draw = create_draw(&mut test, template, &organizer, PRIZE, 0);

    let entrants: Vec<Acct> = (0..5).map(|_| funded(&mut test)).collect();
    for entrant in &entrants {
        enter_free(&mut test, draw, entrant);
    }
    let count: u64 = view(&mut test, draw, "entry_count", &organizer);
    assert_eq!(count, 5);

    // Paid entry into a free draw is rejected with guidance.
    let reason = test.execute_expect_failure(
        test.transaction()
            .call_method(entrants[0].address, "withdraw", args![TARI_TOKEN, 1])
            .put_last_instruction_output_on_workspace("ticket")
            .call_method(draw, "enter", args![Workspace("ticket")])
            .build_and_seal(&entrants[0].secret),
        vec![entrants[0].proof.clone()],
    );
    assert_reject_reason(reason, "This draw is free to enter; use enter_free");

    let hash = test_epoch_hash();
    set_epoch(&mut test, DRAW_EPOCH);
    set_epoch_hash(&mut test, hash);
    settle(&mut test, draw, &organizer);

    let expected_index = expected_winner_index(&hash, 5);
    let winner: Option<RistrettoPublicKeyBytes> = view(&mut test, draw, "winner", &organizer);
    assert_eq!(winner, Some(entrants[expected_index as usize].pubkey));

    let winner_acct = &entrants[expected_index as usize];
    claim_and_deposit(&mut test, draw, winner_acct);
    assert_eq!(
        balance(&test, winner_acct),
        Amount::from(INITIAL + PRIZE),
        "free-draw winner must receive exactly the sponsor prize"
    );
}
