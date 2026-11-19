/*
 * Parity test: web simulation vs the Rust template.
 *
 * The expected values below are NOT computed here. They were produced by
 * running the template's winner formula on the Rust side and are pinned as
 * constants in `tests/parity_vectors.rs` (run `cargo test --test
 * parity_vectors -- --nocapture` there to see them derived). This script
 * asserts that `sim.js` reproduces every one of them. If this test and the
 * Rust test ever disagree, one side changed the formula.
 *
 * Run: node web/parity-test.mjs
 */

import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const Sim = require("./sim.js");

let failures = 0;
function check(label, actual, expected) {
  const ok = actual === expected;
  if (!ok) failures++;
  console.log(`${ok ? "PASS" : "FAIL"} ${label}: got ${actual}, expected ${expected}`);
}
function checkThrows(label, fn, expectedMessage) {
  try {
    fn();
    failures++;
    console.log(`FAIL ${label}: did not throw, expected "${expectedMessage}"`);
  } catch (err) {
    check(label, err.message, expectedMessage);
  }
}

// --- Winner-index vectors (expected values from tests/parity_vectors.rs) ---

const DRAW_RS_HASH = "0b30557a9fc4e90e33587da2c7ec11365b80a5caef14395e83a8cdf2173c6186";
const FIXED_HASH = "0762bd1873ce2984df3a95f04ba6015cb7126dc8237ed9348fea45a0fb56b10c";
const SEED_ONE_HASH = "0100000000000000" + "ab".repeat(24);

const vectors = [
  ["draw_rs_hash_x3", DRAW_RS_HASH, 3, 1],
  ["draw_rs_hash_x5", DRAW_RS_HASH, 5, 3],
  ["draw_rs_hash_x1", DRAW_RS_HASH, 1, 0],
  ["zero_hash_x5", "00".repeat(32), 5, 0],
  ["ff_hash_x7", "ff".repeat(32), 7, 1],
  ["ff_hash_x10", "ff".repeat(32), 10, 5],
  ["seed_one_x100", SEED_ONE_HASH, 100, 1],
  ["fixed_hash_x2", FIXED_HASH, 2, 1],
  ["fixed_hash_x7", FIXED_HASH, 7, 2],
  ["fixed_hash_x100", FIXED_HASH, 100, 27],
];
for (const [name, hash, count, expected] of vectors) {
  check(name, Sim.winnerIndexFromHash(hash, count), expected);
}

// Seeds (the little-endian u64 itself) — catches endianness mistakes that a
// modulo could hide.
check("seed_draw_rs", Sim.seedFromHash(DRAW_RS_HASH), 1074606175335821323n);
check("seed_ff", Sim.seedFromHash("ff".repeat(32)), 18446744073709551615n);
check("seed_fixed", Sim.seedFromHash(FIXED_HASH), 9523369880783249927n);

// Hash normalization: 0x prefix and uppercase must not change the result.
check(
  "normalize_0x_upper",
  Sim.winnerIndexFromHash("0x" + DRAW_RS_HASH.toUpperCase(), 5),
  3
);

// --- Rule parity: the sim enforces the template's rejections ---------------

// Free draw, 5 entries: duplicate entry rejected, early draw rejected,
// non-winner claim rejected, winner claims exactly the prize.
{
  const s = Sim.createDraw({
    organizer: "organizer",
    prize: 50000,
    ticketPrice: 0,
    closeEpoch: 10,
    drawEpoch: 20,
    seed: "parity-flow",
  });
  for (const name of ["alice", "bob", "carol", "dave", "erin"]) Sim.enterFree(s, name);
  checkThrows("double_entry", () => Sim.enterFree(s, "alice"), "This account has already entered the draw");
  checkThrows("paid_into_free", () => Sim.enter(s, "alice"), "This draw is free to enter; use enter_free");
  checkThrows("early_draw", () => Sim.draw(s), "The draw epoch has not been reached");
  Sim.setEpoch(s, 20);
  const result = Sim.draw(s);
  check("flow_winner_is_entrant", s.entries.includes(result.winner), true);
  const loser = s.entries.find((e) => e !== result.winner);
  checkThrows("claim_by_loser", () => Sim.claim(s, loser), "Only the winner can claim the prize");
  const before = Sim.balanceOf(s, result.winner);
  check("claim_amount", Sim.claim(s, result.winner), 50000);
  check("winner_balance", Sim.balanceOf(s, result.winner), before + 50000);
  checkThrows("second_claim", () => Sim.claim(s, result.winner), "The prize has already been claimed");
}

// Paid draw: pool is prize + tickets; entries close at close_epoch.
{
  const s = Sim.createDraw({
    organizer: "organizer",
    prize: 50000,
    ticketPrice: 1000,
    closeEpoch: 10,
    drawEpoch: 20,
    seed: "parity-paid",
  });
  Sim.enter(s, "alice");
  Sim.enter(s, "bob");
  Sim.enter(s, "carol");
  check("paid_pool", s.pool, 53000);
  Sim.setEpoch(s, 10);
  checkThrows("entry_after_close", () => Sim.enter(s, "dave"), "Entries are closed");
}

// Empty draw: draw() rejected; only the organizer can refund, in full.
{
  const s = Sim.createDraw({
    organizer: "organizer",
    prize: 50000,
    ticketPrice: 0,
    closeEpoch: 10,
    drawEpoch: 20,
    seed: "parity-refund",
  });
  Sim.setEpoch(s, 20);
  checkThrows("draw_empty", () => Sim.draw(s), "Cannot draw with no entries; the organizer may refund instead");
  checkThrows("refund_by_stranger", () => Sim.refundAll(s, "mallory"), "Only the organizer can refund an empty draw");
  check("refund_amount", Sim.refundAll(s, "organizer"), 50000);
  check("organizer_balance_restored", Sim.balanceOf(s, "organizer"), Sim.START_BALANCE);
}

console.log(failures === 0 ? "\nALL PARITY CHECKS PASSED" : `\n${failures} CHECK(S) FAILED`);
process.exit(failures === 0 ? 0 : 1);
