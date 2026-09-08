# Murder Mystery 2026 — Game Report

*v2 — updated for an expected 20-30 players. Built from `idea.md` and Dalton's amendments across two synthesis passes: the original 4-fork pass (round structure, character roster, balance math, theme & live-ops) that established the "Ton / Revolutionary Leader / Martyrdom" direction, and this second 4-fork pass (roster scaling, structural fault-finding + round detail, balance math redone at 20-30 players, timing realism & endgame pacing) that stress-tests it at real event scale. This is a readable "how the game plays" document, not a decision log — `idea.md` remains the working history. Anywhere this report made a judgment call, it's flagged; the genuinely open, high-stakes decisions are collected in §11.*

---

## 1. Overview

Murder Mystery 2026 is a live, app-assisted social-deduction party game for a masquerade-themed evening — Bridgerton-Season-4 Regency aesthetic, fluffy dresses and tuxes, actual masquerade masks. Dalton hosts and is the only person who ever sees another player's phone screen. The whole game runs on the central conceit that **a masquerade mask hides your face the same way the app hides your faction** — nobody's true allegiance is visible until it's revealed, by choice or by consequence.

Three factions are locked in a hidden three-way conflict, each hunting a different target and hiding one of their own:

- **The Ton (Aristocrat)** — high society, trying to root out the agitator undermining it.
- **The Uprising (Revolutionary)** — a movement trying to survive the night and keep its leadership intact.
- **The Cult** — a hidden third faction, secretly steering both of the above toward their own ends.

**Target headcount: 20-30 players.** Total runtime: ~2 hours. **5 core rounds** (down from 6 — see §5 and §11) + 1 intermission + 1 climactic final round.

---

## 2. Setup

- **Character assignment:** every player rates their desired involvement 1–10 at signup. 6+ enters a weighted raffle for major roles (6=1 ticket, 7=5, 8=20, 9=50, 10=100 tickets); 5-and-under can't get a major role unless the pool is underfilled. Late arrivals become **Servants** — a separate, non-competing track (own leaderboard, own tasks, doesn't affect the three-way conflict).
- **Character creation:** Character Name, Real Name, and a Bio (Occupation, 5 Hobbies, 5 Notable Clothing Features, 5 Skills) — all capped at 32 characters, displayed in Pascal Case. Servants' bios feed into the task pool too, so latecomers are still woven into the game other players interact with.
- **Hidden roles:** players privately view their own faction/character by press-and-hold on their phone; showing your screen to anyone but Dalton is against the rules.
- **Working faction-sizing assumption for a 20-30 player event** (stated explicitly since it drives §4's roster math): Servants ≈ 10% of total. The Cult starts seeded with just the Cult Leader; the remaining players split roughly evenly between Aristocrat and Revolutionary. At N=20 that's roughly 8-9 Aristocrats, 8-9 Revolutionaries, and a Cult growing from 1 toward ~4 by game's end; at N=30, roughly 13/13/1→4.

---

## 3. The Three Factions & Win Conditions

| Faction | Wins if... | Loses if... |
|---|---|---|
| **The Ton (Aristocrat)** | The Revolutionary Leader is correctly identified and Cast Out (§6) by game's end, **and** the King/Queen has not been converted to the Cult | The Revolutionary Leader survives uncaught, or the King/Queen ends the game converted |
| **The Uprising (Revolutionary)** | Their Leader survives to the end of the game without being correctly Cast Out | Their Leader is Cast Out and no successor remains to carry the title |
| **The Cult** | **Path A:** both the King/Queen *and* the Revolutionary Leader are converted and remain uncaught by game's end, **OR** **Path B:** at least one of the two is converted, *and* the Cult Leader is personally Cast Out by public vote (see §7.3 — this is the "martyrdom" path) | Neither path is achieved by game's end |

Each of the two public factions has a hidden figurehead the other is hunting, and the Cult profits from chaos on either side. Servants remain entirely outside this three-way race. **See §10 — at 20-30 players this symmetry does not currently translate into balanced odds, and that needs Dalton's attention.**

---

## 4. The Full Cast

The roster below is expanded from the original design specifically to give more named roles at 20-30 players — the original cast (6 Aristocrat, 4 Revolutionary, 2 Cult roles) left too many players on a generic catch-all at this scale. New roles are marked **NEW**.

### 4.1 The Ton (Aristocrat)

| Character | Goal | Starting Knowledge | Ability |
|---|---|---|---|
| **King/Queen** | Avoid conversion | None | Once per game, before round 5, may transfer the title to another Aristocrat (unmasking both King/Queen and Prince/Princess; the new King/Queen inherits the title, overwriting any prior role) |
| **Prince/Princess (the "Heir")** | Protect the King/Queen | None | Learns the King/Queen's identity after round 2 |
| **Priest/Priestess (the "Chaperone/Confessor")** | Protect the King/Queen | None | Once per cult-recruitment window, protects one person from conversion (without knowing that's what they're protecting against); the Cult Leader can't target that person that round; can't protect the same person twice all game |
| **Oracle** | Find the Revolutionary Leader | None | After every odd round, views one player's full history to date, locked at that moment. Framed explicitly as the Ton's dedicated Leader-hunting tool |
| **Potion Maker ("the Modiste")** | Protect a target from the vote | None | Once per game, grants execution-immunity: saves whoever the public vote would Cast Out that round |
| **The Magistrate** *(NEW)* | Ensure the Denouncement lands correctly | None | Once per game, their ballot counts as two votes when the tally is taken |
| **The Duelist** *(NEW)* | Force a suspect to face judgment | None | Once per game, before nomination closes, may "challenge" one player — that player is guaranteed a spot on the ballot regardless of how much verbal support they got |
| **The Almanac** *(NEW)* | Narrow the field by elimination | None | Once per game, privately learns 3 players who are **definitely not** the Revolutionary Leader — a breadth clue, distinct from Oracle's single-target depth clue, that gets more useful as the suspect pool grows |
| **Defector** | Starts Aristocrat, flips at intermission | — | Starts as a different Aristocrat character; after intermission, becomes Revolutionary and loses the Aristocrat role. Can never be King/Queen, Prince/Princess, **or the Revolutionary Leader** |
| **Normal Aristocrat** | Catch-all | None | Auto-succeeds one failed social task, once per game |

### 4.2 The Uprising (Revolutionary)

| Character | Goal | Starting Knowledge | Ability |
|---|---|---|---|
| **Revolutionary Leader** | Survive to the end | None — **not even their own faction knows who this is** (see §4.4 for why, and an important caveat right after it) | None by default. May secretly pre-designate a successor at any time via the app; if none is set, succession defaults to a random remaining Revolutionary |
| **Bartender (a footman/valet)** | Disrupt threats to the Leader | None | Once per round, targets someone with a 50% chance of making them drunk that round (their ability fails silently if so; the target is told, the Bartender isn't) |
| **Spymaster** | Identify threats | None | Once per game, views a single player's faction color only |
| **Doctor/Medic** | Protect the Leader | None | Once per round, protects one person; if that person is selected for Cast-Out, their name is simply removed from the resolved list before slots are filled (works cleanly even in a multi-slot round — see §6). Can't protect the same person in two consecutive rounds |
| **The Firebrand** *(NEW)* | Rally the Uprising's numbers | None | Once per game, their ballot counts as two votes — a direct mirror of the Magistrate. This pairing is deliberate, not a coincidence: matched powers on both sides are easier to reason about and keep the "symmetric hunt" design principle intact |
| **The Agitator** *(NEW)* | Protect the movement through misdirection | None | Once per game, during discussion, forces the room to spend extra time debating a different player of their choosing instead — the Uprising's defensive mirror to the Duelist |
| **The Cell Leader** *(NEW)* | Coordinate the rank-and-file without exposing the true Leader | Knows 2 other Revolutionaries' identities (**never** the Leader themselves) | None beyond that knowledge — cheap social glue that reinforces the "secret cells" fiction behind the Leader's isolation, without ever compromising it |
| **Normal Revolutionary** | Catch-all | None | Once per game, ignore one vote cast against you |

**Poison is cut entirely** (it existed only to serve the old "kill the King/Queen" win condition). Round-outcome rewards are repurposed instead: **winning** a round grants a one-time protective/disruptive action benefiting the Leader (vote-immunity for the next Denouncement, or a forced re-vote if the Leader is nominated); **losing** a round grants a chosen-target yes/no query, **"is this person the Revolutionary Leader?"**

### 4.3 The Cult (secret faction)

| Character | Goal | Starting Knowledge | Ability |
|---|---|---|---|
| **Cult Leader** | Achieve Path A or Path B (§3) | None | Before each recruitment window, may query one candidate with a choice of **either** "is this person Aristocrat-aligned?" **or** "is this person the Revolutionary Leader?" |
| **Cultist** | Support the Cult Leader | Knows fellow cultists | — |

- Recruitment stays on a fixed schedule (~1 new cultist per 2 rounds), decoupled from round outcomes. **§10 flags this fixed schedule as a real problem at 20-30 players — the Cult doesn't grow or search any faster in a bigger room, so its relative strength shrinks. See §11.**
- The cult secretly aids whichever public faction is currently behind, each round.
- The King/Queen-conversion cascade is unchanged: if King/Queen is converted before using their title-transfer ability, it auto-fires, and the Prince/Princess is swept into the cult too. No equivalent cascade applies if the Revolutionary Leader is converted.
- A converted player keeps their original character and abilities, fooling their original side exactly as before.

**Two more Cult roles were designed but are deliberately NOT adopted here — see §11:** a "Deceiver" (can force one info-check targeting them to return a false result) and a "Whisperer" (can shield a fellow Cultist from nomination for a round). Both are plausible at a bigger endgame Cult (4+ members), but the Deceiver specifically would degrade the reliability of *every* info-check in the game (Oracle, Spymaster, the Cult Leader's own query, the Uprising's intel query) — that's a big enough ripple effect to need Dalton's explicit sign-off, not a default inclusion.

### 4.4 Why the Revolutionary Leader's identity is secret from their own team

Narratively, it's well-motivated for free (real underground movements organize in cells for exactly this reason). For mystery symmetry, it gives the Uprising's own rank-and-file a real "who is it?" question too, not just the Ton. For balance, it's close to load-bearing: if Revolutionary teammates knew their Leader, they could coordinate votes to shield them, worsening an already-favorable Revolutionary survival rate (§10).

**Important caveat:** nothing in the app or rules actually *enforces* this secrecy — a Leader could simply tell a trusted friend out loud at the party. The design leans on this isolation holding by social convention, not by a mechanic. This isn't something to "fix" so much as something to be aware of: Dalton should understand that some real-world leakage is likely, and the balance numbers in §10 represent a best case, not a guarantee.

---

## 5. How a Round Works

**Structure (revised — 5 core rounds, not 6):** Round 1 (intro) → Round 2 (contest) → Round 3 (task + Denouncement) → Round 4 (contest) → Intermission → Round 5 (task + Denouncement) → The Last Denouncement (finale). Two independent analyses — one on raw timing, one on narrative shape — separately concluded the original 6-round structure doesn't fit a 20-30 player night, and that the correct cut is the *third contest round*, not a Denouncement: the votes are the actual spine of the game, contests are the breathers between them. This preserves all 3 planned Denouncements (rounds 3, 5, and the finale) while removing one of the three "just for fun" rounds.

### Round 1 — soft intro, no vote
1. Dalton gives a short scripted intro (masquerade conceit, the Ton and the Uprising named, the Cult only hinted at, the phone-privacy rule) and **live-demos the press-and-hold reveal** on a dummy screen so nobody fumbles their first real one.
2. Everyone privately reveals their character.
3. The app pushes exactly 2 fixed tasks (1 easy, 1 medium); players mingle and self-report via the existing "name 3 people you talked to" mechanic.
4. No nomination, no vote — the round just ends on the timer. Completion rate is tallied silently, for Whistledown flavor only.
5. At 20-30 people, physically finding and talking to people takes longer than at 16 — budget this round toward the high end of your available time, and tell players plainly that nothing is at stake yet.

### Rounds 3 & 5 — task rounds with a Denouncement
1. **Task phase** (as Round 1, but easy/medium/hard tiers live).
2. App locks submissions, privately resolves any pending info-abilities (Oracle, Almanac, the Uprising's intel query) to their owners, and shows Dalton a single "safe to proceed" cue — he doesn't need to track who has a pending result.
3. **Silent pre-nomination (new — replaces pure open-floor nomination):** everyone privately submits one name via the app; the app surfaces only the top 2-3 most-named players on a shared screen. *(Open floor with 25-30 people fragments into side-conversations and buries quieter voices — this gives the room a finite, visible target instead of an unbounded free-for-all.)*
4. **Discussion**, scoped to just the surfaced names, on a shared timer Dalton starts but doesn't moderate. If multiple people are up for Cast-Out that round (§6), they're all debated together in this same window, not one at a time.
5. **Secret ballot** among the surfaced names, plus an explicit abstain option — public tally only, never individual votes.
6. **Resolution:** Dalton reads the Cast-Out name(s) aloud; each one privately sees their own full reveal on their own phone and moves to the Servant section. Everyone else gets only the same deliberately uninformative public result, every time — see §8.1 for why that consistency matters.
7. Whistledown auto-publishes, timed to land as the room resettles.

**Time budget** (hard-capped via the app, not left open-ended): nomination 2 min, discussion 3-4 min (a bit more if multiple nominees are live), ballot 90 sec, resolution 90 sec — roughly 8 min of vote-phase on top of the task phase, landing the whole round around 18-20 minutes at 20-30 players. This is an explicit tradeoff — not everyone who wants to speak gets to — and it's worth Dalton stating that plainly at the top of the night rather than leaving it implicit.

### Even rounds (2, 4) — contests
Splitting 20-30 people into 3 zones (Strength / Creativity / Intelligence) actually works *better* at this scale than at 16 — roughly 7-10 people per zone is a good size for the existing bracket, judged, and trivia formats. Recommended addition: **staff each zone's scoring with an already-Cast-Out player** from an earlier round — it solves a real operational need and gives eliminated players something concrete to do, which matters more at this scale (see §9's Gallery mechanic for the same idea applied to the finale).

### Intermission
The existing "Lorel's number one love" bit, with two additions: anyone Cast Out earlier is ineligible to enter (a cheap, thematic consequence), and **entrants are capped** — at 45 seconds each, more than 4-5 entrants eats the whole break. Cap via the app (first N to opt in, or a quick random draw) rather than letting it run open-ended.

### The Last Round — "The Last Denouncement"
One final nomination → discussion → vote, at higher stakes and with a longer discussion window than the mid-game rounds. This time, **whoever is Cast Out has their full character and faction revealed publicly** on a shared screen — the masquerade's actual unmasking. Immediately after, Dalton walks through all three win conditions and reveals everything that happened privately all game (conversions, successions, any martyrdom trigger) — this is the one moment full transparency is the whole point. See §9 for the Gallery mechanic that gives already-eliminated players a real stake in this moment.

---

## 6. The Denouncement (the vote mechanic)

In-fiction, this is never called an "execution" or a "vote" — to players, it's **The Denouncement**, and losing it means being **Cast Out**.

- **Nomination:** a silent app-based pre-nomination surfaces the top 2-3 candidates (§5) — not a true open floor at this headcount.
- **Discussion:** shared window covering all surfaced candidates together, hard-capped (§5).
- **The ballot:** cast privately via app (a "calling card" laid against a name) — secret ballot, public tally only.
- **Ties:** only nominees tied *at* the last available slot trigger a runoff (30-second final statement each, then a re-vote among just those tied); anyone clearly above the cutoff is already locked in. A repeat tie means that specific slot simply goes unfilled — it doesn't cancel the rest of the round's result.
- **Abstaining** is allowed and doesn't count toward the tally.
- **How many Cast Out per round — concrete scaling rule** (replaces the old placeholder): **1 execution at ≤20 players remaining; 2 simultaneous executions at 21-30 players remaining.** (For groups that ever exceed 30, the general rule is `ceil(players ÷ 15)`.) This keeps the removal rate proportional across the target range rather than jumping all at once at an arbitrary threshold.
- **The Magistrate/Firebrand double-vote**, in a multi-slot round: the ability adds one extra vote to whichever single nominee that player supported — it doesn't split across multiple nominees and doesn't affect other slots.

### What happens when different people are Cast Out
- **The Revolutionary Leader, correctly identified:** the title passes immediately and privately to a successor. **Never announced.** The room only ever learns "someone was Cast Out."
- **The King/Queen, Cast Out (not converted):** allowed, no immunity. Closes off Cult Path A. Doesn't cost the Ton their win on its own — see §11 for the option to change that.
- **The Cult Leader, Cast Out:** if a royal conversion had already landed, this is the martyrdom trigger (§3 Path B) — resolved privately and immediately (§8.2), revealed publicly only at the finale.
- **A regular Cultist, an innocent bystander, or anyone else:** simply removed. **No faction or role is ever confirmed publicly for any Cast-Out player, including a correctly-caught Cultist** — every result gets the same deliberately uninformative public treatment (§8.1). This was a deliberate fix made during this pass: the original design implied a "was this someone important?" tell might leak through which Whistledown template got used, which would work against the game's own stated goal of keeping every Denouncement outcome equally ambiguous to the room. Full transparency is saved entirely for the Last Denouncement's public reveal.

### What happens to a Cast Out player
Folds into the existing **Servant track**: their app reveals their own full role/history for closure, they're locked out of future nominations/votes/abilities, but they keep playing even-round contests (as a participant or, per §5, as a zone scorekeeper) and — critically at this scale — get the new **Gallery** role for the finale (§9).

---

## 7. How the Factions Interact

### 7.1 A symmetric hunt
The Ton hides a King/Queen the Cult wants to corrupt, and hunts a Revolutionary Leader they need to expose. The Uprising hides its Leader from everyone, including itself, and quietly protects them. The Cult sits underneath both, feeding on whichever side is losing. Every faction has something to protect and something to hunt.

### 7.2 The information economy
- **The Ton** now has three lead-generation tools: Oracle (depth — one player's full history), the new Almanac (breadth — 3 confirmed non-Leaders), and whatever surfaces in open debate.
- **The Uprising** has the Spymaster (once per game) and the round-loss intel query, both aimed at a mystery even they don't have the answer to.
- **The Cult** still has the sharpest single tool — a repeatable, choose-your-question scouting ability — but **§10 shows this doesn't scale with player count the way the other two factions' hunting does**, since it's tied to round count, not headcount.

### 7.3 The martyrdom paradox — the game's best moment
Once the table suspects someone might be the Cult Leader, denouncing them can backfire — if a royal target's already converted, executing the Cult Leader is exactly what hands the Cult the game. Nobody outside the Cult can ever be sure a conversion has landed, so the "correct" play and the "safe" play are genuinely in tension. This should be preserved deliberately — no public warning system, no safety valve.

### 7.4 Whistledown as connective tissue
Because so much of this game happens in private, **Lady Whistledown's Society Papers** is what gives the table a shared, unreliable narrative. **Change made this pass:** Whistledown now publishes a short item after *every* round, not just odd/Denouncement rounds — a cheap fix for a real risk identified this pass (the Cult can otherwise go quiet for long stretches and fade from the room's attention; see §9).

---

## 8. Theme & Narrative

### 8.1 Lady Whistledown, in practice

**Design correction made this pass:** every Denouncement result gets *exactly the same level of ambiguity*, with zero variation tied to who was actually Cast Out — no separate "someone important fell" template. The original two-tier template idea (a routine one, and a distinct one for significant losses) was flagged this pass as a real leak: over just 3 Denouncements with a handful of possible important targets, players would quickly learn to read which template fired as a signal, undermining the entire "uncertainty is the mechanic" premise in §7.3.

Instead: **rotate randomly among 3-4 stylistically different but equally uninformative templates**, purely for flavor variety — the choice of template is cosmetic, never correlated with the target's importance.

> *"Dearest reader, the Ton awoke to shocking news — Lord [Name] was cast from society's good graces at last night's gathering, denounced by the very peers who once toasted his health. Whether justice was served or a grave error made, this author cannot say."*

> *"Society lost one of its own last night, and this author confesses genuine sorrow at the loss — though whether the room's judgment was righteous or rash, only time (and perhaps a guilty conscience) will tell."*

> *"A most curious turn at last night's gathering — [Name], cast out before the assembled Ton, protested their innocence to the last. This author has heard such protests before. Sometimes they are even true."*

Every-round teaser (new, for endgame pacing — see §9):
> *"The Ton dances on, unaware — or is it unwilling? — of what stirs beneath the ballroom floor."*

### 8.2 The martyrdom moment, privately
Unchanged from the prior pass: the instant the Cult Leader is Cast Out with a royal conversion already in place, only the convert sees a private, immediate message, followed by a plain statement of their new win condition. Never a public announcement — surfaces only at the final reveal.

> *"Your hands are still trembling from the vote. You did not know — could not have known — the weight [Cult Leader's name] carried, or the promise you made them in confidence. And yet their final words to you echo louder now than any denouncement: 'Should I fall, you will finish what we began.'"*

### 8.3 Framing the vote itself
The mechanic is **"The Denouncement."** Losing it means being **"Cast Out"** — mask removed, escorted out of the ballroom, seated with the Servants. Nominating is **"laying a calling card against them."**

---

## 9. Running the Game (host notes)

- **The discussion phase is player-run,** but at this scale nomination should not be. Replace true open-floor nomination with the silent app-based pre-nomination step (§5, §6) — faster, fairer to quieter players, and gives the room a finite, visible target for debate instead of chaos.
- **Hard-cap every phase via the app** — nomination, discussion, ballot, and resolution all get their own visible timer. This isn't just about the debate phase anymore (as in the prior pass); at 20-30 players every phase needs a ceiling or the round budget doesn't hold.
- **Recruit already-Cast-Out players as working staff:** zone scorekeepers for even-round contests (§5), and see the Gallery mechanic below for the finale. This isn't just flavor — it's the actual fix for the biggest new risk this pass identified: a potentially large group of eliminated players with nothing to do by the end of the night.
- **The Gallery (new mechanic, addresses the endgame "spectator pile-up" risk):** at 20-30 players with multi-slot Denouncements, a realistic 3-9 players could be Cast Out by the final round — up to roughly a third of the room, sidelined for the single biggest moment of the night. Before the Last Denouncement's ballot closes, every already-Cast-Out player submits one private prediction via the app (who gets Cast Out, or which faction ultimately wins) — scored against the existing Servant leaderboard. It costs nothing structurally new (reuses private app input and the Servant scoring track already in the design) and turns a large passive audience into an invested one for the game's climax.
- **Tie-break rule, decided in advance:** see §6 — only the nominees tied at the cutoff re-vote; a repeat tie leaves that slot unfilled, not the whole round.
- **Display the tally, not the ballots.**

---

## 10. Balance Analysis — recomputed at 20-30 players, with a correction

**Correction to the previous pass's math, caught during this synthesis:** the original §10 modeled Revolutionary Leader survival using only 2 Denouncements (rounds 3 and 5), even though the Last Round is also a real Denouncement where the Leader could still be caught if they've survived that long. The numbers below correctly use **3 total Denouncements** (rounds 3, 5, and the finale) as the baseline "as-designed" cadence.

### Revolutionary Leader survival, naive/near-random baseline

Using P(a specific player is never selected across M draws from a shrinking population of N) = (N−M)/N:

| N | M=3 (as designed: R3, R5, finale) | M=5 (a vote every core round) |
|---|---|---|
| 16 *(prior pass's assumption)* | 81.3% | 68.8% |
| 20 | 85.0% | 75.0% |
| 25 | 88.0% | 80.0% |
| 30 | 90.0% | 83.3% |

**A bigger player pool makes this worse, not better, at a fixed number of votes** — more players simply means more decoys. Even the more aggressive "vote every round" scenario stays far above the 30% target at this scale. Closing this gap requires genuinely strong detection (Oracle, the new Almanac, sharp debate), not just more attempts — vote frequency alone cannot get there, and this is now a *bigger* problem than it was in the 16-player version of this analysis.

### The Cult gets weaker at scale, not stronger — a new finding this pass

Both Cult win paths are budgeted by **round count**, not headcount — recruitment stays fixed at ~1 new member per 2 rounds regardless of whether there are 16 or 30 players in the room. That means:
- The Cult's proportional size (and with it, the strength of its "help whichever side is behind" rubber-band) shrinks from roughly a quarter of the room at N=16 to roughly an eighth at N=30.
- **Path A** (double-conversion): the scouting ability's odds of ever correctly identifying either hidden target scale roughly like *(queries available) ÷ N* — with the same ~3 queries across the game regardless of N, this drops from ~19% at N=16 to ~10% at N=30.
- **Path B** (martyrdom): already self-limiting (a correct "cult execution" only has roughly 1-in-cult-size odds of specifically hitting the Leader), and gets no easier at scale since cult size stays roughly fixed near 4 regardless of N — while the town's own targeting gets noisier with more innocent bystanders to sort through.

### Net verdict at 20-30 players

**Revolutionary ≫ The Ton > The Cult** — a more lopsided picture than the original 16-player analysis, and now the Cult specifically undershoots 30% rather than just Revolutionary overshooting it. This needs deliberate compensation beyond what the prior pass already called for. See §11 for the concrete open decisions this creates.

---

## 11. Open Decisions for Dalton

### Adopted this pass (flagging for awareness, not asking permission)
- The roster expansion in §4 (Magistrate, Duelist, Almanac, Firebrand, Agitator, Cell Leader).
- The 5-round structure (§5) — cutting one contest round, keeping all 3 Denouncements.
- The scaled multi-execution rule (§6): 1 execution ≤20 players, 2 at 21-30.
- Uniform, non-leaking Whistledown templates (§8.1) and no mid-game faction confirmation on any Cast-Out (§6).
- The Gallery mechanic (§9) and every-round Whistledown cadence (§7.4/§8.1).

### Still genuinely open — need your call
1. **Denouncement frequency/strength vs. balance (sharper now than before).** Even 3 votes across the game leaves Revolutionary heavily favored at 20-30 players (§10). Options: (a) accept it for a first playtest and lean hard on Oracle/Almanac/debate quality; (b) add a 4th Denouncement (e.g., attach one to Round 1 or Round 2 instead of keeping it purely an intro/contest round); (c) strengthen Aristocrat's hunting tools further before touching vote frequency at all.
2. **Should the Cult's recruitment/scouting pace scale with headcount** rather than staying fixed at ~1-per-2-rounds regardless of N? This is the most direct fix for the Cult's under-tuning at scale (§10). Options: (a) scale recruitment rate with player count (e.g., recruit every round instead of every 2 rounds above some N threshold); (b) give the Cult Leader more scouting queries at higher N instead of faster recruitment; (c) leave it as-is and accept the Cult is a minor faction at this scale.
3. **Should the Deceiver/Whisperer Cult roles (§4.3) be adopted?** They'd help address finding #2 above (a stronger endgame Cult), but the Deceiver specifically undermines every info-check in the game — a bigger ripple effect than any other single addition in this report.
4. **Does King/Queen's survival matter to the Ton's win**, or only their conversion status (§6)? Unchanged open question from the prior pass.
5. **Revolutionary Leader secrecy has no real enforcement mechanism** (§4.4) — worth knowing going in, not something with clean options to choose between.

---

## 12. What Changed From the Pre-Amendment Design (original pass)

- Revolutionary's win condition is no longer "kill the King/Queen" — it's "the Leader survives."
- A new **Revolutionary Leader** character exists, secret even from their own faction.
- The standalone **poison mechanic is cut**; round-outcome rewards repurposed around the Leader instead.
- **Potion Maker** grants execution-immunity instead of curing poison; **Doctor/Medic** finalized as the Uprising's protector.
- **Oracle** reframed as the Ton's Leader-hunting tool; **the Accusation mechanic retired entirely**, absorbed by the Denouncement.
- The **Last Round** became "The Last Denouncement" instead of an open question.
- The **Cult Leader** gained a second win path (martyrdom).

## 13. What Changed This Pass (20-30 Player Scaling)

- **6 new named characters** added across Aristocrat and Revolutionary (§4.1, §4.2); two more designed but deliberately not adopted (§4.3, §11).
- **Round count cut from 6 to 5** core rounds (§5) — removed one contest round, kept all 3 Denouncements intact.
- **Concrete multi-execution scaling rule** replacing the old placeholder (§6).
- **Open-floor nomination replaced** with a silent app-based pre-nomination step at this headcount (§5, §6, §9).
- **Uniform Whistledown treatment** for every Cast-Out — closes an information leak identified this pass (§6, §8.1).
- **The Gallery mechanic** — gives potentially-large groups of eliminated players a real stake in the finale (§9).
- **Balance math corrected and rerun** at N=20/25/30 — surfaced a new finding (the Cult gets weaker, not stronger, at scale) on top of the existing Revolutionary-survival risk (§10).
