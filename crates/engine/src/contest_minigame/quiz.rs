//! Trivia and Math: mechanically identical shared-question quizzes,
//! differing only in which authored bank of 10 multiple-choice questions
//! is loaded. Correctness is checked engine-side (`correct_choice` is
//! Host-only-visible, the same `TaskView::answer_code` scoping pattern
//! this project already established) since a client can't be trusted to
//! self-report whether *it* got a question right for a mechanic that
//! decides a category's winner.

use crate::error::GameError;
use crate::player::PlayerId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuizKind {
    Trivia,
    Math,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuizQuestion {
    pub prompt: String,
    pub choices: Vec<String>,
    /// Host-only ground truth -- see `crate::view::ContestMinigameView`'s
    /// scoping of the equivalent field.
    pub correct_choice: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuizProgress {
    pub answers: Vec<usize>,
    pub correct_count: u32,
    pub elapsed_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuizPayload {
    pub questions: Vec<QuizQuestion>,
    pub progress: BTreeMap<PlayerId, QuizProgress>,
}

/// Every quiz bank must be exactly 10 questions -- Dalton's own explicit
/// spec for both Trivia and Math. Checked here (not just by the const
/// banks' own fixed-size arrays below) since `OpenMinigameDetail::
/// Intelligence(IntelligenceKind::Trivia{questions})` still takes a plain
/// `Vec`, which a future content edit could accidentally miscount.
pub(crate) fn check_question_count(questions: &[QuizQuestion]) -> Result<(), GameError> {
    if questions.len() != 10 {
        return Err(GameError::QuizMustHaveExactlyTenQuestions(questions.len()));
    }
    Ok(())
}

/// Score for the shared top-N-by-faction resolver: most correct answers
/// wins, ties broken by fastest total time -- pre-sorted by `(correct_count
/// desc, elapsed_ms asc)` so `resolve_ton_won`'s own stable re-sort
/// preserves the speed tiebreak among any exact-score ties (same trick as
/// Wordle's `wordle_scores`).
pub(crate) fn quiz_scores(payload: &QuizPayload) -> Vec<(PlayerId, i64)> {
    let mut scored: Vec<(PlayerId, i64, u64)> = payload
        .progress
        .iter()
        .map(|(&id, progress)| {
            (
                id,
                i64::from(progress.correct_count),
                progress.elapsed_ms.unwrap_or(u64::MAX),
            )
        })
        .collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.2.cmp(&b.2)));
    scored
        .into_iter()
        .map(|(id, correct_count, _)| (id, correct_count))
        .collect()
}

fn q(prompt: &str, choices: [&str; 4], correct_choice: usize) -> QuizQuestion {
    QuizQuestion {
        prompt: prompt.to_string(),
        choices: choices.iter().map(|c| c.to_string()).collect(),
        correct_choice,
    }
}

/// 10 real multiple-choice questions on Regency-era English history (the
/// period Bridgerton is set in) -- Dalton's own explicit instruction:
/// real content now, not a placeholder.
pub fn trivia_questions() -> Vec<QuizQuestion> {
    vec![
        q(
            "Who served as Prince Regent of the United Kingdom from 1811 to 1820, giving the era its name?",
            [
                "The future King George IV",
                "The future King William IV",
                "The Duke of Wellington",
                "The future King George III",
            ],
            0,
        ),
        q(
            "Parliament established the Regency in 1811 because King George III was considered unfit to rule due to:",
            ["Blindness", "His mental illness", "A fatal illness", "Deafness"],
            1,
        ),
        q(
            "What was the grand London residence where the Prince Regent hosted his most lavish parties?",
            ["Carlton House", "Buckingham Palace", "Kensington Palace", "Windsor Castle"],
            0,
        ),
        q(
            "The exclusive assembly rooms considered the most important venue for Regency-era society balls were called:",
            ["Vauxhall Gardens", "Almack's", "The Pantheon", "Drury Lane"],
            1,
        ),
        q(
            "Britain spent most of the Regency era at war with France in what conflict?",
            ["The Seven Years' War", "The Crimean War", "The Napoleonic Wars", "The War of 1812"],
            2,
        ),
        q(
            "What 1815 battle finally ended Napoleon Bonaparte's rule?",
            ["The Battle of Trafalgar", "The Battle of Waterloo", "The Battle of Leipzig", "The Battle of the Nile"],
            1,
        ),
        q(
            "George Bryan \"Beau\" Brummell was the era's most famous trendsetter in what field?",
            ["Men's fashion", "Landscape painting", "Architecture", "Horse breeding"],
            0,
        ),
        q(
            "Which author published beloved novels of manners and marriage among the English gentry anonymously (\"By a Lady\") during this period?",
            ["Charlotte Bronte", "Jane Austen", "Mary Shelley", "George Eliot"],
            1,
        ),
        q(
            "The annual \"London Season,\" when high society gathered in town for balls and courtship, traditionally ran alongside the sitting of what institution?",
            ["The Royal Academy", "Parliament", "The Church of England's synod", "The Royal Navy's admiralty"],
            1,
        ),
        q(
            "A young woman's formal introduction to society, after which she was considered eligible for courtship, was known as her:",
            ["Coronation", "Debut (or \"coming out\")", "Investiture", "Betrothal"],
            1,
        ),
    ]
}

/// 10 real multiple-choice questions spanning arithmetic, geometry,
/// algebra, trigonometry, and calculus (2 each) -- Dalton's own explicit
/// instruction: real content now, not a placeholder.
pub fn math_questions() -> Vec<QuizQuestion> {
    vec![
        // Arithmetic
        q("What is 17 + 28?", ["43", "44", "45", "46"], 2),
        q("What is 144 divided by 12?", ["10", "11", "12", "14"], 2),
        // Geometry
        q(
            "How many degrees do the interior angles of a triangle add up to?",
            ["90", "180", "270", "360"],
            1,
        ),
        q(
            "What is the area of a rectangle with sides 6 and 4?",
            ["20", "22", "24", "26"],
            2,
        ),
        // Algebra
        q("Solve for x: 2x + 3 = 11", ["3", "4", "5", "6"], 1),
        q("What is x^2 when x = 5?", ["10", "15", "20", "25"], 3),
        // Trigonometry
        q(
            "What is sin(90 degrees)?",
            ["0", "0.5", "1", "Undefined"],
            2,
        ),
        q("What is cos(0 degrees)?", ["0", "0.5", "1", "-1"], 2),
        // Calculus
        q(
            "What is the derivative of x^2 with respect to x?",
            ["x", "2x", "x^2", "2"],
            1,
        ),
        q(
            "What is the integral of 2x dx?",
            ["x^2 + C", "2x^2 + C", "x + C", "x^2"],
            0,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trivia_and_math_banks_have_exactly_ten_valid_questions() {
        for questions in [trivia_questions(), math_questions()] {
            assert!(check_question_count(&questions).is_ok());
            for question in &questions {
                assert_eq!(question.choices.len(), 4);
                assert!(question.correct_choice < question.choices.len());
            }
        }
    }

    #[test]
    fn check_question_count_rejects_a_miscounted_bank() {
        let mut questions = trivia_questions();
        questions.pop();
        assert_eq!(
            check_question_count(&questions),
            Err(GameError::QuizMustHaveExactlyTenQuestions(9))
        );
    }

    #[test]
    fn quiz_scores_ranks_by_correct_count_then_elapsed_time() {
        let alice = PlayerId(0);
        let bob = PlayerId(1);
        let mut progress = BTreeMap::new();
        progress.insert(
            alice,
            QuizProgress {
                answers: vec![0; 10],
                correct_count: 8,
                elapsed_ms: Some(9000),
            },
        );
        progress.insert(
            bob,
            QuizProgress {
                answers: vec![0; 10],
                correct_count: 8,
                elapsed_ms: Some(4000),
            },
        );
        let payload = QuizPayload {
            questions: trivia_questions(),
            progress,
        };
        // Tied on correct_count -- Bob's faster time must come first.
        assert_eq!(quiz_scores(&payload), vec![(bob, 8), (alice, 8)]);
    }
}
