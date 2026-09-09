use serde::{Deserialize, Serialize};

/// A player's fixed identity for the game. Distinct from [`crate::Faction`]:
/// faction is the side a player *appears* to be on and can be secretly
/// undermined by conversion (rules.md §3.3), while `Character` is the label
/// that "stays with" a player even through conversion --
/// "a converted player keeps their original character and abilities" is a
/// direct quote from rules.md, and this type exists specifically so that
/// invariant has somewhere to live.
///
/// `KingQueen`, `PrincePrincess`, `RevolutionaryLeader`, and `CultLeader` are
/// *titles* more than fixed identities in one respect: `KingQueen` and
/// `RevolutionaryLeader` can move to a different player over the course of
/// the game (voluntary transfer, the conversion cascade, the Round 3
/// Cast-Out cascade, succession) -- see `GameState`'s title-tracking fields
/// and `resolve_cast_out`/`convert` in `state.rs`. When a title moves off a
/// player, they don't keep the label; see those functions' doc comments for
/// exactly what they become instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Character {
    KingQueen,
    PrincePrincess,
    RevolutionaryLeader,
    CultLeader,
    /// The full 23-character roster (Magistrate, Oracle, Bartender, ...)
    /// lands in Phase 2/3 per the implementation plan -- everyone on the
    /// Ton side without one of the named roles above is this catch-all for
    /// now, matching rules.md's own "Normal Ton member" role.
    NormalTon,
    NormalUprising,
    /// A secretly-Cult-aligned player who isn't the Cult Leader and doesn't
    /// hold a converted title (KingQueen/RevolutionaryLeader/PrincePrincess
    /// while converted) -- always implies `Player::converted == true`. See
    /// `Player::is_consistent` for the invariant this maintains.
    Cultist,
}

/// Whether a player is still part of the live game. `CastOut` covers both
/// "correctly Denounced" and "innocent bystander Denounced" -- rules.md is
/// explicit that both look identical to everyone but the Cast-Out player
/// themselves (§6, "every result gets the same deliberately uninformative
/// public treatment"), so this engine doesn't distinguish them at the type
/// level either. A Cast-Out player remains a member of their true faction
/// for win/loss purposes (rules.md's answer to "a cast out player is still
/// part of the faction that they were cast out in") -- `status` only gates
/// whether they can still *act*, never whether their faction's win counts
/// for them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayerStatus {
    Active,
    CastOut,
}
