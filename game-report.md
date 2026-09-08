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
| **The Grand Inquisitor** *(NEW — added per §11 item 1)* | Press the Ton's advantage at a critical Denouncement | None | Once per game, before a ballot closes, may invoke their office: **both** of the top two vote-getters are Cast Out that round, not just one — regardless of what the standard headcount-based execution count (§6) would otherwise call for. Doesn't cost a separate action elsewhere; this is a one-time override of a single Denouncement's outcome |
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

**Poison is cut entirely** (it existed only to serve the old "kill the King/Queen" win condition). **Round-outcome rewards were retargeted per §11 item 1** — they now trigger off *Denouncement* results specifically, not contest-round wins/losses, since contests have nothing to do with the Leader hunt and Dalton asked for rewards tied more directly to the Denouncement itself:
- **The Leader isn't among the surfaced nominees at all** (the cleanest outcome for the Uprising): a one-time protective/disruptive action benefiting the Leader (vote-immunity for the next Denouncement, or a forced re-vote if the Leader is nominated later).
- **The Leader is surfaced but survives the vote** (a close call): the chosen-target yes/no query, **"is this person the Revolutionary Leader?"** — framed in-fiction as a lesson learned from the near miss.
- **The Leader is correctly Cast Out:** no reward — this is the real loss condition, and succession proceeds as normal (§6).

**A mirrored reward exists for the Ton**, addressing the same request: if a Denouncement correctly Cast Out the real Revolutionary Leader *or* an actual Cultist (i.e., the vote hit a real hidden target, not an innocent bystander), the Ton receives a bonus Almanac-style clue (3 more confirmed non-Leaders) usable before the next Denouncement — stacking with any player's own Almanac use.

**The Leader's Confidants (new mechanic — Dalton's decision):** separate from the Denouncement-tied rewards above, the Revolutionary Leader slowly builds a real network over the course of the game, triggered by the Ton's *task and contest* performance specifically:
- **Trigger, task rounds (3 & 5):** if the Ton fails to hit that round's talking-task completion threshold (§6.1) — i.e., they don't get enough of their people talking to N others with the right characteristics — this fires once for the round.
- **Trigger, contest rounds (2 & 4):** this fires once for **each individual category** (Strength, Creativity, Intelligence) the Ton loses that round, not once per round — a Ton that loses all 3 categories in a single contest round triggers this 3 times.
- **What happens:** the app randomly selects one Revolutionary the Leader doesn't already know (never the Leader themselves) and reveals identities **bi-directionally** — the Leader learns who that Revolutionary is, *and* that Revolutionary learns the Leader is their Leader. This is a deliberate, one-way-only-getting-bigger erosion of the Leader's anonymity (§4.4), unlike the unintended verbal-leakage risk described there — it's designed to happen, just gated behind the Ton actually underperforming.
- **Self-limiting:** once every Revolutionary already knows the Leader, further triggers have nothing left to reveal.
- **Balance note, stated plainly rather than left implicit:** this stacks directly on top of the Revolutionary-favored risk already flagged in §10 — the worse the Ton does at tasks/contests, the more Revolutionaries get let in on their Leader's identity, which (per §4.4) makes coordinated shielding easier right when the Ton can least afford it. That's a real rubber-band effect, not a flaw exactly — it mirrors how the Cult already reinforces whichever side is behind (§4.3) — but it means a struggling Ton could compound its own trouble. Worth watching for in a playtest rather than something this report is second-guessing on Dalton's behalf.

### 4.3 The Cult (secret faction)

| Character | Goal | Starting Knowledge | Ability |
|---|---|---|---|
| **Cult Leader** | Achieve Path A or Path B (§3) | None | Before each recruitment window, may query one candidate with a choice of **either** "is this person Aristocrat-aligned?" **or** "is this person the Revolutionary Leader?" May also designate which recruited Cultist holds the Deceiver and Whisperer titles below — see below |
| **Cultist** | Support the Cult Leader | Knows fellow cultists | — |
| **The Deceiver** *(NEW — adopted per §11 item 3)* | Protect the Cult's cover under scrutiny | Knows fellow cultists | Once per game, if targeted by another player's info-check ability (Oracle, Almanac, Spymaster, the Cult Leader's own query, the Uprising's intel query), may force that check to return a false result |
| **The Whisperer** *(NEW — adopted per §11 item 3)* | Shield a fellow cultist from exposure | Knows fellow cultists | Once per game, may shield one named fellow Cultist from being a valid nomination target for one round |

**How Deceiver and Whisperer get assigned:** only the Cult Leader exists at game start — there's no one to hold these titles until recruitment produces someone. **Per Dalton's decision (§11 item 3), the Cult Leader personally designates which recruited Cultist holds each title**, at the moment of recruitment or at any later point. A title, once assigned, stays with that Cultist for the rest of the game (no reassigning mid-game — keeps this simple to track). Each title can only be held by one Cultist at a time; with only ~4 Cultists expected by game's end (§2), the Cult Leader is effectively choosing 2 of their eventual ~3 recruits to specialize, which is itself a real strategic decision worth having them make deliberately rather than assigning at random.

- **Recruitment now scales with headcount and ramps up over the course of the game — a direct change per §11 item 2** (previously a flat ~1 new cultist per 2 rounds regardless of N, which §10 found made the Cult proportionally weaker at bigger events, not stronger). New schedule: at 21-30 players, the Cult still recruits on the original cadence through the first Denouncement (Round 3), but from Round 4 onward, each subsequent recruitment window brings in **2 new members instead of 1**. This both scales the Cult's endgame size to the room and gives it a rising, back-loaded momentum — narratively fitting (a movement gaining converts as the night's tensions escalate) and mechanically useful (it directly counters the "Cult goes quiet" endgame-fizzle risk flagged in §7.4/§9, since the Cult is now doing more, not less, as the game closes). At ≤20 players, the original flat cadence is retained — the under-tuning problem was specifically a 21-30-player issue.
- The cult secretly aids whichever public faction is currently behind, each round.
- The King/Queen-conversion cascade is unchanged: if King/Queen is converted before using their title-transfer ability, it auto-fires, and the Prince/Princess is swept into the cult too. No equivalent cascade applies if the Revolutionary Leader is converted.
- A converted player keeps their original character and abilities, fooling their original side exactly as before.


### 4.4 Why the Revolutionary Leader's identity is secret from their own team

Narratively, it's well-motivated for free (real underground movements organize in cells for exactly this reason). For mystery symmetry, it gives the Uprising's own rank-and-file a real "who is it?" question too, not just the Ton. For balance, it's close to load-bearing: if Revolutionary teammates knew their Leader, they could coordinate votes to shield them, worsening an already-favorable Revolutionary survival rate (§10).

**This secrecy is a starting state, not a permanent one — by design.** The Leader's Confidants mechanic (§4.2) deliberately erodes it over the course of the game as a reward for the Ton's own poor task/contest performance. That's an intentional, gated erosion path, distinct from the *unintended* leakage risk below — the difference matters: one only grows when the Ton is already struggling (a rubber-band the game controls), the other can happen at any time regardless of how anyone's actually playing (a hole the game can't control at all).

**Important caveat, expanded per §11 item 5:** nothing in the app or rules actually *enforces* this secrecy — a Leader could simply tell a trusted friend out loud at the party. Why this specifically matters, in more detail:

- **The mechanism is trivially available.** Unlike the Cult, whose coordination happens through an app-mediated group chat that at least nominally exists inside the game's systems, Revolutionary teammates are standing in the same room all night with no barrier to just talking. A nervous Leader, once they sense they're under suspicion, has every incentive to quietly recruit protectors — and nothing stops them.
- **It's a more dangerous leak than ordinary hidden-role "meta-gaming."** In most social deduction games, a bit of informal signaling between teammates is a minor edge. Here, it directly attacks the specific mechanism §10's balance math already flags as the game's single biggest risk: if even 2-3 informed Revolutionaries deliberately steer nominations and discussion away from their Leader and toward decoys, that's a *deliberate, coordinated* version of the "wasted execution" dilution already baked into the naive survival numbers — meaning real-world play could land even further above the already-too-high 85-93% survival baseline in §10, not below it.
- **It can't be monitored or policed the way other hidden information is.** Phones-hidden-from-everyone-but-Dalton (§2) is enforceable because it's a physical, observable rule. A private conversation between two guests at a party is not something the game — or Dalton — can ever verify or prevent.
- **A partial, non-mechanical mitigation is available and worth adopting anyway:** state it as an explicit table rule at the top of the night, the same way "never show your phone" already is — something like *"the Revolutionary Leader may never voluntarily disclose their identity to anyone, including fellow Revolutionaries, for any reason."* This won't be enforceable any more than any other honor-system rule in a hidden-role game (nothing stops someone from peeking at a neighbor's cards in any card game either), but naming the expectation explicitly, out loud, at the table measurably reduces how often groups actually break it in practice, compared to leaving it as an unstated assumption. Recommend Dalton add this to the Round 1 rules speech alongside the phone-privacy rule it's modeled on.

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
- **How many Cast Out per round — concrete scaling rule** (replaces the old placeholder): **1 execution at ≤20 players remaining; 2 simultaneous executions at 21-30 players remaining.** (For groups that ever exceed 30, the general rule is `ceil(players ÷ 15)`.) This keeps the removal rate proportional across the target range rather than jumping all at once at an arbitrary threshold. **The Grand Inquisitor (§4.1) can override this rule once per game, forcing a 2-for-1 Denouncement even in a round that would otherwise only take one — added specifically to give the Ton more Denouncement pressure, per §11 item 1.**
- **The Magistrate/Firebrand double-vote**, in a multi-slot round: the ability adds one extra vote to whichever single nominee that player supported — it doesn't split across multiple nominees and doesn't affect other slots.

### What happens when different people are Cast Out
- **The Revolutionary Leader, correctly identified:** the title passes immediately and privately to a successor. **Never announced.** The room only ever learns "someone was Cast Out."
- **The King/Queen, Cast Out (not converted):** allowed, no immunity, and **now carries a real cost to the Ton — resolved per §11 item 2.** Losing the King/Queen this way permanently disables the Oracle's ability for the rest of the game (in-fiction: high society loses its nerve, and the Oracle's sources stop confiding in her without a monarch to protect). This is deliberately a single, clean, memorable penalty rather than a game-ending one — the Ton can still win by correctly denouncing the Revolutionary Leader, just with one fewer hunting tool for however much of the game remains. Losing the King/Queen this way still closes off Cult Path A permanently, same as before.
- **The Cult Leader, Cast Out:** if a royal conversion had already landed, this is the martyrdom trigger (§3 Path B) — resolved privately and immediately (§8.2), revealed publicly only at the finale.
- **A regular Cultist, an innocent bystander, or anyone else:** simply removed. **No faction or role is ever confirmed publicly for any Cast-Out player, including a correctly-caught Cultist** — every result gets the same deliberately uninformative public treatment (§8.1). This was a deliberate fix made during this pass: the original design implied a "was this someone important?" tell might leak through which Whistledown template got used, which would work against the game's own stated goal of keeping every Denouncement outcome equally ambiguous to the room. Full transparency is saved entirely for the Last Denouncement's public reveal.

### What happens to a Cast Out player
Folds into the existing **Servant track**: their app reveals their own full role/history for closure, they're locked out of future nominations/votes/abilities, but they keep playing even-round contests (as a participant or, per §5, as a zone scorekeeper) and — critically at this scale — get the new **Gallery** role for the finale (§9).

---

## 7. How the Factions Interact

### 7.1 A symmetric hunt
The Ton hides a King/Queen the Cult wants to corrupt, and hunts a Revolutionary Leader they need to expose. The Uprising hides its Leader from everyone, including itself, and quietly protects them. The Cult sits underneath both, feeding on whichever side is losing. Every faction has something to protect and something to hunt.

### 7.2 The information economy
- **The Ton** now has four lead-generation tools: Oracle (depth — one player's full history, though it goes dark permanently if King/Queen is Cast Out, §6), the Almanac (breadth — 3 confirmed non-Leaders), a bonus Almanac-style clue on a correct Denouncement (§4.2), and whatever surfaces in open debate — backed by the Grand Inquisitor's ability to force a bigger Denouncement when the moment calls for it.
- **The Uprising** has the Spymaster (once per game), a Denouncement-triggered intel query (§4.2), and — growing only when the Ton stumbles — the Leader's Confidants network (§4.2), which slowly turns "a mystery even they don't have the answer to" into a small, real circle the Leader can actually trust.
- **The Cult now scales with the room** (§4.3, §11 item 2) — recruitment ramps up from Round 4 onward at 21-30 players instead of staying flat regardless of headcount, and the Deceiver (§4.3) means every one of the above tools carries a real, if rare, risk of returning a deliberately false result once the Cult has grown large enough to have someone holding that title.

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

**Revolutionary ≫ The Ton > The Cult** — a more lopsided picture than the original 16-player analysis, and now the Cult specifically undershoots 30% rather than just Revolutionary overshooting it. This needed deliberate compensation beyond what the prior pass already called for.

### Compensating changes adopted in response (§11)

Dalton reviewed this verdict and made four decisions that directly target it — implemented throughout §4 and §6, summarized here:
- **The Grand Inquisitor** (§4.1) gives the Ton a one-time way to force a bigger Denouncement, adding real pressure beyond the fixed M=3 baseline above without adding a whole new round.
- **Denouncement-tied round rewards** (§4.2) give both sides a tighter, more frequent feedback loop specifically around the Leader hunt, rather than rewards tied to unrelated contest outcomes.
- **Scaled, ramping Cult recruitment** (§4.3) directly targets the "Cult gets weaker at scale" finding above — the Cult should end the game meaningfully larger at 21-30 players than the fixed old schedule produced, especially in the back half.
- **The Deceiver and Whisperer** (§4.3) give the now-bigger endgame Cult real defensive tools it didn't have before, making its two win paths less purely a numbers game.
- **A real King/Queen-death penalty for the Ton** (§6 — Oracle goes permanently dark) — this doesn't directly touch the Revolutionary-survival numbers above, but it closes a separate asymmetry Dalton flagged: previously King/Queen dying cost the Ton nothing at all.

**This report has not re-run the survival/win-rate math against these specific changes** — that would require a further modeling pass (the Grand Inquisitor and the ramping recruitment schedule both need real assumptions about *when* and *how often* they fire in practice, which is better informed by an actual playtest than by more speculative math on top of already-speculative math). Recommend treating the table above as the "before" picture, these five changes as a good-faith attempt at the "after," and validating with a real playtest before doing another full numeric pass.

---

## 11. Decisions

### Adopted in the scaling pass (flagging for awareness, not asking permission)
- The roster expansion in §4 (Magistrate, Duelist, Almanac, Firebrand, Agitator, Cell Leader).
- The 5-round structure (§5) — cutting one contest round, keeping all 3 Denouncements.
- The scaled multi-execution rule (§6): 1 execution ≤20 players, 2 at 21-30.
- Uniform, non-leaking Whistledown templates (§8.1) and no mid-game faction confirmation on any Cast-Out (§6).
- The Gallery mechanic (§9) and every-round Whistledown cadence (§7.4/§8.1).

### Resolved by Dalton — his decisions, as given, with where each landed in the doc

1. **Denouncement frequency/strength vs. balance.**
    > *"Possibly add an Ton role that allows the top two voted on people to be denounced? I do believe we need more denouncements. Possibly make the round rewards more tied to denouncements."*
    * **Implemented:** the Grand Inquisitor (§4.1, §6) — a one-time Ton ability that forces a 2-for-1 Denouncement. Round-outcome rewards for both Revolutionary and the Ton were retargeted to trigger off Denouncement results instead of contest outcomes (§4.2).
2. **Cult recruitment/scouting pace.**
    > *"I believe it should scale with headcount, ramping up in later rounds."*
    * **Implemented:** at 21-30 players, recruitment stays on the original cadence through Round 3, then brings in 2 new members per window from Round 4 onward (§4.3).
3. **Deceiver/Whisperer Cult roles.**
    > *"Let us add these roles for now. Only the cult leader starts the game, but the cult leader may give these titles to cultists."*
    * **Implemented:** both roles adopted (§4.3). The Cult Leader personally designates which recruited Cultist holds each title, permanently once assigned.
4. **Does King/Queen's death matter to the Ton's win?**
    > *"Having the king/queen die should heavily matter to the Ton. Some type of extreme drawback, but not too extreme, the Ton should still be able to somehow win."*
    * **Implemented:** if King/Queen is Cast Out (not converted), the Ton's Oracle permanently stops working for the rest of the game — a real, memorable cost, but not a loss condition (§6). **Flagging for Dalton:** this is this report's own specific choice of *which* drawback to use — "heavily matter, but not too extreme" had several possible implementations (a stricter win-margin requirement, losing a different tool, etc.). If disabling the Oracle specifically isn't the penalty you had in mind, this is an easy one-line swap; the important thing was landing on *some* real, single, clearly-stated cost, which this does.
5. **Revolutionary Leader secrecy risk.**
    > *"Expand on why this is an issue."*
    * **Done:** §4.4 now explains the mechanism (no barrier to verbal collusion, unlike the Cult's app-mediated chat), why it specifically compounds the survival-math risk in §10, why it can't be policed the way other hidden information can, and proposes a non-mechanical mitigation (an explicit spoken table rule, modeled on the existing phone-privacy rule) worth adding to the Round 1 intro speech.

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

- **6 new named characters** added across Aristocrat and Revolutionary (§4.1, §4.2); two more (Deceiver, Whisperer) initially held back pending Dalton's sign-off — see §14 for their subsequent adoption.
- **Round count cut from 6 to 5** core rounds (§5) — removed one contest round, kept all 3 Denouncements intact.
- **Concrete multi-execution scaling rule** replacing the old placeholder (§6).
- **Open-floor nomination replaced** with a silent app-based pre-nomination step at this headcount (§5, §6, §9).
- **Uniform Whistledown treatment** for every Cast-Out — closes an information leak identified this pass (§6, §8.1).
- **The Gallery mechanic** — gives potentially-large groups of eliminated players a real stake in the finale (§9).
- **Balance math corrected and rerun** at N=20/25/30 — surfaced a new finding (the Cult gets weaker, not stronger, at scale) on top of the existing Revolutionary-survival risk (§10).

## 14. What Changed From Dalton's §11 Review

Dalton reviewed the open decisions from the scaling pass and answered all five directly in §11. Implemented as a result:

- **The Grand Inquisitor** (§4.1) — a new Ton character who can force a 2-for-1 Denouncement once per game.
- **Denouncement-tied round rewards** (§4.2) for both Revolutionary and the Ton, replacing the old contest-tied trigger.
- **Scaled, ramping Cult recruitment** at 21-30 players (§4.3) — flat through Round 3, then 2 new members per window from Round 4 on.
- **The Deceiver and Whisperer** (§4.3) fully adopted, with the Cult Leader personally designating which recruit holds each title.
- **A real King/Queen-death penalty for the Ton** (§6) — the Oracle permanently stops working if King/Queen is Cast Out (not converted).
- **§4.4 expanded** with a fuller explanation of the Revolutionary Leader secrecy risk and a proposed (non-mechanical) mitigation.
- **§10 updated** with a "compensating changes" note tying these five decisions back to the balance verdict that prompted them, and an explicit flag that the numbers haven't been re-modeled against these specific changes yet — that's a job for a real playtest, not more speculative math.

### 14.1 Addendum — The Leader's Confidants

A follow-up conversation refined how the Revolutionary Leader's isolation (§4.4) should erode over the game: rather than the whole faction gradually learning about each other, **only the Leader** builds a growing, trusted network — one revealed ally at a time, and the reveal is **bi-directional** (both learn who the other is). This was deliberately narrowed from an earlier, broader "everyone learns about everyone" idea specifically because faction-wide coordination would have compounded Revolutionary's existing survival advantage (§10); a Leader-only network is a smaller, more contained risk.

**Trigger, per Dalton's decision:** tied to Ton underperformance on tasks and contests specifically — a failed task-round threshold, or each individual contest category lost — not to Denouncement outcomes. Implemented in §4.2, with a cross-reference added to §4.4 distinguishing this *designed* erosion path from the *unintended* verbal-leakage risk already documented there, and a plain flag that this does stack on top of §10's existing balance concern (a struggling Ton faces a more coordinated Revolutionary) rather than being a hidden cost.
