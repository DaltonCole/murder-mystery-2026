use crate::character::PlayerStatus;
use crate::player::PlayerId;
use crate::state::GameState;
use crate::task::TaskTier;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// rules.md §1: "each capped at 32 characters."
pub const MAX_FIELD_LEN: usize = 32;

/// A player's character sheet (rules.md §1's "Character creation"),
/// submitted during setup. Free text throughout -- the engine validates
/// only the length cap rules.md states, nothing about content. Servants
/// submit one too ("Servants' bios feed into the shared task pool too"),
/// same shape as everyone else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bio {
    pub character_name: String,
    pub real_name: String,
    pub occupation: String,
    pub hobbies: [String; 5],
    pub clothing_features: [String; 5],
    pub skills: [String; 5],
}

impl Bio {
    /// Every free-text field, labeled for error messages -- the single
    /// source of truth `validate` and any future per-field logic walks,
    /// so a new field only ever needs adding here once.
    fn fields(&self) -> impl Iterator<Item = (&'static str, &str)> {
        [
            ("character_name", self.character_name.as_str()),
            ("real_name", self.real_name.as_str()),
            ("occupation", self.occupation.as_str()),
        ]
        .into_iter()
        .chain(self.hobbies.iter().map(|s| ("hobby", s.as_str())))
        .chain(
            self.clothing_features
                .iter()
                .map(|s| ("clothing feature", s.as_str())),
        )
        .chain(self.skills.iter().map(|s| ("skill", s.as_str())))
    }

    /// The first over-length field, if any -- `(label, actual_len)`.
    pub(crate) fn first_oversized_field(&self) -> Option<(&'static str, usize)> {
        self.fields()
            .find(|(_, value)| value.chars().count() > MAX_FIELD_LEN)
            .map(|(label, value)| (label, value.chars().count()))
    }
}

/// rules.md §1: "displayed in Pascal Case." Title-cases each word,
/// preserving spacing (e.g. "long flowing dress" -> "Long Flowing Dress")
/// -- not literal squashed PascalCase/no-spaces, which would mangle a
/// multi-word phrase into one unreadable word on a shared display. This is
/// the visually sensible reading of the rule for free-text party bios;
/// flagged here as a judgment call, not a rules.md quote.
pub fn pascal_case(s: &str) -> String {
    s.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// One not-yet-used, bio-derived task the Host can push as-is via
/// `Command::PushTask` -- this function only computes the pool; it doesn't
/// push anything itself, the same "engine computes candidates, the caller
/// picks and commits" shape `win_condition::evaluate` and `whistledown::posts`
/// already use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskCandidate {
    pub prompt: String,
    pub qualifying_players: Vec<PlayerId>,
}

enum Category {
    ClothingFeatures,
    Hobbies,
    Skills,
}

impl Category {
    /// rules.md §4: "Task phase, easy/medium/hard tiers live" for Rounds
    /// 3/5 -- rules.md gives no explicit tier-to-category mapping, so this
    /// is a judgment call, not a quote: clothing features are visually
    /// apparent across a room (Easy), hobbies need a real conversation to
    /// surface (Medium), and skills need the deepest conversation of the
    /// three to actually verify (Hard).
    fn for_tier(tier: TaskTier) -> Self {
        match tier {
            TaskTier::Easy => Category::ClothingFeatures,
            TaskTier::Medium => Category::Hobbies,
            TaskTier::Hard => Category::Skills,
        }
    }

    fn values<'a>(&self, bio: &'a Bio) -> impl Iterator<Item = &'a str> {
        let fields: &'a [String; 5] = match self {
            Category::ClothingFeatures => &bio.clothing_features,
            Category::Hobbies => &bio.hobbies,
            Category::Skills => &bio.skills,
        };
        fields.iter().map(String::as_str)
    }

    fn prompt_prefix(&self) -> &'static str {
        match self {
            Category::ClothingFeatures => "Find someone wearing",
            Category::Hobbies => "Find someone whose hobby is",
            Category::Skills => "Find someone whose skill is",
        }
    }
}

/// The bio-derived task pool for `tier` (rules.md §1: "Servants' bios feed
/// into the shared task pool too" -- includes every active player
/// regardless of faction). Groups every active player's non-empty field in
/// the tier's category by value (case-insensitively, so "Chess" and
/// "chess" pool together under whichever casing was submitted first), and
/// excludes any prompt that's already been pushed as a real `TaskDef` --
/// this pool is meant to offer fresh options each time it's read, not
/// repeat a task Round 3 already used by the time Round 5 asks again.
pub fn task_candidates(state: &GameState, tier: TaskTier) -> Vec<TaskCandidate> {
    let category = Category::for_tier(tier);
    let already_used: BTreeSet<String> = state.tasks().map(|t| t.prompt.clone()).collect();

    let mut grouped: BTreeMap<String, (String, Vec<PlayerId>)> = BTreeMap::new();
    for (id, bio) in state.bios() {
        let active = state
            .player(id)
            .is_some_and(|p| p.status == PlayerStatus::Active);
        if !active {
            continue;
        }
        for raw in category.values(bio) {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            let key = trimmed.to_lowercase();
            let entry = grouped
                .entry(key)
                .or_insert_with(|| (trimmed.to_string(), Vec::new()));
            if !entry.1.contains(&id) {
                entry.1.push(id);
            }
        }
    }

    grouped
        .into_values()
        .filter_map(|(display_value, qualifying_players)| {
            let prompt = format!(
                "{} {}",
                category.prompt_prefix(),
                pascal_case(&display_value)
            );
            if already_used.contains(&prompt) {
                return None;
            }
            Some(TaskCandidate {
                prompt,
                qualifying_players,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_bio() -> Bio {
        Bio {
            character_name: "Lord Ashworth".into(),
            real_name: "Alex".into(),
            occupation: "Duke".into(),
            hobbies: [
                "chess".into(),
                "fencing".into(),
                "poetry".into(),
                "".into(),
                "".into(),
            ],
            clothing_features: [
                "a silver mask".into(),
                "".into(),
                "".into(),
                "".into(),
                "".into(),
            ],
            skills: [
                "sword fighting".into(),
                "".into(),
                "".into(),
                "".into(),
                "".into(),
            ],
        }
    }

    #[test]
    fn a_bio_within_every_length_cap_validates_clean() {
        assert_eq!(sample_bio().first_oversized_field(), None);
    }

    #[test]
    fn an_over_length_field_is_reported_with_its_label_and_length() {
        let mut bio = sample_bio();
        bio.occupation = "a".repeat(33);
        assert_eq!(bio.first_oversized_field(), Some(("occupation", 33)));
    }

    #[test]
    fn exactly_the_cap_is_still_valid() {
        let mut bio = sample_bio();
        bio.real_name = "a".repeat(MAX_FIELD_LEN);
        assert_eq!(bio.first_oversized_field(), None);
    }

    #[test]
    fn an_over_length_hobby_is_caught_too() {
        let mut bio = sample_bio();
        bio.hobbies[2] = "a".repeat(40);
        assert_eq!(bio.first_oversized_field(), Some(("hobby", 40)));
    }

    #[test]
    fn pascal_case_title_cases_every_word_and_preserves_spacing() {
        assert_eq!(pascal_case("long flowing dress"), "Long Flowing Dress");
        assert_eq!(pascal_case("SWORD FIGHTING"), "Sword Fighting");
        assert_eq!(pascal_case(""), "");
        assert_eq!(pascal_case("a"), "A");
    }

    // --- task_candidates ---

    use crate::command::Command;
    use crate::event::DomainEvent;
    use crate::player::Faction;
    use crate::state::apply_command;

    fn add_player(state: &mut GameState, name: &str, faction: Faction) -> PlayerId {
        let events = apply_command(state, Command::AddPlayer { name: name.into() }).unwrap();
        let id = match events[0] {
            DomainEvent::PlayerAdded { id, .. } => id,
            _ => unreachable!(),
        };
        apply_command(
            state,
            Command::AssignFaction {
                player: id,
                faction,
            },
        )
        .unwrap();
        id
    }

    fn submit_bio(state: &mut GameState, player: PlayerId, bio: Bio) {
        apply_command(state, Command::SubmitBio { player, bio }).unwrap();
    }

    #[test]
    fn easy_medium_hard_map_to_clothing_hobbies_and_skills_respectively() {
        let mut state = GameState::new();
        let alice = add_player(&mut state, "Alice", Faction::Ton);
        submit_bio(&mut state, alice, sample_bio());

        let easy = task_candidates(&state, TaskTier::Easy);
        assert!(easy.iter().any(|c| c.prompt.contains("Silver Mask")));

        let medium = task_candidates(&state, TaskTier::Medium);
        assert!(medium.iter().any(|c| c.prompt.contains("Chess")));

        let hard = task_candidates(&state, TaskTier::Hard);
        assert!(hard.iter().any(|c| c.prompt.contains("Sword Fighting")));
    }

    #[test]
    fn two_players_sharing_a_hobby_case_insensitively_pool_into_one_candidate() {
        let mut state = GameState::new();
        let alice = add_player(&mut state, "Alice", Faction::Ton);
        let bob = add_player(&mut state, "Bob", Faction::Uprising);
        let mut alice_bio = sample_bio();
        alice_bio.hobbies[0] = "Chess".into();
        submit_bio(&mut state, alice, alice_bio);
        let mut bob_bio = sample_bio();
        bob_bio.hobbies[0] = "chess".into();
        submit_bio(&mut state, bob, bob_bio);

        let candidates = task_candidates(&state, TaskTier::Medium);
        let chess = candidates
            .iter()
            .find(|c| c.prompt.contains("Chess"))
            .expect("a chess candidate should exist");
        assert_eq!(chess.qualifying_players.len(), 2);
        assert!(chess.qualifying_players.contains(&alice));
        assert!(chess.qualifying_players.contains(&bob));
        // Only one candidate for "chess" despite the casing difference --
        // not two separate ones.
        assert_eq!(
            candidates
                .iter()
                .filter(|c| c.prompt.contains("hess"))
                .count(),
            1
        );
    }

    #[test]
    fn a_cast_out_players_bio_values_are_excluded() {
        let mut state = GameState::new();
        let alice = add_player(&mut state, "Alice", Faction::Ton);
        apply_command(&mut state, Command::FinalizeSetup).unwrap();
        submit_bio(&mut state, alice, sample_bio());
        apply_command(
            &mut state,
            Command::CastOut {
                player: alice,
                fallback_replacement: None,
            },
        )
        .unwrap();

        let candidates = task_candidates(&state, TaskTier::Medium);
        assert!(candidates.is_empty());
    }

    #[test]
    fn empty_bio_fields_never_produce_a_candidate() {
        let mut state = GameState::new();
        let alice = add_player(&mut state, "Alice", Faction::Ton);
        submit_bio(&mut state, alice, sample_bio());

        // sample_bio() only fills hobbies[0..3]; the rest are "".
        let candidates = task_candidates(&state, TaskTier::Medium);
        assert_eq!(candidates.len(), 3);
    }

    #[test]
    fn a_prompt_already_pushed_as_a_real_task_is_not_offered_again() {
        let mut state = GameState::new();
        let alice = add_player(&mut state, "Alice", Faction::Ton);
        submit_bio(&mut state, alice, sample_bio());

        let candidates = task_candidates(&state, TaskTier::Medium);
        let chess = candidates
            .iter()
            .find(|c| c.prompt.contains("Chess"))
            .unwrap()
            .clone();
        apply_command(
            &mut state,
            Command::PushTask {
                prompt: chess.prompt.clone(),
                tier: TaskTier::Medium,
                qualifying_players: chess.qualifying_players.iter().copied().collect(),
            },
        )
        .unwrap();

        let candidates_after = task_candidates(&state, TaskTier::Medium);
        assert!(!candidates_after.iter().any(|c| c.prompt == chess.prompt));
        // The other two hobbies are still on offer.
        assert_eq!(candidates_after.len(), 2);
    }

    #[test]
    fn servants_bios_feed_the_pool_too() {
        let mut state = GameState::new();
        let servant = add_player(&mut state, "LateArrival", Faction::Servant);
        submit_bio(&mut state, servant, sample_bio());

        let candidates = task_candidates(&state, TaskTier::Medium);
        assert!(candidates.iter().any(|c| c.prompt.contains("Chess")));
    }
}
