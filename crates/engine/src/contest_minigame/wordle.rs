//! Wordle: a shared secret 5-letter word, up to 6 guesses, per-letter
//! feedback. `score_guess`/`normalize_guess`/`WORD_LIST` are ported nearly
//! verbatim from a sibling project, `/home/drc/game-changer`'s
//! `src/state.rs`/`src/games/word_game.rs` -- same author, already real,
//! already unit-tested (including both duplicate-letter directions), and
//! `WORD_LIST`'s own 2,686 entries are already deduplicated and
//! hand-screened for profanity/slurs (see that project's own doc comment
//! on the list). No dictionary validation is applied to *guesses* here
//! either, matching that precedent -- only the secret itself is drawn from
//! this curated list.

use crate::player::PlayerId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Max guesses per player per Wordle session.
pub const MAX_GUESSES: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LetterFeedback {
    /// Right letter, right position ("green").
    Correct,
    /// Right letter, wrong position, and not already claimed by an earlier
    /// `Correct`/`Present` match in this same guess ("yellow").
    Present,
    /// Not in the secret at this position, or every occurrence of this
    /// letter in the secret is already accounted for ("gray").
    Absent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordleGuess {
    pub word: String,
    pub feedback: [LetterFeedback; 5],
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordleProgress {
    pub guesses: Vec<WordleGuess>,
    pub solved: bool,
    pub elapsed_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordlePayload {
    pub secret: String,
    pub progress: std::collections::BTreeMap<PlayerId, WordleProgress>,
}

/// Trims and uppercases a guess, rejecting anything that isn't exactly 5
/// ASCII letters. Ported from game-changer's `normalize_guess`.
pub(crate) fn normalize_guess(raw: &str) -> Result<String, &'static str> {
    let trimmed = raw.trim();
    if trimmed.chars().count() != 5 || !trimmed.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err("guess must be a 5-letter word");
    }
    Ok(trimmed.to_uppercase())
}

/// Standard two-pass Wordle scoring, ported from game-changer's
/// `score_guess`. Pass 1 marks exact-position matches and removes them
/// from a per-letter remaining-count map of the secret; pass 2 marks
/// present-but-wrong-position from what's left. Correctly handles
/// duplicate letters in either direction. Assumes both strings are exactly
/// 5 ASCII bytes (guaranteed by `normalize_guess` for guesses, and by
/// every `WORD_LIST` entry for secrets).
pub(crate) fn score_guess(guess: &str, secret: &str) -> [LetterFeedback; 5] {
    let guess_bytes: Vec<u8> = guess.bytes().collect();
    let secret_bytes: Vec<u8> = secret.bytes().collect();
    let mut feedback = [LetterFeedback::Absent; 5];
    let mut remaining: HashMap<u8, u32> = HashMap::new();

    for i in 0..5 {
        if guess_bytes[i] == secret_bytes[i] {
            feedback[i] = LetterFeedback::Correct;
        } else {
            *remaining.entry(secret_bytes[i]).or_insert(0) += 1;
        }
    }
    for i in 0..5 {
        if feedback[i] == LetterFeedback::Correct {
            continue;
        }
        if let Some(count) = remaining.get_mut(&guess_bytes[i]) {
            if *count > 0 {
                feedback[i] = LetterFeedback::Present;
                *count -= 1;
            }
        }
    }
    feedback
}

/// Score for the shared top-N-by-faction resolver: `if solved { 7 -
/// guesses_used } else { 0 }`, so fewer guesses scores higher, matching
/// the "higher score wins" convention every other category already uses
/// (e.g. Strength's `n - placement + 1`). Pre-sorted by `(score desc,
/// elapsed_ms asc)` -- "fewest guesses, ties by fastest solve" -- so
/// `resolve_ton_won`'s own stable re-sort-by-score-alone preserves this
/// tiebreak among any exact-score ties, the same trick Creativity's
/// median/mean scoring and Quiz's speed tiebreak both rely on.
pub(crate) fn wordle_scores(payload: &WordlePayload) -> Vec<(PlayerId, i64)> {
    let mut scored: Vec<(PlayerId, i64, u64)> = payload
        .progress
        .iter()
        .map(|(&id, progress)| {
            let score = if progress.solved {
                (7 - progress.guesses.len()) as i64
            } else {
                0
            };
            (id, score, progress.elapsed_ms.unwrap_or(u64::MAX))
        })
        .collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.2.cmp(&b.2)));
    scored
        .into_iter()
        .map(|(id, score, _)| (id, score))
        .collect()
}

/// Curated 5-letter secret-word pool, ported directly from game-changer's
/// own already-audited `word_game::WORD_LIST` -- see this module's doc
/// comment.
pub static WORD_LIST: [&str; 2686] = [
    "ABASH", "ABBEY", "ABIDE", "ABLER", "ABODE", "ABOUT", "ABOVE", "ABUSE", "ACIDS", "ACORN",
    "ACRES", "ACRID", "ACTOR", "ACUTE", "ADAPT", "ADDED", "ADEPT", "ADMIT", "ADOBE", "ADORE",
    "ADULT", "AFOOT", "AFTER", "AGAIN", "AGENT", "AGILE", "AGING", "AGONY", "AGREE", "AHEAD",
    "AIDED", "AIMED", "AISLE", "ALARM", "ALBUM", "ALERT", "ALIEN", "ALIKE", "ALIVE", "ALLEY",
    "ALLOT", "ALLOW", "ALLOY", "ALOFT", "ALOHA", "ALONE", "ALOUD", "ALPHA", "ALTER", "AMBER",
    "AMEND", "AMISS", "AMONG", "AMPLE", "AMUSE", "ANGEL", "ANGER", "ANGLE", "ANGRY", "ANIME",
    "ANKLE", "ANNEX", "ANNOY", "ANNUL", "ANTIC", "ANVIL", "AORTA", "APART", "APHID", "APPLE",
    "APPLY", "APRON", "ARBOR", "ARDOR", "AREAS", "ARENA", "ARGON", "ARGUE", "AROMA", "AROSE",
    "ARRAY", "ARROW", "ASHEN", "ASIDE", "ASKED", "ASSAY", "ASSET", "AUDIO", "AUGUR", "AVAIL",
    "AVIAN", "AVOID", "AWAKE", "AWFUL", "AWOKE", "AXIOM", "AXLES", "BACON", "BADGE", "BAGEL",
    "BAKED", "BAKER", "BALKY", "BALLS", "BALMY", "BANDS", "BANJO", "BANKS", "BARGE", "BARKS",
    "BARNS", "BASED", "BASES", "BASIC", "BASIL", "BASIN", "BASIS", "BASTE", "BATCH", "BATHS",
    "BATON", "BEACH", "BEADS", "BEAKS", "BEAMS", "BEANS", "BEARD", "BEARS", "BEAST", "BEEFY",
    "BEEPS", "BEERS", "BEETS", "BEFIT", "BEGIN", "BEING", "BELLE", "BELLS", "BELLY", "BELOW",
    "BELTS", "BENCH", "BENDS", "BERET", "BERRY", "BESET", "BIBLE", "BIKER", "BILLS", "BINDS",
    "BINGO", "BIRDS", "BIRTH", "BITES", "BLABS", "BLACK", "BLADE", "BLANK", "BLAST", "BLAZE",
    "BLEAK", "BLEED", "BLEND", "BLESS", "BLIMP", "BLIND", "BLINK", "BLISS", "BLOAT", "BLOCK",
    "BLOOD", "BLOOM", "BLOTS", "BLOWN", "BLOWS", "BLUER", "BLUES", "BLUFF", "BLUNT", "BLURB",
    "BLURT", "BLUSH", "BOARD", "BOARS", "BOAST", "BOATS", "BOGGY", "BOGUS", "BOILS", "BOLTS",
    "BOMBS", "BONDS", "BONED", "BONES", "BONGS", "BONUS", "BOOKS", "BOOMS", "BOOST", "BOOTH",
    "BOOTS", "BORED", "BORNE", "BOSSY", "BOUND", "BOUTS", "BOWED", "BOWEL", "BOWER", "BOXED",
    "BOXER", "BOXES", "BRAID", "BRAIN", "BRAKE", "BRAND", "BRASH", "BRASS", "BRATS", "BRAVE",
    "BRAWL", "BRAWN", "BREAD", "BREAK", "BREED", "BRICK", "BRIDE", "BRIEF", "BRINE", "BRING",
    "BRINK", "BRISK", "BROAD", "BROIL", "BROKE", "BROOD", "BROOK", "BROOM", "BROTH", "BROWN",
    "BRUSH", "BRUTE", "BUCKS", "BUDDY", "BUGGY", "BUGLE", "BUILD", "BUILT", "BULBS", "BULGE",
    "BULKY", "BULLS", "BUMPS", "BUMPY", "BUNCH", "BUNKS", "BUNNY", "BUOYS", "BURLY", "BURNS",
    "BURNT", "BURST", "BUSED", "BUSHY", "BUSTS", "BUTTE", "BUYER", "BYLAW", "CABBY", "CABIN",
    "CABLE", "CACTI", "CADET", "CAGED", "CAGES", "CAKES", "CALVE", "CAMEL", "CAMPS", "CAMPY",
    "CANAL", "CANDY", "CANES", "CANNY", "CANOE", "CAPED", "CARDS", "CARED", "CARES", "CARGO",
    "CARPS", "CARRY", "CARTS", "CARVE", "CASED", "CASKS", "CASTS", "CATCH", "CATER", "CATTY",
    "CAULK", "CAUSE", "CAVES", "CEASE", "CEDAR", "CEDED", "CELLO", "CELLS", "CENTS", "CHAIN",
    "CHAIR", "CHALK", "CHAMP", "CHANT", "CHAOS", "CHAPS", "CHARM", "CHART", "CHASE", "CHATS",
    "CHEAP", "CHEEK", "CHEER", "CHEFS", "CHESS", "CHEST", "CHEWS", "CHEWY", "CHICK", "CHIDE",
    "CHIEF", "CHILD", "CHILI", "CHILL", "CHIME", "CHIPS", "CHIRP", "CHOIR", "CHOKE", "CHOMP",
    "CHOPS", "CHORD", "CHORE", "CHOSE", "CHOWS", "CHUGS", "CHUMS", "CHUNK", "CIGAR", "CINCH",
    "CIRCA", "CITED", "CITES", "CIVIC", "CIVIL", "CLAIM", "CLAMP", "CLAMS", "CLANG", "CLANK",
    "CLASH", "CLASP", "CLASS", "CLAWS", "CLEAN", "CLEAR", "CLEFS", "CLEFT", "CLERK", "CLICK",
    "CLIFF", "CLIMB", "CLING", "CLIPS", "CLOAK", "CLOCK", "CLOGS", "CLONE", "CLOPS", "CLOSE",
    "CLOTH", "CLOTS", "CLOUD", "CLOUT", "CLOWN", "CLUBS", "CLUCK", "CLUED", "CLUES", "CLUMP",
    "COACH", "COALS", "COAST", "COATS", "COBRA", "COCOA", "CODED", "CODES", "COILS", "COINS",
    "COLAS", "COLDS", "COLOR", "COLTS", "COMBO", "COMBS", "COMET", "COMFY", "COMIC", "COMMA",
    "CONCH", "CONDO", "CONES", "CONGA", "COOED", "COOKS", "COOLS", "COOPS", "COPED", "COPES",
    "CORAL", "CORDS", "CORED", "CORES", "CORKS", "CORNY", "COSTS", "COUCH", "COUGH", "COUNT",
    "COUPE", "COUPS", "COURT", "COVER", "COVES", "COWED", "COYLY", "CRABS", "CRACK", "CRAFT",
    "CRAGS", "CRAMP", "CRAMS", "CRANE", "CRANK", "CRASH", "CRATE", "CRAVE", "CRAWL", "CREAK",
    "CREAM", "CREEK", "CREEP", "CREPE", "CREPT", "CREST", "CREWS", "CRICK", "CRIED", "CRIES",
    "CRIME", "CRISP", "CROAK", "CROOK", "CROPS", "CROSS", "CROWD", "CROWN", "CRUDE", "CRUEL",
    "CRUMB", "CRUSH", "CRUST", "CUBED", "CUBES", "CUFFS", "CULTS", "CURBS", "CURED", "CURES",
    "CURLS", "CURLY", "CURRY", "CURSE", "CURVE", "CURVY", "CUTER", "CYCLE", "DADDY", "DAILY",
    "DAIRY", "DALLY", "DAMES", "DAMNS", "DAMPS", "DANCE", "DANDY", "DARED", "DARES", "DARTS",
    "DATED", "DATUM", "DAUNT", "DAWNS", "DEALS", "DEALT", "DEATH", "DEBTS", "DEBUT", "DECAY",
    "DECKS", "DECOR", "DECOY", "DEEDS", "DEEMS", "DEFER", "DEIGN", "DEITY", "DELAY", "DELTA",
    "DEMOS", "DENIM", "DENSE", "DEPTH", "DERBY", "DESKS", "DEVIL", "DIALS", "DIARY", "DICED",
    "DICES", "DIETS", "DIGIT", "DIMLY", "DINED", "DINER", "DINES", "DINGO", "DINGY", "DIRTY",
    "DISCO", "DISCS", "DITCH", "DIVAS", "DIVED", "DIVER", "DIVES", "DIVOT", "DIZZY", "DOCKS",
    "DODGE", "DOERS", "DOGMA", "DOING", "DOLLS", "DOLLY", "DOMES", "DONOR", "DONUT", "DOORS",
    "DOSED", "DOSES", "DOTED", "DOTES", "DOUBT", "DOUGH", "DOVES", "DOZEN", "DRAFT", "DRAGS",
    "DRAIN", "DRAKE", "DRAMA", "DRANK", "DRAWN", "DRAWS", "DREAD", "DREAM", "DRESS", "DRIED",
    "DRIER", "DRIES", "DRIFT", "DRILL", "DRINK", "DRIPS", "DRIVE", "DRONE", "DROOL", "DROOP",
    "DROPS", "DROSS", "DROVE", "DROWN", "DRUID", "DRUMS", "DRYER", "DUCKS", "DUELS", "DUETS",
    "DUMPS", "DUNES", "DUSKY", "DUSTS", "DUSTY", "DYING", "EAGER", "EAGLE", "EARLS", "EARLY",
    "EARNS", "EARTH", "EASED", "EASEL", "EASES", "EATEN", "EATER", "EBBED", "EBONY", "ECHOS",
    "EDGED", "EDGES", "EGGED", "EIGHT", "EJECT", "ELBOW", "ELDER", "ELECT", "ELFIN", "ELITE",
    "ELOPE", "ELVES", "EMAIL", "EMBED", "EMBER", "EMCEE", "EMOJI", "EMPTY", "ENACT", "ENDED",
    "ENEMY", "ENJOY", "ENSUE", "ENTER", "ENTRY", "ENVOY", "EPOCH", "EPOXY", "EQUAL", "EQUIP",
    "ERASE", "ERECT", "ERODE", "ERROR", "ESSAY", "ETHER", "ETHIC", "EVENT", "EVERY", "EVOKE",
    "EXACT", "EXALT", "EXAMS", "EXCEL", "EXERT", "EXILE", "EXIST", "EXITS", "EXPEL", "EXTRA",
    "FABLE", "FACED", "FACES", "FACET", "FADED", "FADES", "FAINT", "FAIRS", "FAIRY", "FAITH",
    "FAKED", "FAKER", "FAKES", "FALLS", "FALSE", "FANCY", "FANGS", "FARED", "FARES", "FARMS",
    "FARTS", "FATAL", "FATED", "FATES", "FATTY", "FAULT", "FAVOR", "FAWNS", "FEARS", "FEAST",
    "FEATS", "FEEDS", "FEELS", "FEIGN", "FELON", "FEMUR", "FENCE", "FERNS", "FERRY", "FETAL",
    "FETCH", "FETUS", "FEVER", "FEWER", "FIBER", "FIELD", "FIEND", "FIERY", "FIFTH", "FIFTY",
    "FIGHT", "FILED", "FILES", "FILET", "FILLS", "FILMS", "FILTH", "FINAL", "FINCH", "FINDS",
    "FINED", "FINES", "FIRED", "FIRES", "FIRMS", "FIRST", "FISHY", "FIXED", "FIXER", "FIXES",
    "FIZZY", "FJORD", "FLAGS", "FLAIL", "FLAIR", "FLAKE", "FLAKY", "FLAME", "FLANK", "FLAPS",
    "FLARE", "FLASH", "FLASK", "FLATS", "FLEAS", "FLECK", "FLEET", "FLESH", "FLICK", "FLIER",
    "FLIES", "FLING", "FLINT", "FLIPS", "FLIRT", "FLOAT", "FLOCK", "FLOGS", "FLOOD", "FLOOR",
    "FLOPS", "FLOSS", "FLOUR", "FLOWN", "FLOWS", "FLUFF", "FLUID", "FLUKE", "FLUME", "FLUNG",
    "FLUNK", "FLUSH", "FLUTE", "FOAMS", "FOAMY", "FOCAL", "FOCUS", "FOGGY", "FOLDS", "FOLKS",
    "FONTS", "FOODS", "FOOLS", "FORAY", "FORCE", "FORGE", "FORGO", "FORMS", "FORTH", "FORTS",
    "FORTY", "FORUM", "FOUND", "FOWLS", "FOXES", "FRAIL", "FRAME", "FRANK", "FRAUD", "FRAYS",
    "FREAK", "FREED", "FREES", "FRESH", "FRIED", "FRIES", "FRISK", "FROGS", "FROND", "FRONT",
    "FROST", "FROTH", "FROWN", "FROZE", "FRUIT", "FUDGE", "FUELS", "FUMED", "FUMES", "FUNDS",
    "FUNGI", "FUNKY", "FUNNY", "FUZES", "FUZZY", "GABLE", "GAFFE", "GAINS", "GALES", "GALLS",
    "GAMER", "GAMES", "GAMMA", "GANGS", "GAPED", "GAPES", "GASES", "GASPS", "GATED", "GATES",
    "GAUGE", "GAUZE", "GAVEL", "GAWKS", "GAZED", "GAZES", "GEARS", "GECKO", "GEESE", "GENIE",
    "GENII", "GENRE", "GENTS", "GERMS", "GHOST", "GHOUL", "GIANT", "GIDDY", "GIFTS", "GILLS",
    "GIRLS", "GIRTH", "GIVEN", "GIVES", "GLADE", "GLAND", "GLARE", "GLASS", "GLAZE", "GLEAM",
    "GLEAN", "GLIDE", "GLINT", "GLOAT", "GLOBE", "GLOOM", "GLORY", "GLOSS", "GLOVE", "GLOWS",
    "GLUED", "GLUES", "GLUEY", "GNASH", "GNATS", "GNOME", "GOATS", "GOING", "GOLDS", "GOLFS",
    "GOODS", "GOOFY", "GOOSE", "GORGE", "GOWNS", "GRABS", "GRACE", "GRADE", "GRAFT", "GRAIN",
    "GRAND", "GRANT", "GRAPH", "GRASP", "GRASS", "GRATE", "GRAVE", "GRAVY", "GRAZE", "GREAT",
    "GREED", "GREEN", "GREET", "GRIEF", "GRILL", "GRIME", "GRIMY", "GRIND", "GRIPS", "GROAN",
    "GROIN", "GROOM", "GROSS", "GROUP", "GROUT", "GROVE", "GROWL", "GROWN", "GROWS", "GRUEL",
    "GRUFF", "GRUNT", "GUARD", "GUAVA", "GUESS", "GUEST", "GUIDE", "GUILD", "GUILT", "GUISE",
    "GULCH", "GULLS", "GULPS", "GUMBO", "GUMMY", "GUSTO", "GUSTS", "GUTSY", "HABIT", "HAIRS",
    "HAIRY", "HALLS", "HALOS", "HALVE", "HANDS", "HANDY", "HANGS", "HAPPY", "HARDY", "HARES",
    "HARMS", "HARPS", "HARSH", "HASTE", "HASTY", "HATCH", "HATED", "HATER", "HATES", "HAULS",
    "HAUNT", "HAVEN", "HAWKS", "HAYED", "HAZEL", "HAZES", "HEADS", "HEAPS", "HEARD", "HEARS",
    "HEART", "HEAVE", "HEAVY", "HEDGE", "HEELS", "HEFTY", "HEIRS", "HELLO", "HELMS", "HELPS",
    "HENCE", "HERBS", "HERDS", "HEROS", "HIKER", "HILLS", "HINGE", "HINTS", "HIPPO", "HIPPY",
    "HIRED", "HIRES", "HITCH", "HIVES", "HOARD", "HOARY", "HOBBY", "HOCKS", "HOIST", "HOLDS",
    "HOLES", "HOMED", "HOMES", "HONED", "HONES", "HONEY", "HONOR", "HOOFS", "HOOKS", "HOOPS",
    "HOOTS", "HOPED", "HOPES", "HORDE", "HORNS", "HORSE", "HOSED", "HOSES", "HOTEL", "HOTLY",
    "HOUND", "HOURS", "HOUSE", "HOVEL", "HOVER", "HULLS", "HUMAN", "HUMID", "HUMOR", "HUMPS",
    "HUNCH", "HUNTS", "HURLS", "HURRY", "HURTS", "HUSKS", "HUSKY", "HUTCH", "HYENA", "HYMNS",
    "HYPER", "ICING", "ICONS", "IDEAL", "IDLED", "IDLER", "IDLES", "IDYLL", "IGLOO", "IMAGE",
    "IMBUE", "IMPEL", "IMPLY", "INBOX", "INDEX", "INFER", "INGOT", "INKED", "INLET", "INNER",
    "INPUT", "INTRO", "IRATE", "IRKED", "IRONS", "IRONY", "ISLES", "ISSUE", "ITCHY", "IVORY",
    "JACKS", "JADED", "JAILS", "JAZZY", "JEANS", "JELLS", "JELLY", "JERKS", "JERKY", "JESTS",
    "JETTY", "JEWEL", "JIVES", "JOINS", "JOINT", "JOKED", "JOKER", "JOKES", "JOLLY", "JOUST",
    "JOYED", "JUDGE", "JUICE", "JUICY", "JUMBO", "JUMPS", "JUMPY", "JUNKS", "JUNKY", "JUROR",
    "KAYAK", "KEBAB", "KEEPS", "KETCH", "KEYED", "KICKS", "KINGS", "KIOSK", "KITES", "KITTY",
    "KNACK", "KNEAD", "KNEEL", "KNEES", "KNELT", "KNIFE", "KNITS", "KNOBS", "KNOCK", "KNOLL",
    "KNOTS", "KNOWN", "KOALA", "LABEL", "LABOR", "LACED", "LACES", "LACKS", "LADEN", "LADLE",
    "LAGER", "LAKES", "LAMBS", "LAMPS", "LANCE", "LANDS", "LANES", "LAPEL", "LAPSE", "LARGE",
    "LARKS", "LASER", "LASSO", "LATCH", "LATER", "LATTE", "LAUDS", "LAUGH", "LAYER", "LEACH",
    "LEADS", "LEAFY", "LEAKS", "LEAKY", "LEANS", "LEAPS", "LEARN", "LEASE", "LEASH", "LEAST",
    "LEAVE", "LEDGE", "LEECH", "LEEKS", "LEERY", "LEGAL", "LEMON", "LEMUR", "LENDS", "LEVEL",
    "LEVER", "LIBEL", "LIGHT", "LIKED", "LIKEN", "LIKES", "LILAC", "LIMBO", "LIMBS", "LIMIT",
    "LINED", "LINEN", "LINER", "LINES", "LINGO", "LINKS", "LINTS", "LIONS", "LISTS", "LIVED",
    "LIVEN", "LIVER", "LIVES", "LLAMA", "LOADS", "LOAFS", "LOAMY", "LOANS", "LOBBY", "LOBES",
    "LOCAL", "LOCKS", "LODGE", "LOFTS", "LOFTY", "LOGIC", "LOGIN", "LOGOS", "LONER", "LOOKS",
    "LOOMS", "LOOPS", "LOOPY", "LOOSE", "LOOTS", "LOPED", "LOPES", "LORDS", "LORRY", "LOSER",
    "LOSES", "LOTUS", "LOUSY", "LOVED", "LOVER", "LOVES", "LOWER", "LOYAL", "LUCID", "LUCKS",
    "LUCKY", "LUMPS", "LUMPY", "LUNAR", "LUNCH", "LUNGE", "LUNGS", "LURED", "LURES", "LURKS",
    "LUSTY", "LYING", "LYRIC", "MACHO", "MACRO", "MADAM", "MAGIC", "MAIDS", "MAILS", "MAIZE",
    "MAJOR", "MAKER", "MAKES", "MALTS", "MAMMA", "MANES", "MANGE", "MANGO", "MANGY", "MANIA",
    "MANOR", "MAPLE", "MARCH", "MARES", "MARSH", "MASKS", "MASON", "MATCH", "MATED", "MATES",
    "MAUVE", "MAXED", "MAXES", "MAYBE", "MAYOR", "MAZES", "MEALS", "MEANS", "MEANT", "MEATY",
    "MEDAL", "MEDIA", "MELDS", "MELON", "MELTS", "MEMOS", "MENUS", "MERCY", "MERGE", "MERIT",
    "MERRY", "METAL", "METER", "MICRO", "MIDGE", "MIDST", "MIGHT", "MILES", "MILKS", "MILKY",
    "MIMED", "MIMES", "MIMIC", "MINCE", "MINDS", "MINED", "MINER", "MINES", "MINIS", "MINKS",
    "MINOR", "MINTS", "MINUS", "MIRTH", "MISER", "MISTS", "MISTY", "MITTS", "MIXED", "MIXER",
    "MIXES", "MOCHA", "MOCKS", "MODEL", "MODEM", "MODES", "MOGUL", "MOIST", "MOLAR", "MOLDS",
    "MOLDY", "MONEY", "MONKS", "MONTH", "MOODS", "MOODY", "MOOSE", "MOPED", "MOPES", "MORAL",
    "MORES", "MOSSY", "MOTEL", "MOTIF", "MOTOR", "MOUND", "MOUNT", "MOURN", "MOUSE", "MOUTH",
    "MOVED", "MOVER", "MOVES", "MOVIE", "MOWED", "MOWER", "MUCKY", "MUDDY", "MUFFS", "MUGGY",
    "MULCH", "MULES", "MUMMY", "MURAL", "MURKY", "MUSED", "MUSES", "MUSIC", "MUSKY", "MUSTS",
    "MUSTY", "MUTED", "MUTES", "MYTHS", "NACHO", "NAILS", "NAIVE", "NAKED", "NAMED", "NAMES",
    "NASAL", "NASTY", "NATAL", "NAVAL", "NAVEL", "NEEDS", "NEEDY", "NERDY", "NERVE", "NERVY",
    "NESTS", "NEWER", "NEWLY", "NICER", "NICHE", "NICKS", "NIECE", "NIFTY", "NIGHT", "NINES",
    "NINJA", "NINTH", "NOBLE", "NOBLY", "NODES", "NOISE", "NOISY", "NOMAD", "NOOKS", "NOOSE",
    "NORTH", "NOSED", "NOSES", "NOTED", "NOTES", "NOVEL", "NUDGE", "NURSE", "NUTTY", "NYLON",
    "OASIS", "OATHS", "OBESE", "OCCUR", "OCEAN", "ODDER", "ODDLY", "ODORS", "OFFER", "OFTEN",
    "OILED", "OLDER", "OLIVE", "OMEGA", "ONION", "ONSET", "OPENS", "OPERA", "OPTED", "OPTIC",
    "ORBIT", "ORDER", "ORGAN", "OTTER", "OUGHT", "OUNCE", "OUTDO", "OUTED", "OUTER", "OVALS",
    "OVENS", "OWING", "OWLET", "OWNED", "OWNER", "OXIDE", "OZONE", "PACED", "PACES", "PACKS",
    "PADDY", "PAGED", "PAGER", "PAGES", "PAINS", "PAINT", "PAIRS", "PALED", "PALER", "PALES",
    "PALMS", "PANDA", "PANEL", "PANGS", "PANIC", "PANSY", "PANTS", "PAPAS", "PAPER", "PARKA",
    "PARKS", "PARTS", "PARTY", "PASTA", "PASTE", "PASTY", "PATCH", "PATIO", "PATSY", "PAUSE",
    "PAVED", "PAVES", "PAWED", "PAYER", "PEACE", "PEACH", "PEAKS", "PEARL", "PEARS", "PEDAL",
    "PEELS", "PEERS", "PENAL", "PENCE", "PENNY", "PEPPY", "PERCH", "PERIL", "PERKS", "PERKY",
    "PESKY", "PESTS", "PETAL", "PETTY", "PHASE", "PHONE", "PHONY", "PHOTO", "PIANO", "PICKS",
    "PICKY", "PIECE", "PIERS", "PIGGY", "PIKES", "PILED", "PILES", "PILLS", "PILOT", "PINCH",
    "PINED", "PINES", "PINGS", "PINKS", "PINKY", "PINTS", "PIPED", "PIPES", "PITCH", "PITHY",
    "PIVOT", "PIXEL", "PIZZA", "PLACE", "PLAID", "PLAIN", "PLANE", "PLANK", "PLANS", "PLANT",
    "PLATE", "PLAYS", "PLAZA", "PLEAD", "PLEAS", "PLIED", "PLIES", "PLOTS", "PLOWS", "PLUCK",
    "PLUGS", "PLUMB", "PLUME", "PLUMP", "PLUMS", "PLUNK", "POACH", "POEMS", "POINT", "POISE",
    "POKED", "POKER", "POKES", "POLAR", "POLES", "POLIO", "POLKA", "POLLS", "PONDS", "POOLS",
    "POPPY", "PORCH", "POSED", "POSER", "POSES", "POSSE", "POUCH", "POUND", "POURS", "POUTS",
    "PRANK", "PRAWN", "PRESS", "PRICE", "PRIDE", "PRIME", "PRINT", "PRIOR", "PRISM", "PRIZE",
    "PROBE", "PRONE", "PRONG", "PROOF", "PROPS", "PROSE", "PROUD", "PROVE", "PROWL", "PRUNE",
    "PSALM", "PUFFS", "PUFFY", "PULLS", "PULPS", "PULSE", "PUMPS", "PUNCH", "PUNKS", "PUPAE",
    "PUPIL", "PUPPY", "PURGE", "PURSE", "PUSHY", "PUTTY", "QUACK", "QUAIL", "QUAKE", "QUALM",
    "QUARK", "QUART", "QUEEN", "QUELL", "QUERY", "QUEST", "QUICK", "QUIET", "QUILL", "QUILT",
    "QUIRK", "QUITE", "QUITS", "QUOTA", "QUOTE", "RABBI", "RABID", "RACED", "RACER", "RACES",
    "RACKS", "RADAR", "RADIO", "RAFTS", "RAGES", "RAIDS", "RAILS", "RAINS", "RAINY", "RAISE",
    "RAKED", "RAKES", "RALLY", "RAMPS", "RANCH", "RANGE", "RANKS", "RAPID", "RARER", "RASPY",
    "RATED", "RATES", "RATIO", "RAVEN", "RAYON", "RAZOR", "REACH", "REACT", "READS", "READY",
    "REALM", "REAMS", "REAPS", "REARS", "REBEL", "REBUS", "RECAP", "RECUR", "REEDS", "REEFS",
    "REEKS", "REELS", "REFER", "REFIT", "REIGN", "RELAX", "RELAY", "RELIC", "RENAL", "RENEW",
    "RENTS", "REPAY", "REPEL", "REPLY", "RESET", "RESIN", "RESTS", "REUSE", "REVUE", "RHYME",
    "RIDER", "RIDES", "RIDGE", "RIFLE", "RIGHT", "RIGID", "RILED", "RILES", "RINGS", "RINSE",
    "RIOTS", "RIPEN", "RIPER", "RISEN", "RISER", "RISES", "RISKS", "RISKY", "RIVAL", "RIVER",
    "ROACH", "ROADS", "ROAMS", "ROARS", "ROAST", "ROBED", "ROBES", "ROBIN", "ROBOT", "ROCKS",
    "ROCKY", "RODEO", "ROGUE", "ROLES", "ROLLS", "ROMAN", "ROOFS", "ROOMS", "ROOMY", "ROOST",
    "ROOTS", "ROPED", "ROPES", "ROSES", "ROSIN", "ROTOR", "ROUGE", "ROUGH", "ROUND", "ROUSE",
    "ROUTE", "ROVED", "ROVER", "ROVES", "ROYAL", "RUDDY", "RUINS", "RULED", "RULER", "RULES",
    "RUMBA", "RUMMY", "RUMOR", "RUNGS", "RUNNY", "RUNTS", "RURAL", "RUSTS", "RUSTY", "SADLY",
    "SAFER", "SAGES", "SAILS", "SAINT", "SALAD", "SALES", "SALON", "SALSA", "SALTS", "SALTY",
    "SANDS", "SANDY", "SATIN", "SAUCE", "SAUCY", "SAUNA", "SAVED", "SAVER", "SAVES", "SAVOR",
    "SAWED", "SCALD", "SCALE", "SCALP", "SCAMS", "SCANS", "SCANT", "SCARE", "SCARF", "SCARS",
    "SCARY", "SCENE", "SCENT", "SCOLD", "SCONE", "SCOOP", "SCOOT", "SCOPE", "SCORE", "SCORN",
    "SCOUR", "SCOUT", "SCOWL", "SCRAP", "SCREW", "SCRUB", "SCUBA", "SCUFF", "SEALS", "SEAMS",
    "SEATS", "SEEDS", "SEEDY", "SEEKS", "SEEMS", "SEEPS", "SEIZE", "SELLS", "SENDS", "SENSE",
    "SERVE", "SETUP", "SEVEN", "SEWED", "SEWER", "SHACK", "SHADE", "SHADY", "SHAFT", "SHAKE",
    "SHAKY", "SHALE", "SHALL", "SHAME", "SHAPE", "SHARD", "SHARE", "SHARK", "SHARP", "SHAVE",
    "SHAWL", "SHEAR", "SHEDS", "SHEEN", "SHEEP", "SHEER", "SHEET", "SHELF", "SHELL", "SHIED",
    "SHIFT", "SHINE", "SHINY", "SHIPS", "SHIRK", "SHIRT", "SHOAL", "SHOCK", "SHOED", "SHOES",
    "SHONE", "SHOOK", "SHOOT", "SHORE", "SHORN", "SHORT", "SHOTS", "SHOUT", "SHOVE", "SHOWN",
    "SHOWS", "SHOWY", "SHRED", "SHREW", "SHRUB", "SHRUG", "SHUCK", "SHUNS", "SHUNT", "SHUSH",
    "SHUTS", "SHYLY", "SIDED", "SIDES", "SIEGE", "SIEVE", "SIFTS", "SIGHS", "SIGHT", "SIGNS",
    "SILKS", "SILKY", "SILLS", "SILLY", "SILOS", "SINCE", "SINKS", "SIRUP", "SITES", "SIXTH",
    "SIXTY", "SIZED", "SIZES", "SKATE", "SKEIN", "SKIED", "SKIER", "SKIES", "SKIFF", "SKILL",
    "SKIMP", "SKIMS", "SKINS", "SKIPS", "SKIRT", "SKULK", "SKULL", "SKUNK", "SLABS", "SLACK",
    "SLAIN", "SLAMS", "SLANG", "SLANT", "SLAPS", "SLASH", "SLATE", "SLATS", "SLAYS", "SLEDS",
    "SLEEK", "SLEEP", "SLEET", "SLEPT", "SLICE", "SLICK", "SLIDE", "SLIME", "SLIMY", "SLING",
    "SLINK", "SLIPS", "SLITS", "SLOGS", "SLOOP", "SLOPE", "SLOPS", "SLOSH", "SLOTH", "SLOTS",
    "SLOWS", "SLUGS", "SLUMP", "SLUMS", "SLUNG", "SLURP", "SLURS", "SLUSH", "SMACK", "SMALL",
    "SMART", "SMASH", "SMEAR", "SMELL", "SMELT", "SMILE", "SMIRK", "SMITE", "SMOCK", "SMOKE",
    "SMOKY", "SNACK", "SNAGS", "SNAIL", "SNAKE", "SNAKY", "SNAPS", "SNARE", "SNARL", "SNEAK",
    "SNIDE", "SNIFF", "SNIPE", "SNIPS", "SNOBS", "SNOOP", "SNORE", "SNORT", "SNOTS", "SNOUT",
    "SNOWS", "SNOWY", "SNUCK", "SNUFF", "SOAKS", "SOAPS", "SOAPY", "SOARS", "SOBER", "SOCKS",
    "SODAS", "SOFAS", "SOFTY", "SOGGY", "SOILS", "SOLAR", "SOLID", "SOLOS", "SOLVE", "SONGS",
    "SONIC", "SOOTH", "SOOTY", "SORER", "SORRY", "SORTS", "SOUND", "SOUPS", "SOUPY", "SOUSE",
    "SOUTH", "SOWED", "SPACE", "SPADE", "SPANS", "SPARE", "SPARK", "SPARS", "SPASM", "SPATE",
    "SPAYS", "SPEAK", "SPEAR", "SPECK", "SPECS", "SPEED", "SPELL", "SPEND", "SPENT", "SPICE",
    "SPICY", "SPIED", "SPIES", "SPIKE", "SPIKY", "SPILL", "SPILT", "SPINE", "SPINS", "SPINY",
    "SPIRE", "SPITE", "SPLAT", "SPLIT", "SPOIL", "SPOKE", "SPOOF", "SPOOK", "SPOOL", "SPOON",
    "SPORE", "SPORT", "SPOTS", "SPOUT", "SPRAY", "SPREE", "SPRIG", "SPUDS", "SPUNK", "SPURN",
    "SPURS", "SPURT", "SQUAD", "SQUAT", "SQUID", "STABS", "STACK", "STAFF", "STAGE", "STAGS",
    "STAID", "STAIN", "STAIR", "STAKE", "STALE", "STALK", "STALL", "STAMP", "STAND", "STANK",
    "STARE", "STARK", "STARS", "START", "STASH", "STATE", "STATS", "STAVE", "STAYS", "STEAD",
    "STEAK", "STEAL", "STEAM", "STEED", "STEEL", "STEEP", "STEER", "STEMS", "STENT", "STEPS",
    "STERN", "STEWS", "STICK", "STIFF", "STILE", "STILL", "STILT", "STING", "STINK", "STINT",
    "STIRS", "STOCK", "STOIC", "STOKE", "STOLE", "STOMP", "STONE", "STONY", "STOOD", "STOOL",
    "STOOP", "STOPS", "STORE", "STORK", "STORM", "STORY", "STOUT", "STOVE", "STRAP", "STRAW",
    "STRAY", "STREW", "STRIP", "STRUM", "STRUT", "STUBS", "STUCK", "STUDS", "STUDY", "STUFF",
    "STUMP", "STUNG", "STUNK", "STUNT", "STYES", "STYLE", "SUAVE", "SUEDE", "SUGAR", "SUITE",
    "SULKY", "SUMAC", "SUNNY", "SUNUP", "SUPER", "SURER", "SURFS", "SURGE", "SUSHI", "SWABS",
    "SWAGS", "SWAMI", "SWAMP", "SWANK", "SWANS", "SWAPS", "SWARD", "SWARM", "SWATS", "SWAYS",
    "SWEAR", "SWEAT", "SWEEP", "SWEET", "SWELL", "SWEPT", "SWIFT", "SWILL", "SWIMS", "SWINE",
    "SWING", "SWIPE", "SWIRL", "SWISH", "SWORD", "SWORE", "SWORN", "SWUNG", "SYRUP", "TABLE",
    "TABOO", "TACIT", "TACKY", "TACOS", "TAFFY", "TAINT", "TAKEN", "TAKER", "TAKES", "TALES",
    "TALKS", "TALLY", "TAMED", "TAMER", "TAMES", "TANGO", "TANGY", "TANKS", "TAPED", "TAPER",
    "TAPES", "TAPIR", "TARDY", "TAROT", "TARPS", "TARTS", "TASKS", "TASTE", "TASTY", "TATTY",
    "TAUNT", "TAWNY", "TAXED", "TAXES", "TAXIS", "TEACH", "TEAMS", "TEARS", "TEASE", "TEENS",
    "TEETH", "TEMPO", "TEMPT", "TENDS", "TENOR", "TENSE", "TENTH", "TENTS", "TERMS", "TESTS",
    "TEXTS", "THANK", "THAWS", "THEFT", "THEIR", "THEME", "THERE", "THESE", "THICK", "THIEF",
    "THIGH", "THINE", "THING", "THINK", "THINS", "THIRD", "THORN", "THOSE", "THREE", "THREW",
    "THROB", "THUDS", "THUGS", "THUMB", "THUMP", "TIARA", "TIDAL", "TIDED", "TIDES", "TIGER",
    "TIGHT", "TILDE", "TILED", "TILES", "TILLS", "TILTS", "TIMED", "TIMER", "TIMES", "TIMID",
    "TINGE", "TINGS", "TINNY", "TINTS", "TIRED", "TIRES", "TITAN", "TITHE", "TITLE", "TOADS",
    "TOAST", "TODAY", "TODDY", "TOFFY", "TOKEN", "TOKES", "TOLLS", "TOMBS", "TONAL", "TONED",
    "TONES", "TONGS", "TONIC", "TOOLS", "TOOTH", "TOPAZ", "TOPIC", "TOQUE", "TORCH", "TORSO",
    "TORTE", "TORUS", "TOTAL", "TOTED", "TOTES", "TOUCH", "TOUGH", "TOURS", "TOUTS", "TOWED",
    "TOWEL", "TOWER", "TOWNS", "TOXIC", "TOYED", "TRACE", "TRACK", "TRACT", "TRADE", "TRAIL",
    "TRAIN", "TRAIT", "TRAMS", "TRAPS", "TRASH", "TRAWL", "TREAD", "TREAT", "TREED", "TREES",
    "TREKS", "TREND", "TRIAD", "TRIAL", "TRIBE", "TRICE", "TRICK", "TRIED", "TRIES", "TRIMS",
    "TRIOS", "TRIPE", "TRIPS", "TRITE", "TROLL", "TROOP", "TROPE", "TROTS", "TROUT", "TRUCE",
    "TRUCK", "TRUED", "TRUER", "TRUES", "TRULY", "TRUMP", "TRUNK", "TRUSS", "TRUST", "TRUTH",
    "TRYST", "TUBAS", "TUBBY", "TUCKS", "TUFTS", "TULIP", "TUMOR", "TUNAS", "TUNED", "TUNER",
    "TUNES", "TUNIC", "TURBO", "TURFS", "TURNS", "TUSKS", "TUTOR", "TWANG", "TWEAK", "TWEED",
    "TWEET", "TWERP", "TWICE", "TWIGS", "TWINE", "TWINS", "TWIRL", "TWIST", "TWITS", "UDDER",
    "ULCER", "UNCLE", "UNCUT", "UNDER", "UNDID", "UNDUE", "UNFIT", "UNIFY", "UNION", "UNITE",
    "UNITS", "UNITY", "UNPIN", "UNSAY", "UNTIE", "UNTIL", "UNZIP", "UPEND", "UPPED", "UPPER",
    "UPSET", "URBAN", "URGED", "URGES", "URINE", "USAGE", "USERS", "USHER", "USING", "USUAL",
    "UTTER", "VAGUE", "VALET", "VALID", "VALOR", "VALUE", "VALVE", "VAPID", "VAPOR", "VASES",
    "VAULT", "VEGAN", "VEINS", "VENDS", "VENOM", "VENTS", "VENUE", "VERBS", "VERGE", "VERSE",
    "VESTS", "VEXED", "VEXES", "VIALS", "VIBES", "VICAR", "VIDEO", "VIEWS", "VIGIL", "VIGOR",
    "VILLA", "VINES", "VINYL", "VIOLA", "VIPER", "VIRAL", "VIRUS", "VISED", "VISES", "VISIT",
    "VISOR", "VITAL", "VIVID", "VOCAL", "VODKA", "VOGUE", "VOICE", "VOIDS", "VOLTS", "VOTED",
    "VOTER", "VOTES", "VOUCH", "VOWED", "VOWEL", "WACKY", "WADED", "WADES", "WAFER", "WAFTS",
    "WAGED", "WAGER", "WAGES", "WAGON", "WAIFS", "WAILS", "WAIST", "WAITS", "WAKED", "WAKEN",
    "WAKES", "WALKS", "WALLS", "WALTZ", "WANDS", "WANED", "WANES", "WARDS", "WARES", "WARMS",
    "WARNS", "WARPS", "WARTS", "WASPS", "WASTE", "WATCH", "WATER", "WAVED", "WAVER", "WAVES",
    "WAXED", "WAXES", "WEARS", "WEARY", "WEAVE", "WEDGE", "WEEDS", "WEEDY", "WEEKS", "WEEPS",
    "WEEPY", "WEIGH", "WEIRD", "WELDS", "WHACK", "WHALE", "WHARF", "WHEAT", "WHEEL", "WHELP",
    "WHERE", "WHICH", "WHIFF", "WHILE", "WHIMS", "WHINE", "WHINY", "WHIPS", "WHIRL", "WHISK",
    "WHITE", "WHOLE", "WHOOP", "WHOSE", "WICKS", "WIDEN", "WIDER", "WIDOW", "WIDTH", "WIELD",
    "WIGHT", "WIMPS", "WIMPY", "WINCE", "WINDS", "WINDY", "WINED", "WINES", "WINGS", "WINKS",
    "WIPED", "WIPER", "WIPES", "WIRED", "WIRES", "WISER", "WISPS", "WISPY", "WITCH", "WITTY",
    "WIVES", "WOKEN", "WOLFS", "WOMBS", "WOMEN", "WONKY", "WOODS", "WOODY", "WOOED", "WOOER",
    "WOOLY", "WORDS", "WORDY", "WORKS", "WORLD", "WORMS", "WORMY", "WORRY", "WORSE", "WORST",
    "WORTH", "WOULD", "WOUND", "WOVEN", "WRACK", "WRAPS", "WRATH", "WREAK", "WRECK", "WREST",
    "WRIST", "WRITE", "WRITS", "WRONG", "WROTE", "WRUNG", "WRYLY", "YACHT", "YANKS", "YARDS",
    "YARNS", "YEARN", "YEARS", "YEAST", "YELPS", "YIELD", "YOKED", "YOKES", "YOLKS", "YOUNG",
    "YOUTH", "YUMMY", "ZEBRA", "ZESTS", "ZONED", "ZONES",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_guess_marks_exact_matches_correct() {
        let feedback = score_guess("APPLE", "APPLE");
        assert!(feedback.iter().all(|f| *f == LetterFeedback::Correct));
    }

    #[test]
    fn score_guess_handles_a_duplicate_letter_in_the_secret() {
        // secret APPLE, guess ELITE -- the secret's single E is claimed by
        // the exact match at position 4, so the E at position 0 must be
        // Absent, not Present.
        let feedback = score_guess("ELITE", "APPLE");
        assert_eq!(feedback[0], LetterFeedback::Absent);
        assert_eq!(feedback[4], LetterFeedback::Correct);
    }

    #[test]
    fn score_guess_handles_a_duplicate_letter_in_the_guess() {
        // secret MELON, guess LEVEL -- only one L in the secret, so only the
        // first L (position 0) should be Present; the second (position 4)
        // must be Absent.
        let feedback = score_guess("LEVEL", "MELON");
        assert_eq!(feedback[0], LetterFeedback::Present);
        assert_eq!(feedback[1], LetterFeedback::Correct);
        assert_eq!(feedback[4], LetterFeedback::Absent);
    }

    #[test]
    fn normalize_guess_trims_and_uppercases() {
        assert_eq!(normalize_guess("  apple  ").unwrap(), "APPLE");
    }

    #[test]
    fn normalize_guess_rejects_wrong_length() {
        assert_eq!(
            normalize_guess("APPLES"),
            Err("guess must be a 5-letter word")
        );
        assert_eq!(normalize_guess("APP"), Err("guess must be a 5-letter word"));
    }

    #[test]
    fn normalize_guess_rejects_non_letters() {
        assert_eq!(
            normalize_guess("AP9LE"),
            Err("guess must be a 5-letter word")
        );
    }

    #[test]
    fn word_list_entries_are_all_five_uppercase_ascii_letters() {
        for word in WORD_LIST {
            assert_eq!(word.len(), 5, "{word} is not 5 bytes");
            assert!(
                word.chars().all(|c| c.is_ascii_uppercase()),
                "{word} isn't all-uppercase ASCII"
            );
        }
    }

    #[test]
    fn wordle_scores_ranks_by_fewest_guesses_then_fastest_solve() {
        let alice = PlayerId(0);
        let bob = PlayerId(1);
        let carol = PlayerId(2);
        let mut progress = std::collections::BTreeMap::new();
        progress.insert(
            alice,
            WordleProgress {
                guesses: vec![WordleGuess {
                    word: "APPLE".into(),
                    feedback: [LetterFeedback::Correct; 5],
                }],
                solved: true,
                elapsed_ms: Some(5000),
            },
        );
        progress.insert(
            bob,
            WordleProgress {
                guesses: vec![
                    WordleGuess {
                        word: "MELON".into(),
                        feedback: [LetterFeedback::Absent; 5],
                    },
                    WordleGuess {
                        word: "APPLE".into(),
                        feedback: [LetterFeedback::Correct; 5],
                    },
                ],
                solved: true,
                elapsed_ms: Some(3000),
            },
        );
        progress.insert(
            carol,
            WordleProgress {
                guesses: vec![WordleGuess {
                    word: "MELON".into(),
                    feedback: [LetterFeedback::Absent; 5],
                }],
                solved: false,
                elapsed_ms: None,
            },
        );
        let payload = WordlePayload {
            secret: "APPLE".into(),
            progress,
        };
        let scores = wordle_scores(&payload);
        // Alice solved in 1 guess (score 6), beats Bob's 2 guesses (score
        // 5), beats Carol's unsolved 0 -- order, not just membership,
        // matters here since resolve_ton_won relies on it.
        assert_eq!(scores, vec![(alice, 6), (bob, 5), (carol, 0)]);
    }

    #[test]
    fn wordle_scores_breaks_an_equal_guess_count_tie_by_elapsed_time() {
        let alice = PlayerId(0);
        let bob = PlayerId(1);
        let solved_in_one = |elapsed_ms| WordleProgress {
            guesses: vec![WordleGuess {
                word: "APPLE".into(),
                feedback: [LetterFeedback::Correct; 5],
            }],
            solved: true,
            elapsed_ms: Some(elapsed_ms),
        };
        let mut progress = std::collections::BTreeMap::new();
        progress.insert(alice, solved_in_one(9000));
        progress.insert(bob, solved_in_one(2000));
        let payload = WordlePayload {
            secret: "APPLE".into(),
            progress,
        };
        // Same score (both solved in 1 guess) -- Bob's faster time must
        // come first.
        assert_eq!(wordle_scores(&payload), vec![(bob, 6), (alice, 6)]);
    }
}
