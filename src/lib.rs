use tari_template_lib::prelude::*;

#[template]
mod draw {
    use super::*;

    /// UndiBukti — a verifiable community draw.
    ///
    /// The organizer funds a prize pool and opens entries. Each account may enter
    /// at most once; the entrant's identity is the transaction signer's public
    /// key (from the signer badge in scope), never an address passed as an
    /// argument, so entries cannot be forged for someone else.
    ///
    /// Once entries close, anyone may call `draw()` at or after `draw_epoch`.
    /// The winner index is derived from the consensus epoch hash of that epoch:
    ///
    /// ```text
    /// winner_index = u64::from_le_bytes(epoch_hash.leading_bytes::<8>()) % entry_count
    /// ```
    ///
    /// The epoch hash is fixed for the whole epoch and unpredictable before the
    /// epoch begins, so the organizer cannot choose the winner after entries
    /// close. Anyone can recompute the winner off-chain from the public entry
    /// list (`entries()`) and the epoch hash. Note the honest limit: this proves
    /// the organizer could not pick the winner — it is not a claim of perfect
    /// randomness.
    ///
    /// Access design: every method is callable by anyone
    /// (`AccessRules::allow_all`) and the component has no owner
    /// (`OwnerRule::None`), so nobody — including the organizer — can change
    /// the rules after creation. Authorization that matters (only the winner
    /// can claim, only the organizer can refund an empty draw) is enforced
    /// inside the methods from the transaction signer's public key.
    pub struct Draw {
        /// Public key of the account that created (and funded) this draw.
        organizer: RistrettoPublicKeyBytes,
        /// Prize pool. Ticket payments (paid draws) are deposited here too.
        prize_vault: Vault,
        /// Resource of the prize pool; tickets must be paid in this resource.
        prize_resource: ResourceAddress,
        /// Ticket price. Zero means the draw is free to enter (use `enter_free`).
        ticket_price: Amount,
        /// Entries are accepted while `current_epoch < close_epoch`.
        close_epoch: u64,
        /// The draw may be settled when `current_epoch >= draw_epoch`.
        draw_epoch: u64,
        /// Entrant public keys, in entry order. This order is part of the
        /// verifiable record: the winner index counts into this list.
        entries: Vec<RistrettoPublicKeyBytes>,
        /// Index into `entries` of the winner, set by `draw()`.
        winner_index: Option<u64>,
        /// The winning entrant, set by `draw()`.
        winner: Option<RistrettoPublicKeyBytes>,
        /// Whether `draw()` has settled the draw.
        drawn: bool,
        /// Whether the winner has claimed the prize.
        paid: bool,
        /// Whether the organizer has refunded an empty draw.
        refunded: bool,
    }

    impl Draw {
        /// Create a new draw, funding the prize pool with `prize_bucket`.
        ///
        /// The creator (transaction signer) becomes the organizer. Tickets are
        /// paid in the same resource as the prize bucket.
        pub fn new(
            prize_bucket: Bucket,
            ticket_price: Amount,
            close_epoch: u64,
            draw_epoch: u64,
        ) -> Component<Self> {
            assert!(
                draw_epoch > close_epoch,
                "The draw epoch must be after the close epoch"
            );
            let prize_resource = prize_bucket.resource_address();
            let organizer = CallerContext::transaction_signer_public_key();
            Component::new(Self {
                organizer,
                prize_vault: Vault::from_bucket(prize_bucket),
                prize_resource,
                ticket_price,
                close_epoch,
                draw_epoch,
                entries: Vec::new(),
                winner_index: None,
                winner: None,
                drawn: false,
                paid: false,
                refunded: false,
            })
            .with_access_rules(AccessRules::allow_all())
            .with_owner_rule(OwnerRule::None)
            .create()
        }

        /// Enter the draw by paying exactly one ticket.
        ///
        /// Only for paid draws (`ticket_price > 0`). The bucket must contain
        /// exactly `ticket_price` of the prize resource; it is added to the
        /// prize pool.
        pub fn enter(&mut self, ticket_bucket: Bucket) {
            assert!(
                self.ticket_price > Amount::ZERO,
                "This draw is free to enter; use enter_free"
            );
            assert!(
                ticket_bucket.resource_address() == self.prize_resource,
                "Ticket bucket has the wrong resource"
            );
            assert!(
                ticket_bucket.amount() == self.ticket_price,
                "Ticket bucket must contain exactly the ticket price"
            );
            let signer = CallerContext::transaction_signer_public_key();
            self.register_entry(signer);
            self.prize_vault.deposit(ticket_bucket);
        }

        /// Enter a free draw (`ticket_price == 0`). No payment is taken.
        pub fn enter_free(&mut self) {
            assert!(
                self.ticket_price == Amount::ZERO,
                "This draw requires a ticket; use enter"
            );
            let signer = CallerContext::transaction_signer_public_key();
            self.register_entry(signer);
        }

        /// Shared entry bookkeeping: enforces the entry window and the
        /// one-entry-per-account rule, keyed on the signer of the transaction.
        fn register_entry(&mut self, signer: RistrettoPublicKeyBytes) {
            assert!(!self.drawn, "The draw has already been settled");
            assert!(
                Consensus::current_epoch() < self.close_epoch,
                "Entries are closed"
            );
            assert!(
                !self.entries.contains(&signer),
                "This account has already entered the draw"
            );
            self.entries.push(signer);
        }

        /// Settle the draw. May be called by anyone once `draw_epoch` is
        /// reached. The winner is derived deterministically from the current
        /// epoch hash and the entry list, as documented on the component.
        pub fn draw(&mut self) {
            assert!(!self.drawn, "The draw has already been settled");
            assert!(
                Consensus::current_epoch() >= self.draw_epoch,
                "The draw epoch has not been reached"
            );
            assert!(
                !self.entries.is_empty(),
                "Cannot draw with no entries; the organizer may refund instead"
            );
            let epoch_hash = Consensus::current_epoch_hash();
            let winner_index = winner_index_from_hash(&epoch_hash, self.entries.len() as u64);
            self.winner_index = Some(winner_index);
            self.winner = Some(self.entries[winner_index as usize]);
            self.drawn = true;
        }

        /// Claim the prize. Only the winner (verified from the transaction
        /// signer) may call this; it returns the entire prize pool as a bucket,
        /// which the caller's transaction deposits into the winner's account.
        /// No one else — including the organizer — can take the pool.
        pub fn claim(&mut self) -> Bucket {
            assert!(self.drawn, "The draw has not been settled yet");
            assert!(!self.paid, "The prize has already been claimed");
            let signer = CallerContext::transaction_signer_public_key();
            assert!(
                self.winner == Some(signer),
                "Only the winner can claim the prize"
            );
            self.paid = true;
            let balance = self.prize_vault.balance();
            self.prize_vault.withdraw(balance)
        }

        /// Refund the prize pool to the organizer when the draw ends with no
        /// entries. Only the organizer may call this, and only at or after
        /// `draw_epoch` (before that, entries could still arrive).
        pub fn refund_all(&mut self) -> Bucket {
            assert!(
                self.entries.is_empty(),
                "Refunds are only available when the draw has no entries"
            );
            assert!(
                Consensus::current_epoch() >= self.draw_epoch,
                "The draw epoch has not been reached"
            );
            assert!(!self.refunded, "The prize has already been refunded");
            let signer = CallerContext::transaction_signer_public_key();
            assert!(
                signer == self.organizer,
                "Only the organizer can refund an empty draw"
            );
            self.refunded = true;
            let balance = self.prize_vault.balance();
            self.prize_vault.withdraw(balance)
        }

        /// The public entry list, in entry order.
        pub fn entries(&self) -> Vec<RistrettoPublicKeyBytes> {
            self.entries.clone()
        }

        /// Number of entries so far.
        pub fn entry_count(&self) -> u64 {
            self.entries.len() as u64
        }

        /// The winner's public key, once the draw has been settled.
        pub fn winner(&self) -> Option<RistrettoPublicKeyBytes> {
            self.winner
        }

        /// The winner's index into `entries()`, once settled.
        pub fn winner_index(&self) -> Option<u64> {
            self.winner_index
        }

        /// Current prize pool balance (sponsor prize plus any ticket payments).
        pub fn prize_balance(&self) -> Amount {
            self.prize_vault.balance()
        }

        /// Whether the draw has been settled.
        pub fn is_drawn(&self) -> bool {
            self.drawn
        }
    }

    /// The single source of truth for how a winner is picked. The web
    /// verification page and the engine tests recompute exactly this function
    /// off-chain: take the first 8 bytes of the epoch hash as a little-endian
    /// u64 and reduce modulo the number of entries.
    fn winner_index_from_hash(epoch_hash: &Hash32, entry_count: u64) -> u64 {
        let seed = u64::from_le_bytes(epoch_hash.leading_bytes::<8>());
        seed % entry_count
    }
}
