# UndiBukti — a verifiable community draw for Tari Ootle

UndiBukti is a Tari Ootle template for running community draws, raffles, and
giveaways whose result anyone can verify. The organizer funds a prize pool and
opens entries; when the draw settles, the winner is derived deterministically
from the consensus epoch hash of the draw epoch and the public entry list.
The organizer cannot pick the winner after entries close, and nobody has to
trust the organizer's say-so: the entry list and the epoch hash are public,
so anyone can recompute the winner off-chain and check it against the
component's recorded result.

## How it works

1. **Create.** `Draw::new(prize_bucket, ticket_price, close_epoch, draw_epoch)`
   creates a draw. The creator (the transaction signer) becomes the organizer
   and funds the prize pool. `draw_epoch` must be after `close_epoch`.
   A `ticket_price` of zero makes the draw free to enter.
2. **Enter.** While `current_epoch < close_epoch`, accounts enter with
   `enter(ticket_bucket)` (paid draws: the bucket must contain exactly the
   ticket price, which joins the prize pool) or `enter_free()` (free draws).
   One entry per account, enforced from the transaction signer's public key —
   never from an address passed as an argument — so entries cannot be forged
   for someone else, and a second entry from the same account is rejected.
3. **Draw.** Once `current_epoch >= draw_epoch`, anyone may call `draw()`.
   The winner index is computed from the current epoch hash:

   ```text
   winner_index = u64::from_le_bytes(epoch_hash[0..8]) mod entry_count
   ```

   That is: take the first 8 bytes of the 32-byte epoch hash as a
   little-endian `u64`, reduce it modulo the number of entries, and count
   into the entry list in entry order.
4. **Claim.** Only the winner — verified from the transaction signer — can
   call `claim()`, which pays out the entire pool (sponsor prize plus all
   ticket payments). Nobody else, including the organizer, can take the pool.
5. **Refund.** If the draw ends with zero entries, `draw()` is rejected and
   the organizer — only the organizer, and only at or after `draw_epoch` —
   can call `refund_all()` to recover the prize in full.

The component has no owner and every method is callable by anyone; the
authorization that matters (winner-only claim, organizer-only refund) is
enforced inside the methods from the signer's public key. Nobody, including
the organizer, can change the rules after creation.

## Verify it yourself

**Template (source of truth).** Clone this repository and run the engine
tests — they execute the template against the real Tari engine in-process:

```sh
cargo test
```

Nine tests cover the full flow and the rejection list: double entry, entry
after close, draw before the draw epoch, draw with no entries, wrong ticket
amount, claim by a non-winner, second claim, refund of an empty draw, and —
most importantly — that the settled winner equals an independent off-chain
recomputation from the injected epoch hash, with the exact pool paid out.
A tenth test file, `tests/parity_vectors.rs`, pins winner-index vectors that
the web simulation is checked against.

**Web MVP.** Open `docs/index.html` (no build step; it also works from
GitHub Pages). It runs a two-minute demo — create a draw, add five entries,
jump epochs, draw, claim — and includes a verification section: paste an
entry list and an epoch hash, and the page recomputes the winner in your
browser with the formula above. The simulation's rules mirror the template
(including its rejection messages), and `node docs/parity-test.mjs` verifies
the simulation against the Rust-pinned vectors.

## Honest limits

- **The web MVP is a walletless simulation.** It runs the same rules and the
  same winner formula as the template, off-chain, in your browser. It is not
  connected to a live wallet or network, and no real funds move.
- **The template is not published** to the Esmeralda testnet or mainnet.
  What is submitted is the source code, its engine tests, and the simulation.
- **The epoch hash is a consensus value, not a randomness beacon.** It is
  fixed for the whole epoch and unpredictable before the epoch begins. The
  claim UndiBukti makes is narrow: *the organizer cannot choose the winner
  after entries close.* It is not a claim of perfect randomness — for
  example, a party able to influence consensus could in principle grind on
  the epoch hash, and everyone can compute the winner as soon as the draw
  epoch starts.
- **Privacy is thin by design.** Entries are public (that is what makes the
  draw verifiable), so entrants are visible. UndiBukti's strength is
  verifiability, not privacy.

## Layout

- `src/lib.rs` — the `Draw` template.
- `tests/draw.rs` — engine tests (full flows + rejections).
- `tests/parity_vectors.rs` — pinned winner-index vectors from the Rust formula.
- `docs/` — the walletless web MVP (`index.html`, `sim.js`, `app.js`) and its
  parity test (`parity-test.mjs`).

## License

MIT — see [LICENSE](LICENSE). Copyright (c) 2026 biggals.
