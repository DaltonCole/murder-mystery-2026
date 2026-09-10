use crate::player::{Faction, PlayerId};
use serde::{Deserialize, Serialize};

/// A Cast-Out player's private Gallery prediction (rules.md §7: "who gets
/// Cast Out, or which faction ultimately wins"), submitted before the Last
/// Denouncement's ballot closes and scored once the real outcome is known
/// -- see `state::resolve_gallery_predictions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GalleryPrediction {
    CastOutIs(PlayerId),
    FactionWins(Faction),
}
