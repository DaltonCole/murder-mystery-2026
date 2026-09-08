# Murder Mystery 2026 — Game Report

*A complete design synthesis of the "Ton / Revolutionary Leader / Martyrdom" direction, built out from `idea.md` and Dalton's latest amendment by four parallel design passes (round structure, character roster, balance math, theme & live-ops). This is a readable "how the game plays" document, not a decision log — `idea.md` remains the working history. Anywhere this report had to make a judgment call to produce a coherent whole, it's flagged explicitly, and the genuinely open, high-stakes decisions are collected in §11 for Dalton to resolve.*

---

## 1. Overview

Murder Mystery 2026 is a live, app-assisted social-deduction party game for a masquerade-themed evening — Bridgerton-Season-4 Regency aesthetic, fluffy dresses and tuxes, actual masquerade masks. Dalton hosts and is the only person who ever sees another player's phone screen. The whole game runs on the central conceit that **a masquerade mask hides your face the same way the app hides your faction** — nobody's true allegiance is visible until it's revealed, by choice or by consequence.

Three factions are locked in a hidden three-way conflict, each hunting a different target and hiding one of their own:

- **The Ton (Aristocrat)** — high society, trying to root out the agitator undermining it.
- **The Uprising (Revolutionary)** — a movement trying to survive the night and keep its leadership intact.
- **The Cult** — a hidden third faction, secretly steering both of the above toward their own ends.

Total runtime: 2 hours. 6 core rounds (15–18 min each) + 1 intermission + 1 climactic final round.

---

## 2. Setup (unchanged from the base design)

- **Character assignment:** every player rates their desired involvement 1–10 at signup. 6+ enters a weighted raffle for major roles (6=1 ticket, 7=5, 8=20, 9=50, 10=100 tickets); 5-and-under can't get a major role unless the pool is underfilled. Late arrivals become **Servants** — a separate, non-competing track (own leaderboard, own tasks, doesn't affect the three-way conflict).
- **Character creation:** Character Name, Real Name, and a Bio (Occupation, 5 Hobbies, 5 Notable Clothing Features, 5 Skills) — all capped at 32 characters, displayed in Pascal Case. Servants' bios feed into the task pool too, so latecomers are still woven into the game other players interact with.
- **Hidden roles:** players privately view their own faction/character by press-and-hold on their phone; showing your screen to anyone but Dalton is against the rules.

---

## 3. The Three Factions & Win Conditions

| Faction | Wins if... | Loses if... |
|---|---|---|
| **The Ton (Aristocrat)** | The Revolutionary Leader is correctly identified and Cast Out (§6) by game's end, **and** the King/Queen has not been converted to the Cult | The Revolutionary Leader survives uncaught, or the King/Queen ends the game converted |
| **The Uprising (Revolutionary)** | Their Leader survives to the end of the game without being correctly Cast Out | Their Leader is Cast Out and no successor remains to carry the title |
| **The Cult** | **Path A:** both the King/Queen *and* the Revolutionary Leader are converted and remain uncaught by game's end, **OR** **Path B:** at least one of the two is converted, *and* the Cult Leader is personally Cast Out by public vote (see §7.3 — this is the "martyrdom" path) | Neither path is achieved by game's end |

This is a genuine change from the original design, where Revolutionary's sole win condition was "kill the King/Queen." That's gone — Revolutionary is now purely defensive (protect your own hidden leader), which makes the whole structure symmetric: **each of the two public factions has a hidden figurehead the other is hunting**, and the Cult profits from chaos on either side. Servants remain entirely outside this three-way race.

---

## 4. The Full Cast

Everything below reflects what survived, what got repurposed, and what's brand new after folding the amendment into the existing roster. Nearly the entire original cast survived — most of it via reframing, not replacement.

### 4.1 The Ton (Aristocrat)

| Character | Goal | Starting Knowledge | Ability | Status |
|---|---|---|---|---|
| **King/Queen** | Avoid conversion | None | Once per game, before round 5, may transfer the title to another Aristocrat (unmasking both King/Queen and Prince/Princess; the new King/Queen inherits the title, overwriting any prior role) | Unchanged — the ability's *purpose* shifted from escaping assassination (gone) to escaping conversion (still the only real threat) |
| **Prince/Princess (the "Heir")** | Protect the King/Queen | None | Learns the King/Queen's identity after round 2 | Unchanged |
| **Priest/Priestess (the "Chaperone/Confessor")** | Protect the King/Queen | None | Once per cult-recruitment window, protects one person from conversion (without knowing that's what they're protecting against); the Cult Leader can't target that person that round; can't protect the same person twice all game | Unchanged |
| **Oracle** | Find the Revolutionary Leader | None | After every odd round, views one player's full history to date, locked at that moment | **Repurposed.** This ability was always a generic intel tool — it's now framed explicitly as the Ton's dedicated tool for hunting the Leader (voting patterns, behavior, alliances). This is a free win: it gives Aristocrat its first real symmetric counterpart to the Cult Leader's scouting ability, and it retires the old complaint that Oracle and Accusation did the same job — Accusation is gone (§6.5), so there's no more overlap. |
| **Potion Maker ("the Modiste")** | Protect a target from the vote | None | Once per game, grants execution-immunity: saves whoever the public vote would Cast Out that round | **Repurposed** from "cures poison" — poison is cut (see 4.2), so this slots the Modiste directly into the game's new central mechanic instead of a dying one. |
| **Defector** | Starts Aristocrat, flips at intermission | — | Starts as a different Aristocrat character; after intermission, becomes Revolutionary and loses the Aristocrat role. Can never be King/Queen, Prince/Princess, **or the Revolutionary Leader** | Unchanged, with the Leader added to its existing exclusion list |
| **Normal Aristocrat** | Catch-all | None | Auto-succeeds one failed social task, once per game | Unchanged — task rounds still exist (§5) |

### 4.2 The Uprising (Revolutionary)

| Character | Goal | Starting Knowledge | Ability | Status |
|---|---|---|---|---|
| **Revolutionary Leader** *(new)* | Survive to the end | None — **not even their own faction knows who this is** | None by default — deliberately powerless, mirroring King/Queen. May secretly pre-designate a successor at any time via the app (changeable until it fires); if none is set, succession defaults to a random remaining Revolutionary | **Brand new.** See §4.4 for why the secrecy is load-bearing, not flavor. |
| **Bartender (a footman/valet)** | Disrupt threats to the Leader | None | Once per round, targets someone with a 50% chance of making them drunk that round (their ability fails silently if so; the target is told, the Bartender isn't) | Unchanged mechanically — reframed from "assist the poisoning plot" to general disruption |
| **Spymaster** | Identify threats | None | Once per game, views a single player's faction color only | Unchanged — reframed from "find the King/Queen" to "identify a threat to the Leader" |
| **Doctor/Medic** | Protect the Leader | None | Once per round, protects one person; if that person is selected for Cast-Out, the vote is voided/postponed instead. Can't protect the same person in two consecutive rounds | **Finalized** — was "proposed, not decided" in the old design; this amendment gives it an obvious, load-bearing job (the Uprising's answer to Priest/Priestess) |
| **Normal Revolutionary** | Catch-all | None | Once per game, ignore one vote cast against you | **New, resolves an old open question** ("is there a Normal Revolutionary catch-all analogous to Normal Aristocrat?" — yes, this is it) |

**Cut:** the standalone poison mechanic and the win/loss-triggered poison-vote / intel-reveal loop, in their original form — both existed purely to serve "kill the King/Queen," which no longer exists. In their place, the round-outcome rewards are repurposed to serve the new goal directly:
- **Win a round →** a one-time protective or disruptive action benefiting the Leader (e.g., grant the Leader vote-immunity for the next Denouncement, or force a re-vote if the Leader gets nominated).
- **Lose a round →** the existing "choose a target, get a yes/no answer" intel query, now asking **"is this person the Revolutionary Leader?"** — genuinely useful given the Leader's identity is secret even from their own side.

### 4.3 The Cult (secret faction)

| Character | Goal | Starting Knowledge | Ability |
|---|---|---|---|
| **Cult Leader** | Achieve Path A or Path B (§3) | None | Before each recruitment window, may query one candidate with a choice of **either** "is this person Aristocrat-aligned?" **or** "is this person the Revolutionary Leader?" — one query, either question, same cost as before |
| **Cultist** | Support the Cult Leader | Knows fellow cultists | — |

- Recruitment stays on its existing fixed schedule (~1 new cultist per 2 rounds), decoupled from round outcomes.
- The cult still secretly aids whichever public faction is currently behind, each round.
- The King/Queen-conversion cascade is unchanged: if King/Queen is converted before using their title-transfer ability, it auto-fires, and the Prince/Princess is swept into the cult too. **No equivalent cascade applies if the Revolutionary Leader is converted** — nobody else automatically knows the Leader's identity closely enough to be swept in, so this asymmetry is intentional, not an oversight.
- A converted player keeps their original character and abilities, fooling their original side exactly as before.

### 4.4 Why the Revolutionary Leader's identity is secret from their own team

This is the single most important new design decision in this report, and it wasn't obvious from the amendment text — two independent analyses (character design and balance math) converged on it from completely different angles, which is why it's presented here as a locked recommendation rather than an open question:

- **Narratively**, it's well-motivated for free: real underground movements often *do* organize in secretive cells specifically so no single capture unravels the whole network — the Leader running through anonymous intermediaries fits the "Uprising" fiction naturally.
- **For mystery symmetry**, it means every faction has a real "who is it?" question hanging over it — not just the Aristocrats hunting the Leader, but the Uprising's own rank-and-file too, giving the Spymaster and the loss-triggered intel query a genuine job.
- **For balance**, this is close to load-bearing rather than optional flavor: if Revolutionary teammates *did* know their Leader, they could coordinate votes to shield them, which — per the survival math in §10 — would push an already-favorable Revolutionary win rate even further out of balance.

---

## 5. How a Round Works

### Round 1 (soft intro, special round)
Unchanged from the base design: a light 2-task round (1 easy, 1 medium "talk to X" task) with no vote, so a first-time group can learn the app before anything is at stake.

### Rounds 3 & 5 (task rounds with a Denouncement)
These carry both the existing talking-task content *and* the new public vote, back to back:

| Time | Phase |
|---|---|
| 0:00–8:00 | Task-round mingling: players complete "talk to X" tasks (easy/medium/hard tiers), self-reported via the app |
| 8:00–9:00 | App closes tasks, resolves any info abilities that feed the coming discussion (Oracle lookup, the Uprising's intel query) |
| 9:00–10:30 | Open nomination — anyone can name a suspect aloud |
| 10:30–13:00 | Open discussion/debate on the nominee(s) |
| 13:00–14:00 | Secret ballot cast via app (see §6) |
| 14:00–15:00+ | Resolution announced, Whistledown headline auto-publishes |

This is a tight fit at 15 minutes — realistically these two rounds should run **17–18 minutes**, borrowed from elsewhere in the 2-hour budget, rather than compressing the task or debate phases further.

### Even rounds (2, 4, 6) — unchanged
Pure contest rounds: Strength, Creativity, Intelligence categories as already designed (bracket, judged contests, trivia). No vote — these stay as breathers between the tenser odd rounds, and they're the one place the whole room does something together rather than splitting into deduction mode.

### Intermission
The existing "Lorel's number one love" bit, with one small addition: **anyone Cast Out earlier in the night is ineligible to enter** — a cheap, thematic consequence (you're no longer welcome in decent company) that costs nothing new to build.

### The Last Round — "The Last Denouncement" (resolves the old open Last-Round question)
Rather than inventing a fourth new system for the finale, the climax reuses the same mechanic the whole game has trained players on, played once more at higher stakes: one final nomination → discussion → vote, except this time **whoever is Cast Out has their full character and faction revealed publicly** — the masquerade's actual unmasking. Immediately after, all three factions' win conditions are checked and the full result — including anything that happened privately mid-game, like a martyrdom trigger — is revealed for the first time.

---

## 6. The Denouncement (the vote mechanic)

In-fiction, this is never called an "execution" or a "vote" — to players, it's **The Denouncement**, and losing it means being **Cast Out**.

- **Nomination:** open floor, spoken aloud — the drama lives in real-time accusation, not a silent form.
- **Discussion:** roughly 2.5 minutes of open debate before the ballot closes.
- **The ballot:** cast privately via app (a "calling card" laid against a name, reusing a gesture already established elsewhere in the design) — a **secret ballot with a public tally** (you see vote totals per nominee, never who voted for whom). This keeps the public drama in the debate, not the vote itself, and stays consistent with the game's whole identity of routing every meaningful action through a private phone interaction.
- **Ties:** a short re-vote between only the tied nominees, 30 seconds' final statement each; a second tie means no one is Cast Out that round.
- **Abstaining** is allowed and doesn't count toward the tally.
- **How many Cast Out per round:** one by default; two simultaneously once 16+ players remain at the start of that round. *(This threshold is a placeholder pending real headcount — see §11.)*

### What happens when different people are Cast Out

- **The Revolutionary Leader, correctly identified:** the title passes immediately and privately to a successor (pre-designated, or random if none was set). **This is never announced.** The room only ever learns "someone was Cast Out" — not that it was the real Leader, and not that a new one now exists. Whistledown's coverage is deliberately ambiguous either way (see §8.1), so the fact of succession can't be reverse-engineered from which headline template fires.
- **The King/Queen, Cast Out (not converted):** allowed — no special immunity. This permanently closes off Cult Path A (can't convert someone no longer in the game). As written, this does **not** cost the Ton their win, since their win condition only requires King/Queen to avoid *conversion*, not to survive the vote — see §11 for the one-line option to change that if Dalton wants King/Queen's survival to matter too.
- **The Cult Leader, Cast Out:** if a royal conversion had already landed, this is the martyrdom trigger (§3, Path B) — see §7.3 and §8.2 for exactly how this resolves and how it's revealed (privately, immediately; publicly, only at the finale).
- **A regular exposed Cultist, Cast Out:** simply removed — the cult loses a member, no win-condition trigger.
- **An innocent bystander, Cast Out:** removed, no extra penalty beyond the vote itself being "wasted" — that cost is already real given the vote cadence isn't unlimited.

### What happens to a Cast Out player
Rather than inventing a new "ghost" system, Cast Out players fold into the existing **Servant track**: their app reveals their full role/history for closure, they're locked out of future nominations/votes/abilities, but they can keep playing the even-round contests for fun and Servant-leaderboard credit. This directly solves the long-standing "what does an eliminated player do for the rest of a 2-hour party" gap using infrastructure that already exists.

---

## 7. How the Factions Interact

This is the heart of what makes the amendment work as a *game*, not just a win-condition table.

### 7.1 A symmetric hunt
For the first time in this design's history, the structure is genuinely symmetric: the Ton hides a King/Queen the Cult wants to corrupt, and hunts a Revolutionary Leader they need to expose. The Uprising hides its Leader from *everyone, including itself*, and quietly works to protect them. The Cult sits underneath both, feeding on whichever side is currently losing and angling for either a clean double-conversion or a dramatic backup plan. Every faction has something to protect and something to hunt — nobody just plays defense, and nobody just plays offense.

### 7.2 The information economy
- **The Ton's** main lead-generation tool is the Oracle (repurposed as the Leader-hunting tool) plus whatever surfaces organically in open debate.
- **The Uprising's** main lead-generation tools are the Spymaster (once per game) and the round-loss intel query (now asking "is this the Leader?" — useful precisely because their own Leader is a mystery to them too).
- **The Cult** has the sharpest tool of the three: a repeatable, choose-your-question scouting ability the Cult Leader can aim at either royal target, every recruitment cycle.

### 7.3 The martyrdom paradox — the game's best moment
Once the table suspects someone might be the Cult Leader, the normal genre instinct is "vote them out immediately." Here, that instinct can backfire: if a royal target has already been secretly converted, executing the Cult Leader is exactly what hands the Cult the game (the "heartbroken, galvanized" convert completes the mission in their master's memory). Nobody outside the Cult can ever be sure a conversion has already landed — which means the *correct* play (denounce the suspicious person) and the *safe* play (leave them be, just in case) are genuinely in tension, with no way to resolve it except taking the risk. This paradox should be preserved deliberately — no public warning system, no safety valve. The uncertainty **is** the mechanic.

### 7.4 Whistledown as connective tissue
Because so much of this game happens in private (conversions, successions, martyrdom triggers), **Lady Whistledown's Society Papers** — the recurring in-app gossip column — is what gives the whole table a shared, if unreliable, narrative of what's happening. It never states mechanics outright; it hints, misdirects, and lets the room argue about what it means. See §8.1 for example copy.

---

## 8. Theme & Narrative

### 8.1 Lady Whistledown, in practice

Whistledown should never confirm a mechanic directly — only "society's" partial, editorializing read on events. A few worked examples:

**A routine, low-stakes Denouncement:**
> *"Dearest reader, the Ton awoke to shocking news — Lord [Name] was cast from society's good graces at last night's gathering, denounced by the very peers who once toasted his health. Whether justice was served or a grave error made, this author cannot say. But oh, how the punch bowl trembled."*

**Someone secretly important is Cast Out** (used interchangeably for the Revolutionary Leader *or* the Cult Leader, so the phrasing itself is never a tell):
> *"Society lost one of its most curious figures last night — a person of quiet magnetism, whom few truly knew and fewer still will admit to mourning. This author has heard whispers that somewhere in the Ton, a heart breaks harder than propriety allows. Grief, dear reader, makes fools and zealots of us all."*

Keep 2–3 interchangeable templates for "someone significant fell" and rotate them without pattern, so players can't learn to read the headline style as a signal.

### 8.2 The martyrdom moment, privately

The instant the Cult Leader is Cast Out with a royal conversion already in place, only the convert sees this — immediately, privately, before Whistledown even prints:

> *"Your hands are still trembling from the vote. You did not know — could not have known — the weight [Cult Leader's name] carried, or the promise you made them in confidence. And yet their final words to you echo louder now than any denouncement: 'Should I fall, you will finish what we began.' The mask you wear suddenly feels less like a game, and more like the only honest thing left on your face."*

This is followed immediately by a plain, unambiguous statement of their (now locked-in) win condition — the flavor sets the mood, but the mechanics need to be completely clear so nothing gets missed in the moment. **This never becomes a public announcement** — it surfaces, if at all, only in the final reveal at the end of the game.

### 8.3 Framing the vote itself
- The mechanic is **"The Denouncement."** Losing it means being **"Cast Out"** — mask removed, escorted out of the ballroom, seated with the Servants.
- Nominating someone is **"laying a calling card against them"** — reuses a gesture already established for tasks elsewhere in the design, so there's no new UI concept to teach.
- Introductory flavor line for game start: *"Tonight, propriety itself is on trial. Should the Ton judge you a danger to the Crown, you shall be Cast Out — your mask removed for all to see, though not all truths revealed."*

---

## 9. Running the Game (host notes)

- **The debate phase should be entirely player-run.** Dalton starts a visible shared timer and lets the room self-organize — he doesn't moderate who speaks, which removes him as a bottleneck for exactly the phase that would otherwise demand the most of his attention.
- **Give the debate window its own hard timer**, separate from the round timer (recommend a fixed ~2.5 minutes) — this is what stops a live debate from organically eating the whole round, and means Dalton is never the one who has to cut someone off mid-argument.
- **Tie-break rule, decided in advance:** a tied vote re-runs only between the tied nominees; a second tie means no one is Cast Out. No host tiebreak, to avoid any appearance of favoritism.
- **Display the tally, not the ballots:** show only the final vote counts per nominee, never who voted for whom.
- **This resolves an old open question:** the Last Round's design is no longer a blank slot — it's one more Denouncement, at higher stakes, with a full public reveal attached. No new system to build or teach on the final night.

---

## 10. Balance Analysis — an honest read, including the bad news

The amendment is a real structural improvement: for the first time, both public factions have something to hide *and* something to hunt, instead of the Ton being able to win by pure inaction while the other two factions groped in the dark. Two of the biggest old imbalance findings are fixed directly by this redesign (Oracle now gives the Ton a real hunting tool; the Cult Leader's scouting ability can now target either royal). But the math surfaces one serious, unresolved risk:

**The Revolutionary Leader may be substantially over-favored to survive, and the fix isn't obvious.**

Modeling execution as if it were close to random (i.e., without assuming Aristocrat deduction is very strong), and assuming a working player count around 16:
- If the Denouncement only happens twice (rounds 3 and 5, as designed in §5), the Leader's baseline survival odds are roughly **87.5%** — far above the 30% target, essentially by default.
- Even running the Denouncement in *every* round, that baseline only drops to roughly **62.5%** — still too high on its own.

Closing that gap requires genuinely strong detection, not just more attempts — vote frequency alone can't get there. This is the single biggest open numeric risk in the whole design, bigger than any other adjustment made here, and it's presented as an open decision rather than something this report resolved unilaterally (see §11).

The martyrdom path (Cult win, Path B) is comparatively minor and self-limiting — it needs both a successful conversion *and* the specific bad luck (for the Cult) of the room correctly targeting the Leader rather than any other cultist, which gets statistically harder the more the cult has successfully grown. Expect it to contribute a real but small slice of the Cult's win rate, not to carry it.

---

## 11. Open Decisions for Dalton

These need an explicit call before this design is final — none of them were safe to decide unilaterally, either because the four reviews genuinely disagreed, or because the answer depends on real-world facts (headcount, venue) this report doesn't have.

1. **Denouncement frequency vs. balance.** §5 proposes voting only at rounds 3, 5, and the finale, for pacing and simplicity. §10's math says this alone makes Revolutionary too strong (~87.5% naive survival) — even voting *every* round only brings that to ~62.5%. Closing the rest of the gap requires the Ton's detection tools (Oracle, debate-driven deduction) to be genuinely sharp, not just "somewhat better than random." Options: (a) keep the sparse schedule and lean hard on making Oracle/debate powerful — simplest to run, riskiest for balance, best suited to a first playtest; (b) run the Denouncement every round from round 2 on — better balance headroom, meaningfully more live-ops load every single round.
2. **Does King/Queen's survival matter to the Ton's win, or only their conversion status?** As written (§3, §6), a King/Queen who's Cast Out (not converted) doesn't cost the Ton anything. That's a deliberate reading of the amendment's literal wording, not an obvious inevitability — a one-clause fix ("and the King/Queen is alive") is available if survival should matter too.
3. **The 16-player threshold for a double-execution round** (§6) is a placeholder — needs Dalton's actual expected headcount to set correctly.
4. **Whether to extend the Cult Leader's scouting ability to Revolutionary-Leader-queries was adopted directly in this report (§4.3)** as a strong cross-fork recommendation — flagging it here anyway since it's a real power increase for the Cult worth a deliberate sign-off, not just inheriting it by default.
5. **Whether the Revolutionary Leader's identity should be secret even from their own team (§4.4)** — this report adopted it as a load-bearing recommendation (two independent reviews converged on it for different reasons), but it's a genuine departure from how the Cult faction works (where members do know each other), and worth Dalton explicitly signing off on rather than discovering later.

---

## 12. What Changed From the Pre-Amendment Design (quick reference)

- Revolutionary's win condition is no longer "kill the King/Queen" — it's "the Leader survives."
- A new **Revolutionary Leader** character exists, secret even from their own faction.
- The standalone **poison mechanic is cut**; its round-outcome rewards are repurposed to protect/inform around the Leader instead.
- **Potion Maker** now grants execution-immunity instead of curing poison.
- **Doctor/Medic** is finalized as the Uprising's dedicated protector (was previously an undecided proposal).
- **Oracle** is reframed as the Ton's tool for hunting the Revolutionary Leader (was previously a generic, somewhat redundant lookup).
- **The Accusation mechanic is retired entirely** — the Denouncement fully absorbs its job.
- The **Last Round** is no longer an open question — it's one final, higher-stakes Denouncement with a public reveal.
- The **Cult Leader now has two win paths**, including the new "martyrdom" path tied directly to the Denouncement mechanic.
