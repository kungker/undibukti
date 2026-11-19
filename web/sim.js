"use strict";

/*
 * UndiBukti — walletless simulation of the Tari Ootle `Draw` template.
 *
 * This module mirrors the template in `src/lib.rs` rule for rule, including
 * its rejection messages, so the web demo behaves the way the on-chain
 * component would. It is a simulation: accounts are plain names standing in
 * for signer public keys, balances are a demo ledger, and the "epoch hash"
 * is a deterministic stand-in (see `epochHashFor`). In the real template the
 * epoch hash is the consensus hash of the current epoch; everything else —
 * entry rules, settlement, the winner formula — is the same logic.
 *
 * The winner formula, identical to the template's
 * `winner_index_from_hash`:
 *
 *   winner_index = u64_from_le_bytes(first 8 bytes of epoch hash) % entry_count
 *
 * `web/parity-test.mjs` checks this implementation against fixed vectors
 * whose expected values come from the Rust side (`tests/parity_vectors.rs`).
 */

const UndiBuktiSim = (() => {
  /** Demo starting balance for every account that shows up. */
  const START_BALANCE = 100000;

  function fail(message) {
    throw new Error(message);
  }

  /** Normalize an epoch hash: optional 0x prefix, any case -> 64 lowercase hex chars. */
  function normalizeHashHex(hashHex) {
    if (typeof hashHex !== "string") fail("Epoch hash must be a hex string");
    let h = hashHex.trim().toLowerCase();
    if (h.startsWith("0x")) h = h.slice(2);
    if (!/^[0-9a-f]{64}$/.test(h)) {
      fail("Epoch hash must be 32 bytes (64 hex characters)");
    }
    return h;
  }

  function hashBytes(hashHex) {
    const h = normalizeHashHex(hashHex);
    const bytes = new Uint8Array(32);
    for (let i = 0; i < 32; i++) {
      bytes[i] = parseInt(h.slice(i * 2, i * 2 + 2), 16);
    }
    return bytes;
  }

  /** The seed: first 8 bytes of the hash as a little-endian u64 (BigInt). */
  function seedFromHash(hashHex) {
    const bytes = hashBytes(hashHex);
    let seed = 0n;
    for (let i = 7; i >= 0; i--) {
      seed = (seed << 8n) | BigInt(bytes[i]);
    }
    return seed;
  }

  /**
   * The winner formula — byte-for-byte the template's:
   * `u64::from_le_bytes(epoch_hash.leading_bytes::<8>()) % entry_count`.
   */
  function winnerIndexFromHash(hashHex, entryCount) {
    if (!Number.isInteger(entryCount) || entryCount <= 0) {
      fail("Cannot compute a winner with no entries");
    }
    return Number(seedFromHash(hashHex) % BigInt(entryCount));
  }

  /*
   * Simulation-only stand-in for the consensus epoch hash.
   *
   * On-chain, `Consensus::current_epoch_hash()` is fixed for the whole epoch
   * and unpredictable before the epoch begins. The demo needs the same two
   * properties — one stable 32-byte value per (draw, epoch) — so it derives
   * one deterministically from the draw's seed string and the epoch number
   * with a small PRNG. This is NOT a consensus value and proves nothing by
   * itself; the verifiable part is the winner formula above, which the
   * verification page recomputes from whatever hash is pasted into it.
   */
  function epochHashFor(drawSeed, epoch) {
    let h = 2166136261 >>> 0; // FNV-1a over `${drawSeed}:${epoch}`
    const s = `${drawSeed}:${epoch}`;
    for (let i = 0; i < s.length; i++) {
      h ^= s.charCodeAt(i);
      h = Math.imul(h, 16777619);
    }
    let state = h >>> 0;
    const next = () => {
      // mulberry32
      state = (state + 0x6d2b79f5) >>> 0;
      let t = state;
      t = Math.imul(t ^ (t >>> 15), t | 1);
      t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
    let hex = "";
    for (let i = 0; i < 32; i++) {
      hex += Math.floor(next() * 256)
        .toString(16)
        .padStart(2, "0");
    }
    return hex;
  }

  /**
   * Create a draw. Mirrors `Draw::new`: the creator becomes the organizer and
   * funds the prize pool; `drawEpoch` must be after `closeEpoch`.
   * `ticketPrice` 0 means a free draw.
   */
  function createDraw({ organizer, prize, ticketPrice, closeEpoch, drawEpoch, seed }) {
    if (!(drawEpoch > closeEpoch)) {
      fail("The draw epoch must be after the close epoch");
    }
    const state = {
      organizer,
      prize,
      ticketPrice,
      closeEpoch,
      drawEpoch,
      entries: [],
      pool: 0,
      winnerIndex: null,
      winner: null,
      drawn: false,
      paid: false,
      refunded: false,
      currentEpoch: 0,
      epochHash: null, // hash of the epoch in which draw() settled
      seed: seed ?? `draw-${Date.now()}`,
      balances: new Map(),
    };
    state.balances.set(organizer, START_BALANCE);
    if (balanceOf(state, organizer) < prize) fail("Insufficient balance for the prize");
    state.balances.set(organizer, balanceOf(state, organizer) - prize);
    state.pool = prize;
    return state;
  }

  function balanceOf(state, account) {
    if (!state.balances.has(account)) state.balances.set(account, START_BALANCE);
    return state.balances.get(account);
  }

  /** Shared entry bookkeeping — mirrors `register_entry` in the template. */
  function registerEntry(state, account) {
    if (state.drawn) fail("The draw has already been settled");
    if (!(state.currentEpoch < state.closeEpoch)) fail("Entries are closed");
    if (state.entries.includes(account)) {
      fail("This account has already entered the draw");
    }
    state.entries.push(account);
  }

  /** Paid entry — mirrors `enter`. The ticket goes into the prize pool. */
  function enter(state, account) {
    if (!(state.ticketPrice > 0)) fail("This draw is free to enter; use enter_free");
    if (balanceOf(state, account) < state.ticketPrice) {
      fail("Insufficient balance for the ticket price");
    }
    registerEntry(state, account);
    state.balances.set(account, balanceOf(state, account) - state.ticketPrice);
    state.pool += state.ticketPrice;
  }

  /** Free entry — mirrors `enter_free`. */
  function enterFree(state, account) {
    if (state.ticketPrice !== 0) fail("This draw requires a ticket; use enter");
    registerEntry(state, account);
  }

  /** Advance the simulated clock. Epochs only move forward. */
  function setEpoch(state, epoch) {
    if (epoch < state.currentEpoch) fail("Epochs cannot move backwards");
    state.currentEpoch = epoch;
  }

  /**
   * Settle the draw — mirrors `draw`. Uses the epoch hash of the *current*
   * epoch, exactly as the template reads `Consensus::current_epoch_hash()`
   * at call time.
   */
  function draw(state) {
    if (state.drawn) fail("The draw has already been settled");
    if (!(state.currentEpoch >= state.drawEpoch)) {
      fail("The draw epoch has not been reached");
    }
    if (state.entries.length === 0) {
      fail("Cannot draw with no entries; the organizer may refund instead");
    }
    state.epochHash = epochHashFor(state.seed, state.currentEpoch);
    state.winnerIndex = winnerIndexFromHash(state.epochHash, state.entries.length);
    state.winner = state.entries[state.winnerIndex];
    state.drawn = true;
    return {
      epochHash: state.epochHash,
      seed: seedFromHash(state.epochHash),
      winnerIndex: state.winnerIndex,
      winner: state.winner,
    };
  }

  /** Claim the prize — mirrors `claim`. Only the winner may call it. */
  function claim(state, account) {
    if (!state.drawn) fail("The draw has not been settled yet");
    if (state.paid) fail("The prize has already been claimed");
    if (state.winner !== account) fail("Only the winner can claim the prize");
    state.paid = true;
    const amount = state.pool;
    state.pool = 0;
    state.balances.set(account, balanceOf(state, account) + amount);
    return amount;
  }

  /** Refund an empty draw — mirrors `refund_all`. Only the organizer. */
  function refundAll(state, account) {
    if (state.entries.length !== 0) {
      fail("Refunds are only available when the draw has no entries");
    }
    if (!(state.currentEpoch >= state.drawEpoch)) {
      fail("The draw epoch has not been reached");
    }
    if (state.refunded) fail("The prize has already been refunded");
    if (account !== state.organizer) {
      fail("Only the organizer can refund an empty draw");
    }
    state.refunded = true;
    const amount = state.pool;
    state.pool = 0;
    state.balances.set(account, balanceOf(state, account) + amount);
    return amount;
  }

  /**
   * Standalone recomputation for the verification page: given the public
   * entry list (in entry order) and an epoch hash, recompute the winner.
   */
  function verifyWinner(entries, hashHex) {
    if (!Array.isArray(entries) || entries.length === 0) {
      fail("Entry list is empty");
    }
    const winnerIndex = winnerIndexFromHash(hashHex, entries.length);
    return {
      seed: seedFromHash(hashHex),
      winnerIndex,
      winner: entries[winnerIndex],
    };
  }

  return {
    START_BALANCE,
    normalizeHashHex,
    seedFromHash,
    winnerIndexFromHash,
    epochHashFor,
    createDraw,
    balanceOf,
    enter,
    enterFree,
    setEpoch,
    draw,
    claim,
    refundAll,
    verifyWinner,
  };
})();

if (typeof module !== "undefined" && module.exports) {
  module.exports = UndiBuktiSim;
}
if (typeof window !== "undefined") {
  window.UndiBuktiSim = UndiBuktiSim;
}
