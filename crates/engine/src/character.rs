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

    // --- Phase 2: simple/independent abilities (rules.md §3.1-3.3) ---
    /// Ton, info-check family: after every odd round, views a player's full
    /// history. Permanently disabled if the King/Queen is Cast Out
    /// unconverted (rules.md §5) -- see `GameState::oracle_disabled`.
    Oracle,
    /// Ton, info-check family: once per game, learns 3 players who are
    /// definitely not the Revolutionary Leader.
    Almanac,
    /// Uprising, info-check family: once per game, views one player's
    /// apparent faction only.
    Spymaster,
    /// Ton, protect family: once per Cult recruitment window, protects one
    /// person from conversion.
    PriestPriestess,
    /// Ton, protect family: once per game, grants round-wide
    /// execution-immunity.
    PotionMaker,
    /// Ton, vote-weight: once per game, their ballot counts as two votes.
    Magistrate,
    /// Uprising, protect family: once per round, makes a target drunk with
    /// 50% odds (can't nominate/vote that round if it lands).
    Bartender,
    /// Uprising, protect family: once per round, protects one person from
    /// this round's Cast-Out resolution; can't repeat the same target on
    /// consecutive rounds.
    DoctorMedic,
    /// Uprising, vote-weight: once per game, their ballot counts as two
    /// votes -- the Magistrate's mirror.
    Firebrand,
    /// Uprising, passive-knowledge: knows 2 other Uprising members (never
    /// the Leader) -- see `GameState::cell_leader_knows`.
    CellLeader,
    /// Cult, falsify pipeline: once per game (if armed), forces an
    /// info-check that targets them to return a false result.
    Deceiver,
    // Deliberately no `Whisperer` variant yet, even though rules.md §3.3
    // pairs it with `Deceiver` as the other Cult Leader-designated title
    // ("may shield one named fellow Cultist from being a valid nomination
    // target for one round"). Its ability is a Denouncement-*procedure*
    // modifier -- shaped exactly like the Duelist's "guarantee a ballot
    // spot" and the Agitator's "redirect discussion," both explicitly
    // Phase 3 scope (`ProcedureEffect` in the implementation plan) -- so
    // it belongs with that batch, not this one, despite rules.md grouping
    // it with the Cult section.
    /// The full 23-character roster lands across Phase 2/3 per the
    /// implementation plan -- everyone on the Ton side without one of the
    /// named roles above is this catch-all for now, matching rules.md's own
    /// "Normal Ton member" role.
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
