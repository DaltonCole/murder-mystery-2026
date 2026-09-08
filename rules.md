# Murder Mystery 2026 — Official Rules

A live, app-assisted social-deduction party game for a masquerade-themed evening — Bridgerton-Season-4 Regency aesthetic, fluffy dresses and tuxes, actual masquerade masks. Dalton hosts and is the only person who ever sees another player's phone screen.

The whole game runs on one conceit: **a masquerade mask hides your face the same way the app hides your faction** — nobody's true allegiance is visible until it's revealed, by choice or by consequence.

Three factions are locked in a hidden three-way conflict, each hunting a different target and hiding one of their own:

- **The Ton (Aristocrat)** — high society, trying to root out the agitator undermining it.
- **The Uprising (Revolutionary)** — a movement trying to survive the night and keep its leadership intact.
- **The Cult** — a hidden third faction, secretly steering both of the above toward its own ends.

**Target headcount:** 20-30 players. **Runtime:** ~2 hours. **Structure:** 5 core rounds + 1 intermission + 1 climactic final round.

---

## 1. Setup

- **Character assignment:** every player rates their desired involvement 1–10 at signup. A rating of 6+ enters a weighted raffle for major roles (6 → 1 ticket, 7 → 5, 8 → 20, 9 → 50, 10 → 100). A rating of 5 or below can't receive a major role unless the pool is otherwise underfilled.
- **Late arrivals** become **Servants** — a separate, non-competing track with their own leaderboard and tasks, outside the three-way conflict.
- **Character creation:** Character Name, Real Name, Occupation, 5 Hobbies, 5 Notable Clothing Features, and 5 Skills — each capped at 32 characters, displayed in Pascal Case. Servants' bios feed into the shared task pool too.
- **Hidden roles:** players privately view their own faction and character by press-and-hold on their phone. Showing your screen to anyone but Dalton is against the rules.
- **Approximate faction sizes at 20-30 players:** Servants ≈ 10% of the total. The Cult starts seeded with just the Cult Leader; the rest splits roughly evenly between the Ton and the Uprising. At 20 players that's roughly 8-9 Ton, 8-9 Uprising, and a Cult growing from 1 toward ~4 by game's end; at 30 players, roughly 13/13/1→4.

---

## 2. Factions & Win Conditions

| Faction | Wins if... | Loses if... |
|---|---|---|
| **The Ton** | The Revolutionary Leader is correctly identified and Cast Out by game's end, **and** the King/Queen has not been converted to the Cult | The Leader survives uncaught, or the King/Queen ends the game converted |
| **The Uprising** | Their Leader survives to the end without being correctly Cast Out | Their Leader is Cast Out and no successor remains |
| **The Cult** | **Path A:** both the King/Queen *and* the Revolutionary Leader are converted and remain uncaught, **or** **Path B:** at least one of the two is converted, *and* the Cult Leader is personally Cast Out by public vote (the "martyrdom" path) | Neither path is achieved by game's end |

Servants sit entirely outside this three-way race.

---

## 3. The Cast

### 3.1 The Ton (Aristocrat)

| Character | Goal | Starting Knowledge | Ability |
|---|---|---|---|
| **King/Queen** | Avoid conversion | None | Once per game, before Round 5, may transfer the title to another Ton player (unmasking both the King/Queen and the Prince/Princess). The title passes to a random remaining Ton player, overwriting any existing role. |
| **Prince/Princess** ("the Heir") | Protect the King/Queen | None | Learns the King/Queen's identity after Round 2. |
| **Priest/Priestess** ("the Chaperone/Confessor") | Protect the King/Queen | None | Once per Cult recruitment window, protects one person from conversion (without knowing that's what they're protecting against). The Cult Leader can't target that person that round. Can't protect the same person twice all game. |
| **Oracle** | Find the Revolutionary Leader | None | After every odd round, views one player's full history to date, locked at that moment. Permanently disabled if the King/Queen is Cast Out (§5). |
| **Potion Maker** ("the Modiste") | Protect a target from the vote | None | Once per game, grants execution-immunity — saves whoever the public vote would Cast Out that round. |
| **The Magistrate** | Ensure the Denouncement lands correctly | None | Once per game, their ballot counts as two votes at tally. |
| **The Duelist** | Force a suspect to face judgment | None | Once per game, before nomination closes, may "challenge" one player — guaranteeing them a spot on the ballot regardless of verbal support. |
| **The Almanac** | Narrow the field by elimination | None | Once per game, privately learns 3 players who are **definitely not** the Revolutionary Leader. |
| **The Grand Inquisitor** | Press the Ton's advantage at a critical Denouncement | None | Once per game, before a ballot closes, may invoke their office: **both** of the top two vote-getters are Cast Out that round, regardless of the standard execution-count rule (§5). A one-time override of a single Denouncement's outcome. |
| **Defector** | Starts Ton, flips at intermission | — | Starts as a different Ton character; after intermission, becomes Uprising and loses the Ton role. Can never be King/Queen, Prince/Princess, or the Revolutionary Leader. |
| **Normal Ton member** | Catch-all | None | Auto-succeeds one failed social task, once per game. |

### 3.2 The Uprising (Revolutionary)

| Character | Goal | Starting Knowledge | Ability |
|---|---|---|---|
| **Revolutionary Leader** | Survive to the end | None — not even their own faction knows who this is, at the start (see §3.4) | None by default. May secretly pre-designate a successor at any time via the app; if none is set, succession defaults to a random remaining Uprising member. |
| **Bartender** (a footman/valet) | Disrupt threats to the Leader | None | Once per round, targets someone with a 50% chance of making them drunk that round (their ability fails silently if so). The target is told if drunk; the Bartender is not told whether it worked. |
| **Spymaster** | Identify threats | None | Once per game, views a single player's faction color only. |
| **Doctor/Medic** | Protect the Leader | None | Once per round, protects one person; if that person is selected for Cast-Out, their name is removed from the resolved list before slots are filled. Can't protect the same person in two consecutive rounds. |
| **The Firebrand** | Rally the Uprising's numbers | None | Once per game, their ballot counts as two votes — the Uprising's mirror to the Magistrate. |
| **The Agitator** | Protect the movement through misdirection | None | Once per game, during discussion, forces the room to spend extra time debating a different player of their choosing instead — the mirror to the Duelist. |
| **The Cell Leader** | Coordinate the rank-and-file without exposing the true Leader | Knows 2 other Uprising members' identities (never the Leader) | None beyond that knowledge. |
| **Normal Uprising member** | Catch-all | None | Once per game, ignore one vote cast against you. |

**Poison is cut entirely.** In its place, round-outcome rewards are tied directly to the Denouncement:
- **The Leader isn't among the surfaced Denouncement nominees at all:** a one-time protective/disruptive action for the Leader (vote-immunity at the next Denouncement, or a forced re-vote if the Leader is nominated later).
- **The Leader is surfaced but survives the vote:** a chosen-target yes/no query, *"is this person the Revolutionary Leader?"*
- **The Leader is correctly Cast Out:** no reward — this is the real loss condition, and succession proceeds normally.

**Mirrored Ton reward:** if a Denouncement correctly Cast Out the real Revolutionary Leader *or* an actual Cultist, the Ton receives a bonus Almanac-style clue (3 more confirmed non-Leaders), usable before the next Denouncement.

**The Leader's Confidants:** separate from the rewards above, the Revolutionary Leader slowly builds a real network over the course of the game, gated behind the Ton's own task and contest performance:
- **Task rounds (3 & 5):** if the Ton fails to hit that round's talking-task completion threshold, this fires once.
- **Contest rounds (2 & 4):** this fires once for **each individual category** (Strength, Creativity, Intelligence) the Ton loses — losing all 3 categories in one round triggers it 3 times.
- **Effect:** the app randomly selects one Uprising member the Leader doesn't already know (never the Leader themselves) and reveals identities **bi-directionally** — the Leader learns who they are, and that person learns who their Leader is.
- Self-limiting: once every Uprising member knows the Leader, further triggers have nothing left to reveal.

### 3.3 The Cult (secret faction)

| Character | Goal | Starting Knowledge | Ability |
|---|---|---|---|
| **Cult Leader** | Achieve Path A or Path B | None | Before each recruitment window, may query one candidate with a choice of *"is this person Ton-aligned?"* or *"is this person the Revolutionary Leader?"* May also designate which recruited Cultist holds the Deceiver and Whisperer titles below. |
| **Cultist** | Support the Cult Leader | Knows fellow cultists | — |
| **The Deceiver** | Protect the Cult's cover under scrutiny | Knows fellow cultists | Once per game, if targeted by another player's info-check ability (Oracle, Almanac, Spymaster, the Cult Leader's own query, the Uprising's intel query), may force that check to return a false result. |
| **The Whisperer** | Shield a fellow cultist from exposure | Knows fellow cultists | Once per game, may shield one named fellow Cultist from being a valid nomination target for one round. |

**Assigning Deceiver and Whisperer:** only the Cult Leader exists at game start. As recruitment brings in new Cultists, the Cult Leader personally designates who holds each title — at the moment of recruitment or any point after. Once assigned, a title stays with that Cultist for the rest of the game.

**Recruitment schedule:** at 20 players or fewer, the Cult recruits one new member every 2 rounds, flat, for the whole game. **At 21-30 players**, that same flat cadence holds through Round 3, but from Round 4 onward, each recruitment window brings in **2 new members instead of 1**.

- The Cult secretly aids whichever public faction is currently behind, each round.
- If the King/Queen is converted before using their title-transfer ability, that ability auto-fires, and the Prince/Princess is swept into the Cult too. No equivalent cascade applies to the Revolutionary Leader.
- A converted player keeps their original character and abilities, letting them keep fooling their original side.

### 3.4 Revolutionary Leader secrecy

The Leader's identity is unknown even to their own faction at the start of the game — a deliberate choice, narratively grounded in how real underground movements organize in secretive cells. It slowly erodes over the game specifically through **The Leader's Confidants** (§3.2) — a controlled, gated path tied to the Ton's own performance.

Nothing in the app or rules can actually *prevent* a Leader from telling a trusted friend out loud at the party — that risk exists regardless of any mechanic. **State the following explicitly at the start of the game**, alongside the phone-privacy rule: *the Revolutionary Leader may never voluntarily disclose their identity to anyone, including fellow Uprising members, for any reason.*

---

## 4. Round Structure

**Round 1** (intro) → **Round 2** (contest) → **Round 3** (task + Denouncement) → **Round 4** (contest) → **Intermission** → **Round 5** (task + Denouncement) → **The Last Denouncement** (finale).

### Round 1 — soft intro, no vote
1. Dalton gives a short scripted intro (the masquerade conceit, the Ton and the Uprising named, the Cult only hinted at, the phone-privacy rule, and the Leader-secrecy rule above) and live-demos the press-and-hold reveal on a dummy screen.
2. Everyone privately reveals their character.
3. The app pushes exactly 2 fixed tasks (1 easy, 1 medium); players mingle and self-report by naming 3 people they talked to.
4. No nomination, no vote — the round ends on the timer. Completion rate is tallied silently, for Whistledown flavor only.

### Rounds 3 & 5 — task rounds with a Denouncement
1. **Task phase**, easy/medium/hard tiers live.
2. The app locks submissions and privately resolves any pending info-abilities (Oracle, Almanac, the Uprising's intel query) to their owners.
3. **Silent pre-nomination:** everyone privately submits one name via the app; the app surfaces only the top 2-3 most-named players on a shared screen.
4. **Discussion**, scoped to just the surfaced names, on a shared timer that Dalton starts but doesn't moderate. If multiple people are up for Cast-Out that round, they're all debated together in the same window.
5. **Secret ballot** among the surfaced names, plus an explicit abstain option — public tally only, never individual votes.
6. **Resolution:** Dalton reads the Cast-Out name(s) aloud; each privately sees their own full reveal on their own phone and moves to the Servant section. Everyone else gets the same uninformative public result, every time (§6).
7. Whistledown auto-publishes.

**Time budget:** nomination 2 min, discussion 3-4 min, ballot 90 sec, resolution 90 sec — roughly 8 minutes of vote-phase on top of the task phase, landing the whole round around 18-20 minutes.

### Contest rounds (2, 4)
Split into 3 zones — Strength, Creativity, Intelligence — roughly 7-10 people per zone. Staff each zone's scoring with an already-Cast-Out player from an earlier round.

### Intermission
"Who is Lorel's number one love?" — anyone Cast Out earlier is ineligible to enter. Entrants are capped at 4-5 (first come via the app, or a quick random draw) so the bit doesn't consume the whole break.

### The Last Round — "The Last Denouncement"
One final nomination → discussion → vote, with a longer discussion window than the mid-game rounds. This time, **whoever is Cast Out has their full character and faction revealed publicly** on a shared screen. Immediately after, Dalton walks through all three win conditions and reveals everything that happened privately all game — conversions, successions, any martyrdom trigger.

---

## 5. The Denouncement

In-fiction, this mechanic is never called an "execution" or a "vote" — to players, it's **The Denouncement**, and losing it means being **Cast Out**.

- **Nomination:** silent app-based pre-nomination surfaces the top 2-3 candidates.
- **Discussion:** a shared window covering all surfaced candidates together, hard-capped.
- **The ballot:** cast privately via app (a "calling card" laid against a name) — secret ballot, public tally only.
- **Ties:** only nominees tied *at* the last available slot trigger a runoff (30-second final statement each, then a re-vote among just those tied). Anyone clearly above the cutoff is already locked in. A repeat tie leaves that specific slot unfilled.
- **Abstaining** is allowed and doesn't count toward the tally.
- **How many are Cast Out per round:** 1 execution at ≤20 players remaining; 2 simultaneous executions at 21-30 players remaining (`ceil(players ÷ 15)` beyond that). The Grand Inquisitor can override this once per game, forcing a 2-for-1 Denouncement regardless of headcount.
- **The Magistrate/Firebrand double-vote**, in a multi-slot round, adds one extra vote to whichever single nominee that player supported — it doesn't split across multiple nominees.

### Resolution by target

- **The Revolutionary Leader, correctly identified:** the title passes immediately and privately to a successor. Never announced — the room only ever learns "someone was Cast Out."
- **The King/Queen, Cast Out (not converted):** allowed, no immunity. Permanently closes off Cult Path A, and permanently disables the Oracle for the rest of the game — a real cost, but not a loss condition; the Ton can still win.
- **The Cult Leader, Cast Out:** if a royal conversion had already landed, this triggers Cult Path B — the martyrdom path — resolved privately and immediately, revealed publicly only at the finale. *(Because nobody outside the Cult can ever be sure a conversion has already happened, Casting Out a suspected Cult Leader always carries real risk — the "safe" choice and the "correct" choice aren't always the same thing.)*
- **A regular Cultist, an innocent bystander, or anyone else:** simply removed. No faction or role is ever confirmed publicly for any Cast-Out player, including a correctly-caught Cultist — every result gets the same deliberately uninformative public treatment (§6). Full transparency is saved entirely for the Last Denouncement.

### What happens to a Cast Out player
They fold into the **Servant track**: their app reveals their own full role and history for closure, they're locked out of future nominations/votes/abilities, but they can keep playing contest rounds — as a participant or as a zone scorekeeper — and get the **Gallery** role for the finale (§7).

---

## 6. Theme & Narrative

### Lady Whistledown's Society Papers
Publishes after **every** round, not just Denouncement rounds. Every Denouncement result gets exactly the same level of ambiguity — no template ever correlates with who was actually Cast Out. Rotate randomly among a few stylistically different but equally uninformative templates, purely for flavor variety:

> *"Dearest reader, the Ton awoke to shocking news — Lord [Name] was cast from society's good graces at last night's gathering, denounced by the very peers who once toasted his health. Whether justice was served or a grave error made, this author cannot say."*

> *"Society lost one of its own last night, and this author confesses genuine sorrow at the loss — though whether the room's judgment was righteous or rash, only time (and perhaps a guilty conscience) will tell."*

> *"A most curious turn at last night's gathering — [Name], cast out before the assembled Ton, protested their innocence to the last. This author has heard such protests before. Sometimes they are even true."*

**Every-round teaser** (for rounds without a Denouncement):
> *"The Ton dances on, unaware — or is it unwilling? — of what stirs beneath the ballroom floor."*

### The martyrdom moment
The instant the Cult Leader is Cast Out with a royal conversion already in place, only the convert sees a private, immediate message, followed by a plain statement of their new win condition — never a public announcement:

> *"Your hands are still trembling from the vote. You did not know — could not have known — the weight [Cult Leader's name] carried, or the promise you made them in confidence. And yet their final words to you echo louder now than any denouncement: 'Should I fall, you will finish what we began.'"*

### Framing
The mechanic is **"The Denouncement."** Losing it means being **"Cast Out"** — mask removed, escorted out of the ballroom, seated with the Servants. Nominating someone is **"laying a calling card against them."**

---

## 7. Running the Game (host notes)

- **Nomination is app-driven, not open-floor** — everyone privately submits one name; only the top 2-3 get surfaced for discussion.
- **Discussion is player-run.** Dalton starts a visible shared timer and lets the room self-organize; he doesn't moderate who speaks.
- **Hard-cap every phase via the app** — nomination, discussion, ballot, and resolution each get their own visible timer.
- **Recruit already-Cast-Out players as working staff:** zone scorekeepers for contest rounds, and Gallery participants for the finale.
- **The Gallery:** before the Last Denouncement's ballot closes, every already-Cast-Out player submits one private prediction via the app (who gets Cast Out, or which faction ultimately wins), scored against the Servant leaderboard.
- **Display the tally, not the ballots** — vote counts per nominee only, never who voted for whom.
- **Opening speech checklist:** phone-privacy rule, live demo of the press-and-hold reveal, the masquerade conceit, the Ton and Uprising named (Cult only hinted at), and the Revolutionary Leader secrecy rule (§3.4).
