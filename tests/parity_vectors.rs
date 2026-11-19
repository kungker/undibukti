//! Parity vectors for the web simulation.
//!
//! The JavaScript simulation in `docs/sim.js` must pick exactly the same
//! winner as the Rust template for the same epoch hash and entry count.
//! This test is the source of truth for the expected values: it recomputes
//! the winner index with the template's documented formula
//! (`u64::from_le_bytes(epoch_hash.leading_bytes::<8>()) % entry_count`)
//! over a fixed set of (epoch hash, entry count) vectors and asserts the
//! results. `docs/parity-test.mjs` asserts the same constants from the
//! JavaScript side; if the two ever disagree, one of them changed the
//! formula.
//!
//! The first vectors reuse the exact epoch hash injected by the engine
//! tests in `tests/draw.rs` (`test_epoch_hash()`), with the same entry
//! counts those tests settle with (3 paid entries, 5 free entries).

use tari_template_test_tooling::template_lib_types::Hash32;

/// The exact hash injected by `tests/draw.rs::test_epoch_hash()`.
fn draw_rs_test_hash() -> Hash32 {
    let mut bytes = [0u8; 32];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = (i as u8).wrapping_mul(37).wrapping_add(11);
    }
    Hash32::from_array(bytes)
}

/// A second fixed, arbitrary hash (no special structure).
fn fixed_hash() -> Hash32 {
    let mut bytes = [0u8; 32];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = (i as u8).wrapping_mul(91).wrapping_add(7);
    }
    Hash32::from_array(bytes)
}

/// Seed exactly 1: first byte 0x01, the rest of the leading 8 bytes zero.
fn seed_one_hash() -> Hash32 {
    let mut bytes = [0xabu8; 32];
    bytes[0] = 0x01;
    for b in bytes.iter_mut().take(8).skip(1) {
        *b = 0;
    }
    Hash32::from_array(bytes)
}

fn winner_index(epoch_hash: &Hash32, entry_count: u64) -> u64 {
    u64::from_le_bytes(epoch_hash.leading_bytes::<8>()) % entry_count
}

fn to_hex(hash: &Hash32) -> String {
    hash.as_slice().iter().map(|b| format!("{b:02x}")).collect()
}

/// (name, epoch hash, entry count, expected winner index).
/// The expected indices are pinned constants: they were produced by running
/// this formula on the Rust side and are asserted identically by
/// `docs/parity-test.mjs`.
fn vectors() -> Vec<(&'static str, Hash32, u64, u64)> {
    vec![
        // Same hash and entry counts as the engine tests in tests/draw.rs.
        ("draw_rs_hash_x3", draw_rs_test_hash(), 3, EXPECTED_DRAW_RS_X3),
        ("draw_rs_hash_x5", draw_rs_test_hash(), 5, EXPECTED_DRAW_RS_X5),
        ("draw_rs_hash_x1", draw_rs_test_hash(), 1, 0),
        // Degenerate hashes.
        ("zero_hash_x5", Hash32::from_array([0u8; 32]), 5, 0),
        ("ff_hash_x7", Hash32::from_array([0xffu8; 32]), 7, EXPECTED_FF_X7),
        ("ff_hash_x10", Hash32::from_array([0xffu8; 32]), 10, EXPECTED_FF_X10),
        // Seed exactly 1: 1 % 100 == 1.
        ("seed_one_x100", seed_one_hash(), 100, 1),
        // Arbitrary fixed hash at several counts.
        ("fixed_hash_x2", fixed_hash(), 2, EXPECTED_FIXED_X2),
        ("fixed_hash_x7", fixed_hash(), 7, EXPECTED_FIXED_X7),
        ("fixed_hash_x100", fixed_hash(), 100, EXPECTED_FIXED_X100),
    ]
}

const EXPECTED_DRAW_RS_X3: u64 = 1;
const EXPECTED_DRAW_RS_X5: u64 = 3;
const EXPECTED_FF_X7: u64 = 1;
const EXPECTED_FF_X10: u64 = 5;
const EXPECTED_FIXED_X2: u64 = 1;
const EXPECTED_FIXED_X7: u64 = 2;
const EXPECTED_FIXED_X100: u64 = 27;

#[test]
fn parity_vectors_match_template_formula() {
    let rows: Vec<(&str, u64, u64)> = vectors()
        .iter()
        .map(|(name, hash, count, expected)| {
            let actual = winner_index(hash, *count);
            println!(
                "VECTOR {name} hash={} count={count} seed={} index={actual}",
                to_hex(hash),
                u64::from_le_bytes(hash.leading_bytes::<8>())
            );
            (*name, actual, *expected)
        })
        .collect();
    for (name, actual, expected) in rows {
        assert_eq!(actual, expected, "parity vector {name} changed");
    }
}
