# Murder Mystery 2026

App-assisted, live social-deduction party game (similar in spirit to "Feed the Kraken" / Jackbox-style hidden-role games). Players are split into secret factions with individual characters, abilities, and win conditions, and play out a series of timed rounds mixing social/talking tasks with physical/mental party-game contests. Dalton runs the game and is the only person allowed to see anyone's phone screen.

## 1. Overview

* **Format:** Live party game, app-driven (hidden roles, tasks, timers, chat)
* **Theme / Era:** TBD — masquerade, "Bridgerton season 4" aesthetic
* **Attire:** Fluffy dresses and tuxes, masquerade
* **Total play time:** 2 hours
* **Structure:** 6 rounds (15 min each) + 2 special rounds (first and last) + 1 intermission around the halfway point
* **Host:** Dalton (game master; only person allowed to view players' phone screens)

## 2. Factions

| Faction | Color | Secret? |
|---|---|---|
| Aristocrat | Blue | No |
| Revolutionary | Red | No |
| Cultist | Lime Green | Yes — kept hidden and revealed gradually over the game |
| Servant | *(unassigned)* | No |

## 3. Player & Character Assignment

* During character creation, every player rates their desired involvement level 1–10.
* Players rating 5 or under cannot receive a major character (unless there isn't enough interest in major roles to fill them).
* Above 5, involvement is weighted into a raffle-style ticket system so more-interested players are more likely to land a major role:
    * 6 → 1 ticket
    * 7 → 5 tickets
    * 8 → 20 tickets
    * 9 → 50 tickets
    * 10 → 100 tickets
* Players who arrive late become **Servants** instead of a main role, since they missed the window to be assigned one.

## 4. Character Creation

Fields collected per player:

* **Character Name** — 32 char max
* **Real Name** — 32 char max
* **Bio:**
    * **Occupation** — 32 char max
    * **Hobbies** — list of 5, 32 char max each
    * **Notable clothing features** (color, type, etc.) — list of 5, 32 char max each
    * **Skills** — list of 5, 32 char max each
* Where it makes sense, bio text is converted to Pascal Case With Spaces for display.

## 5. Factions in Detail

### 5.1 Aristocrat

**Win condition:** The King/Queen is alive at the end of the night and is not a cultist.

**Characters:**

* **King/Queen** — Count: 1
    * Goal: Survive the night
    * Starting knowledge: None
    * Ability: Once per game, before round 5, may transfer their royal title to someone else. Doing so unmasks both the King/Queen and the Prince/Princess. The title then passes to a random Aristocrat — if that person already had a role, it is overwritten with King/Queen.
* **Prince/Princess** — Count: 1
    * Goal: Protect the King/Queen
    * Starting knowledge: None
    * Ability: Learns of the King's/Queen's identity after round 2
* **Priest/Priestess** — Count: 1
    * Goal: Protect the King/Queen
    * Starting knowledge: None
    * Ability: Can protect one person per cult-recruitment round from being converted (they don't know this is what they're protecting against). The Cult Leader may not select the protected person during that round's recruitment and must choose someone else. They cannot protect the same person twice.
* **Oracle** — Count: 1
    * Starting knowledge: None
    * Ability: After every odd round, may view a single player's entire history up to that point. That view is locked at that moment — the Oracle does not gain updates on how that player acts afterward.
* **Potion Maker** — Count: 1
    * Starting knowledge: None
    * Ability: Once per game, gives one person an antidote. If that person was previously poisoned, they are cured. The Revolutionaries learn who the antidote was used on.
* **Defector** — Count: 1
    * Starting fraction: Aristocrat (with a different character). After the intermission, becomes Revolutionary and loses their Aristocrat role.
    * Restriction: Cannot be assigned to the King/Queen or Prince/Princess.
* **Normal Aristocrat** — Count: catch-all (anyone on the Aristocrat team without another named character)
    * Starting knowledge: None
    * Ability: Auto-succeeds one failed social task, once per game.

### 5.2 Revolutionary

**Win condition:** Kill the King/Queen. *(Decided: single win condition only — see §9.1 item 5.)*

**Characters:**

* **Bartender** — Count: 1
    * Starting knowledge: None
    * Flavor: Acts as a bartender by day; works to intoxicate one person per round for the Revolutionaries.
    * Ability: Once per round, selects a person who has a 50% chance of becoming drunk for that round. If successful, that person's ability doesn't work this round. The Bartender isn't told whether it worked; the target IS told if they're drunk.
* **Spymaster** — Count: 1 *(new — adopted directly, see §10.1/§10.3)*
    * Starting knowledge: None
    * Ability: Once per game, may view a single player's faction color only (not their full history, unlike the Oracle, §5.1). Gives Revolutionary its first real tool for narrowing down who the King/Queen might be — previously neither Revolutionary nor Cultist had any identification tool at all, which review found to be the single biggest lever on hitting the 30/30/30 win-rate target (§10.1). Also resolves the former "Normal Revolutionary catch-all" open question.

**Proposed / not yet finalized:**
* Poison mechanic — poisoned players all die at the end of the night.
* Doctor/Medic character — protects one person per round; cannot protect the same person two rounds in a row.

**Round-outcome mechanics:**
* Revolutionaries operate as a democracy — they vote on who to poison.
* If the Revolutionaries win a round, they get to poison someone.
* If the Revolutionaries lose a round, they get a consolation "intelligence" reveal: they choose a specific player and learn a direct yes/no answer to "is this person royalty (King/Queen or Prince/Princess)?" *(tightened from a generic role-reveal — see §10.1. Still doesn't resolve the mismatched trigger noted there — the poison action only fires on a round win, this only fires on a loss — see §8 item 9.)*

### 5.3 Cultist (secret faction)

**Characters:**

* **Cult Leader** — Count: 1
    * Goal: Convert the King/Queen to the cult AND keep the King/Queen alive at the end of the night
    * Starting knowledge: None
    * Ability: Before each recruitment window, may query one candidate with a direct yes/no "is this person Aristocrat-aligned?" check before deciding who to recruit. *(New — adopted directly. This was balance review's top-priority fix, bigger than any Aristocrat-side defensive change: the Cult Leader previously had zero way to find the King/Queen, making blind conversion attempts land well under a 30% success rate — see §10.1.)*
* **Cultist** — Count: starts at 0, grows via recruitment during the game
    * Goal: Help the Cult Leader
    * Starting knowledge: Knows who their fellow cultists are

**Gameplay:**
* At set times during the game, the Cult Leader recruits a new cultist to their cause.
* Each round, the cult gets a "divine revelation" on which side (Aristocrat or Revolutionary) to help, and secretly aids whichever side is currently losing (some metric is needed to track who's ahead between the Revolutionaries and Aristocrats).
* Recruitment runs on a fixed schedule, independent of round outcomes — e.g. the Cult Leader may recruit one new member every 2 rounds. *(Decided: rubber-band — decoupling recruitment from "did our helped side win" so the cult can keep growing even while propping up the underdog; see §9.1 item 2.)*
* If the King/Queen is converted before they've used their title-transfer ability (and it's still available), that ability is auto-used. In this case, both the King/Queen and Prince/Princess join the cult.
* The cult has a secret in-app group chat.
* A converted cultist keeps their original character and abilities, letting them keep fooling their original faction.

**Open item:** the faction win condition is currently only stated from the Cult Leader's personal goal — worth confirming whether regular Cultists share that exact win condition or have their own.

### 5.4 Servant

* Latecomers who missed main-role assignment.
* Get tasks each round like other factions, but their score does not count toward the Aristocrat or Revolutionary score.
* Have their own separate high-score leaderboard.
* Their bios are added into the pool of task options (e.g. "talk to someone with hobby X" tasks can reference Servants too).

## 6. Round Structure

Alternating odd/even rounds, bookended by two special rounds, with an intermission around the midpoint.

### 6.1 Odd Rounds — Social/Task Rounds

* Goal dynamic: Revolutionaries try to stall the Aristocrats from completing their tasks.
* Only X% of Aristocrats need to complete their task to succeed; this threshold starts very high (in round 1) and decreases across the night.
* Idea: one character could have a once-per-night ability to raise or lower the needed threshold.
* Talking-task mechanic: for a "talk to X" task, a player selects 3 people they talked to. If any of the 3 match, they get credit for completing the task — but they don't learn *which* of the 3 it was.
* **Task difficulty tiers:**
    * *Easy:* Talk to someone wearing X
    * *Medium:* Talk to someone who does X for work / has the hobby of X / has the skill of X
    * *Hard:* TBD
* **Round 1 (special, first round):** Soft intro round — everyone gets only 2 tasks (1 easy + 1 medium), to let people get familiar with the game. Will likely favor the Aristocrats.

### 6.2 Even Rounds — Contest Rounds

* Format: Take the N/2 highest scores between Revolutionaries and Aristocrats and average them (cultists secretly contribute to whichever side they were told to help that round). Contests are held in Strength, Intelligence, and Creativity — one point per contest won; most points at round's end wins the round.
* Some even rounds: everyone splits into 3 groups by category and plays multiple games within their chosen contest.
* Other even rounds: everyone competes together, moving through Creativity → Intelligence → Strength in sequence. This format is used for the **first even round**, so players get a feel for each category before rounds mix formats.

**Strength:**
* Most self-reported? *(unclear)*
* Ninja
* One-legged battle-off
* Most impressive feat of strength — ranked-choice voting
* Tug of war
* Bracket format: everyone who picks Strength splits into two teams in the backyard; winning side keeps playing and re-splits into two (halving each round) until it's 1v1. Contestants score points based on the round they reached.

**Creativity:**
* Best drawing — same style/voting as *Game Changer*
* Best haiku
* Best balloon animal
* Best 10-second melody on a virtual piano keyboard (7 notes only) — same voting/timing format as *Game Changer*; make sure audio works correctly

**Intelligence:**
* Trivia — multiple choice: History, Science, Math
* Multiple choice: Geometry, Algebra, simple arithmetic
* Tallest paper tower building contest

### 6.3 Last Round (special, final round)

* TBD — no design yet.

### 6.4 Intermission

* Around the halfway point (by round count), a ~15-minute break with a different kind of mini-game.
* Idea: "Who is Lorel's number one love?" — anyone wanting to enter states their case in 45 seconds; Lorel picks a winner at the end; the winner's faction gets a reward.

### 6.5 Lady Whistledown's Society Papers *(adopted — see §9.2)*

* After every odd round, an in-app "society paper" is published to everyone — a public, anonymous, in-fiction gossip column narrated as Lady Whistledown.
* Built entirely from data the app already tracks — no new information is created, only existing round outcomes are surfaced narratively: who won/lost the round, poison/antidote events (framed as scandal, not raw mechanics), leaked-intel reveals, and Accusation results (§6.6).
* Doubles as the delivery mechanism for the existing "leaked intel" reward (§7) and for a Cultist exposed via the Accusation (§6.6) — both become a Whistledown headline rather than a flat app notification.
* Shares the Oracle's existing "after every odd round" cadence (§5.1) — open styling detail whether the Oracle is flavored as Whistledown's source, or the two are kept unrelated in-fiction.

### 6.6 The Accusation *(adopted — see §9.1 item 1; first-pass design, refine after playtesting)*

* Trigger: end of each odd round from round 3 onward (round 1 stays a pure soft-intro round with no accusation).
* Any player — including Cultists, who may vote to deflect suspicion — casts one anonymous accusation via the app, naming a single other player they believe is a Cultist.
* Resolution:
    * If a plurality of accusations lands on the same player **and** that player is actually a Cultist, they are exposed: their Cultist status is revealed to the whole group as a Whistledown headline (§6.5). As a penalty, the Cult Leader skips their next scheduled recruitment window (§5.3).
    * If the plurality target is **not** a Cultist, nothing happens to them — but as a small cost for a wrong guess, the next odd round's Aristocrat task-completion threshold increases slightly, to discourage blind/spam accusations.
* Gives Aristocrats and Revolutionaries their first real actionable tool against the Cultist faction, directly addressing the balance review's top finding.

## 7. General Rules & App/UX Notes

* Phones must stay hidden from other players — press-and-hold a hidden area to reveal your own faction/character. Players may not show their phone to anyone except Dalton; any questions go to Dalton.
* The Cultist faction is kept secret at first: the game intro explains the Aristocrat and Revolutionary factions and their goals, and mentions there's a secret third faction without detail. Information about the Cultists is slowly revealed as the game progresses.
* Players can review their own full history at any time (who they talked to, tasks completed per round, etc.) — but only their own, not other players'.
* Challenge rewards may include the ability to identify someone's role/faction ("leaked intel").

## 8. Open Questions / TODO

These are the gaps and ambiguities left in the source notes. Two multi-angle reviews (§9, §10) proposed concrete options for most of them — listed here as a menu, not a decision — flagged for follow-up.

1. **Last round design:** completely undefined.
    * *Option A:* The Unmasking — ceremonial public faction reveal for all survivors, followed by a live vote/trial on the King/Queen's fate (Secret Hitler-style climax).
    * *Option B:* Final Ball — condensed high-stakes normal round: King/Queen publicly names 1-2 protectors, Revolutionaries get one last coordinated poison vote, Cult Leader gets one final conversion attempt, all resolved live.
    * *Option C:* Trial by the Ton — Among Us-style open accusation, discussion, and vote to banish one player, role revealed on elimination, right before the final win-condition check. (Note: overlaps with the now-adopted Accusation mechanic, §6.6 — could reuse/escalate it here instead of a separate system.)
2. **Hard-tier tasks** for odd rounds: undefined.
    * *Option A:* Talk to someone with a hidden-state trait not visible in their bio (currently drunk, currently poisoned, is a Servant, is in faction X) — forces real deduction instead of a bio lookup.
    * *Option B:* Extraction task — get a target to reveal one specific fact about themselves (real name, an ability, a history entry) and report it correctly; they may lie to you.
    * *Option C:* Popularity task — get N different people to independently name *you* as someone they talked to this round; requires proactive strategy, not passive matching.
3. **Doctor/Medic and the poison-death mechanic:** still just brainstormed options, not yet decided — keep weighing.
    * Both the balance and content reviews independently flagged the same recommendation if/when this gets locked in: resolve poison as an **end-of-night accumulation, not an instant mid-game elimination** — instant elimination creates dead time in a 2-hour party game with no spectator/ghost design, while end-of-night resolution keeps everyone playing through the finale. "Can't protect the same person twice in a row" (Doctor/Medic) already assumes this slower cadence. Not adopted — just recorded for whenever this gets decided.
4. **Servant faction color:** not assigned (Aristocrat/Revolutionary/Cultist all have one).
    * *Option A:* Silver/Grey — reads as "unranked/neutral," elegant against a masquerade backdrop.
    * *Option B:* Champagne/Gold — ballroom-coded, distinct hue family from Blue/Red/Lime.
    * *Option C:* Plum/Deep Purple — regal-adjacent but clearly separate from the other three; fits "mysterious latecomer."
5. **Cultist win condition:** only stated via the Cult Leader's personal goal — do regular Cultists share it exactly?
    * *Option A:* Shared/simple — the whole faction wins iff the Leader's condition is met (King/Queen converted AND alive at the end); team lives or dies as one block (Secret Hitler-style).
    * *Option B:* Scoped/alternate — cult wins if the King/Queen is converted **or** the cult reaches N members by night's end; rewards recruitment momentum even if the final conversion fails.
    * *Option C:* Shared win + individual bonus — base win is the Leader's condition for everyone, but each recruited cultist also carries a personal secondary objective (stay unexposed / your recruiter survives) — Feed the Kraken-style team win with an individual layer.
6. **⚠️ Top priority — the "Blood on the Clocktower" amendment** (see bottom of doc, "# Amendments"). Dalton is considering replacing the current design with a daily public-elimination-vote format: the whole group votes each round to eliminate one player (or two, with enough players); Aristocrats hunt a new "Revolutionary Leader" role instead of protecting the King/Queen; Revolutionaries aim to survive the night; the Leader's title passes to another Revolutionary if eliminated early (mirrors the King/Queen's existing title-transfer, §5.1); Cultists keep helping the losing side (already compatible with §5.3). A four-angle review (§10) evaluated this in detail — summary before the options:
    * **Fun:** genuinely the strongest lever available for making the game more fun — public elimination-vote formats reliably generate more real tension and table-talk than anything currently in the doc. But conditional on two things the amendment text doesn't specify: an actual scheduled discussion/debate phase before the vote (not a silent tally, which is all Accusation currently has — see item 10 below), and a defined role for eliminated players (being cut from a live 2-hour party is a much bigger deal than an abstract "poisoned" status).
    * **Balance:** not adoptable as-is. It relocates the game's core balance flaw (no faction can *identify* its target) rather than fixing it — Aristocrats still have no way to find the Revolutionary Leader. Worse, nothing in the stated goals puts any elimination pressure on Cultists at all, which could push their win rate well above 30% instead of toward it. "Survive the night" for Revolutionary is also dangerously unscoped — does the whole faction need to avoid elimination, or just the Leader?
    * **Simplicity:** a genuine net simplification of the core deduction loop if it *fully replaces* the odd-round tasks and Accusation (§6.1, §6.6) — those would become fully redundant and should be deleted, not kept alongside it. But the transition cost is real: it silently orphans 5 of Aristocrat's 6 characters and the whole King/Queen-conversion cascade (§5.1, §5.3), since Aristocrat's entire kit is currently built around protecting a King/Queen this amendment has no further use for. This is a full redesign of §5, not a bolt-on.
    * **Content:** the biggest blocking question is whether the elimination vote *replaces* the round structure or runs *alongside* it — the amendment reads as a replacement ("instead of my current style") but doesn't say so outright. Two adoption paths:
        * *Path A — clean mirror-swap:* Retire King/Queen. Revolutionary Leader becomes the universal target all three factions revolve around (Cultist's win condition becomes "convert and preserve the Revolutionary Leader," a direct swap of §5.3). The old Aristocrat protector kit (Prince/Princess, Priest/Priestess) reassigns to Revolutionary to protect their own Leader; Oracle moves to Aristocrat as a hunting tool. Potion Maker becomes an execution-immunity role (saves whoever the vote lands on) rather than an antidote-giver. A full functional kit-swap between factions, not a single new character.
        * *Path B — additive:* Keep King/Queen as a lighter/ceremonial role (e.g. runs the nomination phase), treat "hunt the Leader" as an *added* Aristocrat objective alongside the existing one. Preserves existing character investment, but risks a dual-win-condition complexity problem and doesn't deliver the simplification the format is otherwise capable of. Weaker option.
    * **Reusable regardless of path:** the vote mechanism itself substantially overlaps with the already-built Accusation mechanic (§6.6) — generalizing its target (anyone, not just suspected Cultists) and consequence (elimination, not exposure) gets most of the way there rather than inventing a new system. Eliminated players can fold into the existing Servant faction's separate scoring track (§5.4) rather than needing new "ghost" rules built from scratch.
    * *Option A:* Adopt via Path A (full kit-swap) — biggest simplification payoff, biggest redesign cost.
    * *Option B:* Adopt via Path B (additive) — lower redesign cost, weaker simplification/balance payoff, real risk of dual-win-condition bloat.
    * *Option C:* Don't adopt for this event — keep the current task/contest design, but port the *discussion-phase* and *defined-elimination-status* lessons into the existing Accusation mechanic (§6.6) as a lighter-weight fun upgrade instead.
    * Whichever direction: resolve first (1) does it fully replace §6.1/§6.2 or coexist, (2) what puts elimination pressure on Cultists, (3) what "survive the night" means precisely for Revolutionary.
7. **Oracle (§5.1) and Accusation (§6.6) do overlapping jobs** — both are "find the Cultist" tools with the same odd-round cadence. Simplicity review recommends cutting or shrinking Oracle in favor of Accusation (more socially engaging — whole-table participation vs. one player's private lookup).
    * *Option A:* Keep both as-is.
    * *Option B:* Cut Oracle, lean on Accusation alone.
    * *Option C:* Merge — fold Oracle's private-lookup flavor into Accusation's resolution (e.g. the exposer gets a bonus history glimpse).
8. **The odd-round "Revolutionaries stall the Aristocrats" framing (§6.1) has no mechanic behind it** (also flagged §9.1 item 6). Simplicity review recommends cutting the framing line rather than building a new mechanic to justify it, unless Dalton wants the real sabotage tool.
    * *Option A:* Cut the line — task rounds are just task rounds, no adversarial framing.
    * *Option B:* Build a real sabotage action for Revolutionaries on odd rounds.
    * *Option C:* Leave as unbacked flavor text — low cost, but inconsistent if a player asks "how do I actually stall them?"
9. **Revolutionary's poison-action and intel-reveal fire on mutually exclusive round outcomes** (poison only on a round win, §5.2; intel only on a round loss) — balance review flags this as working against Revolutionary ever having both a target and a kill opportunity in the same window.
    * *Option A:* Grant both on any round outcome (win or lose).
    * *Option B:* Keep the asymmetry, but let a target identified on a loss stay "marked" for the next win's poison.
10. **Accusation (§6.6) currently resolves as a silent app tally with no scheduled discussion phase** — fun review flags this as leaving the actual dramatic payoff of an accusation mechanic (the argument, not the tally) off the clock.
    * *Option A:* Carve out explicit discussion time before the vote closes each eligible round.
    * *Option B:* Leave it silent/app-only — lower time cost, less table drama.
11. **"Servant Patronage" — proposed new mechanic, not yet decided:** once per game a Servant secretly pledges to one main faction; if that faction wins, the Servant gets a leaderboard bonus (announced anonymously via Whistledown, §6.5). Reuses existing Servant scoring and Whistledown publication — no new subsystem — and gives Servants a real stake in the main conflict instead of a purely parallel scoreboard.
    * *Option A:* Adopt.
    * *Option B:* Skip — Servant track stays fully separate as currently designed.

## 9. Multi-Angle Review Findings (2026-08-31)

Four parallel reviews of the plan above — game-balance (benchmarked against Feed the Kraken / Secret Hitler / Among Us), Bridgerton theme depth, live-event logistics, and the content gaps in §8. Findings below; nothing in this section has been adopted into the design yet except where noted.

### 9.1 Structural / Balance Risks

1. **No player-driven accuse/vote mechanic exists anywhere in the doc.** All three named inspirations give the majority faction a collective tool against suspected minority players (Kraken's votes, Hitler's election pressure, Among Us's emergency meetings). Here, Aristocrats have zero tool to act on suspicion — only the Oracle's passive look and the Priest/Priestess's blind block exist. **Decision: adopted** — see §6.6 "The Accusation" for the mechanic as designed.
2. **Cult recruitment trigger contradicts the cult's stated behavior.** The doc says the cult helps whichever side is *losing* (rubber-band), but only rewards recruitment when the side they helped *wins*. If they're propping up the underdog, that side is still likely to lose — so the cult may rarely grow, stalling its own win condition. **Decision: adopted, rubber-band** — recruitment decoupled onto a fixed schedule; see updated §5.3 Gameplay.
3. **King/Queen is a single point of failure/success for all three factions' win conditions but has almost no agency.** Their only tool is a once-per-game, self-triggered, no-starting-info title transfer, and the §3 ticket system doesn't exclude them from low-involvement assignment the way it excludes the Defector. Recommend either excluding King/Queen from low-involvement assignment, or giving them a small passive ability/starting clue so they aren't a pure damage sponge for 90+ minutes. *(Still open — sharpened by §10.2 item 1: the ticket system tends to seat your most engaged player in this least-active role.)*
4. **Poison should resolve at end-of-night, not as mid-game elimination** — see §8 item 3; both this review and the content-gap review converged on this independently. *(Still open — folded into the still-brainstormed Doctor/Medic/poison item.)*
5. **Revolutionary's only defined win condition is single-target assassination.** If the King/Queen is well-protected (Priest/Priestess block + Potion Maker antidote + Prince/Princess), Revolutionaries may have no path to victory for the whole back half of the game. **Decision: no second win condition will be added** — Revolutionary keeps this single win condition (§5.2). This risk is accepted as-is; the new Accusation mechanic (§6.6) at least gives Revolutionaries something actionable even if the assassination plot stalls, but doesn't fully resolve the concern.
6. **The odd-round "Revolutionaries stall the Aristocrats" framing has no mechanical lever.** The stated tension isn't backed by any mechanic that actually interferes with task completion — the Bartender's ability-suppression doesn't obviously touch the talk-to-X task system. Either give Revolutionaries a direct sabotage action on odd rounds, or reframe the fiction — right now the mechanic and the narrative don't connect. *(Now tracked with concrete options as §8 item 8.)*
7. **Zero-feedback abilities risk disengagement over a 2-hour game.** The Priest/Priestess never learns whether their block mattered; the Bartender never learns if their 50% roll landed. Recommend a delayed/end-game reveal ("here's who you protected/drugged and whether it worked") so these roles feel meaningful in retrospect.
8. **Oracle's full-history reveal (available after every odd round — up to 3x/game) may be too strong** relative to the cult's information advantage, and risks flattening the "secret third faction" mystery the design otherwise depends on. Consider reducing frequency (once per game) or narrowing scope (task-talk history only, not everything). *(Sharpened by §10.4 item 1: Oracle and Accusation, §6.6, are now flagged as doing the same job outright — see §8 item 7.)*

### 9.2 Bridgerton Theme Integration

**Top recommendation: Lady Whistledown's Society Papers.** Skin the existing "leaked intel" reward (§7) and the Oracle's history-snapshot ability (§5.1) as an anonymous gossip broadsheet that "prints" periodically — e.g. after every odd round, piggybacking on the Oracle's existing cadence — using only data the app already tracks (contest results, round winners, poison/antidote events, leaked-intel reveals). This is near-pure re-skin: no new mechanic, no new data collection, and it gives the game a recurring narrative pulse instead of theme living only in attire + the intermission. **Decision: adopted** — see §6.5 for the mechanic as designed.

**Free re-skins (naming/flavor only, no mechanical change):**
* Aristocrat → *The Ton*; Revolutionary → *The Reformists*; Cultist stays hidden, flavored as a fringe secret society.
* Prince/Princess → *Heir*; Priest/Priestess → *Chaperone/Confessor* (chaperones literally exist to prevent a debutante from being led astray — stronger fit than "priest" for blocking cult conversion).
* Potion Maker → *the Modiste* (dressmaker/apothecary) — pairs with the "notable clothing features" bio field (§4).
* Bartender → a footman/valet plying guests with ratafia at the punch bowl — same mechanic, new dressing.
* Normal Aristocrat's auto-succeed ability → *a favor called in via a calling card*.
* Easy task-tier ("wearing X") → fan/glove/mask color clues — genuinely period-accurate (the language of fans and gloves was a real Regency flirtation code) and ties directly into the existing masquerade attire.
* Medium tasks → routed through an in-fiction calling-card exchange.
* Creativity contests → add bouquet arranging (language of flowers) or a waltz demonstration as an event option.
* Intelligence trivia → add a "Society & Scandal" category (in-fiction lore Dalton seeds) alongside History/Science/Math.

**Intermission sharpening:** turn "Lorel's number one love" into a proper rose ceremony — entrants present calling cards instead of just speaking; a Servant-flavored "chaperone" role could veto one entrant per propriety rules, adding a social-deduction beat instead of a pure popularity contest. Tie the winner's reward directly into the Whistledown mechanic (a favorable press mention) rather than a generic "faction reward."

**Name the central theme parallel explicitly:** the doc's own core conceit — a masquerade mask hides your face the way the app hides your faction — isn't currently stated anywhere. Worth one line in §1 or §7 making this explicit; it's free and sharpens how the game gets pitched at the intro.

**Stretch idea, new mechanic (not free — flag before committing given the existing §8 backlog): a "Scandal" track.** A visible meter that rises when Whistledown items name a player, paired with an in-fiction "duel" (challenge) mechanic that could replace the generic poison-vote resolution for Revolutionary's public confrontation beat. Optional; evaluate against the TODO backlog before taking on.

### 9.3 Live-Event Logistics

1. **Host bottleneck.** Dalton is the sole phone-viewer *and* the sole question-answerer, likely while also running physical contests. At any double-digit headcount, disputes will queue on one person during hard 15-minute timers. Consider designating 1-2 floor helpers who can handle logistics questions (not role-reveal), or a pre-scripted FAQ card for self-service.
2. **Round 1 pacing is optimistic.** 15 minutes has to cover reading a task, physically finding and talking to someone, self-reporting in-app, and any ability resolution — for a group that's never used the app before. Consider budgeting round 1 at 18-20 minutes, or timing the app-reporting flow in an actual playtest first.
3. **Even-round contest logistics need sizing info.** The Strength bracket (halving to 1v1) needs a roughly power-of-2 subgroup or byes get lopsided; Strength (loud, physical, backyard), Creativity (quiet, needs a device for the piano), and Intelligence (quiet, needs focus) running concurrently risks noise/space bleed between zones. *Missing info: expected guest count and venue layout — needed to size brackets and judge feasibility.*
4. **No spectator/idle-player experience is defined** for someone poisoned/eliminated, or waiting their turn in a bracket. Over a 2-hour event this is a real disengagement risk — consider an explicit low-stakes spectator role (judge/voter for Creativity, cheer squad for Strength).
5. **Judged-voting contests need infrastructure not yet specified.** "Same style/voting as Game Changer" implies real-time submission collection, display, and audience voting, on top of working audio for the piano game — this is the most technically demanding part of the app and currently has the least design detail. *Missing info: how submissions are collected/displayed, and who tallies votes.*
6. **Latecomer/Servant onboarding is undefined mid-event.** Arriving mid-round presumably means handing them a device, running the full character-creation bio form (§4), and assigning a Servant sheet — while a live round timer runs for everyone else. Consider a fast-path "Servant quick-create" with defaults so Dalton doesn't have to pause the round.
7. **Device/tech assumptions are unstated.** The design assumes every player has a personal smartphone, working audio, and reliable connectivity, with no stated fallback for a dead battery or forgotten phone. *Missing info: native vs. web app, one device per player guaranteed, offline/no-signal fallback at the venue — blocks the tech-stack decision.*
8. **Faction-color reveal could leak by accident.** Aristocrat=blue / Revolutionary=red / Cultist=lime green, combined with close-quarters mingling — if a phone is glanced mid press-and-hold and color paints before anything else, a neighbor could read someone's faction at a glance. Consider making color the last thing revealed on long-press, behind a tap-through, not the first paint.

## 10. Balance / Fun / Content / Simplicity Review (2026-09-03)

Four parallel reviews covering the angles Dalton asked for directly: 30/30/30 balance math, fun/engagement, additional content, and a simplicity audit — each also required to evaluate the "Blood on the Clocktower" amendment below specifically. The amendment's full cross-fork analysis lives in **§8 item 6** rather than being duplicated here, since it's a decision for Dalton to make, not a finding to file away. What follows is everything else.

### 10.1 Balance — target 30/30/30 (Aristocrat / Revolutionary / Cultist; Servant excluded)

**Core finding: Aristocrat wins by default; Revolutionary and Cultist both lacked any way to identify the King/Queen.** Aristocrat's win condition (§5.1) is a passive default — nothing needs to happen for them to win. Revolutionary and Cultist both need to land a specific action on one hidden individual, and neither previously had a real way to find out who that is:
* The Cult Leader had zero scouting ability — recruitment was a blind guess among all players, at ~1 recruit per 2 rounds over 6 rounds, giving real odds well under 30%. **Fixed directly** — the Cult Leader now gets a yes/no "is this candidate Aristocrat-aligned" query before each recruitment window (§5.3).
* Revolutionary's only lead-generation tool (the round-loss intel reveal, §5.2) only fired on a round *loss*, while the poison action it would feed into only fired on a round *win* — mutually exclusive triggers. **Partially fixed directly** — the intel reveal is now a chooseable, direct "is this person royalty?" check rather than a vague role-reveal (§5.2). The win/lose trigger mismatch itself is still open — see §8 item 9.
* Revolutionary was also character-thin (1 named role vs. Aristocrat's 6) with no identification tool of its own. **Fixed directly** — added the Spymaster character (§5.2), which also resolves the former "Normal Revolutionary catch-all" open question.

Net effect: both hostile factions now have a real (if limited) way to find their target, which review found to be the single biggest lever on hitting 30/30/30 — bigger than any further defensive buff to Aristocrat would have been.

### 10.2 Fun

1. **King/Queen has nothing to do, and the ticket system (§3) tends to seat your most engaged player there anyway** — a purely passive damage-sponge role landing on the person most likely to want to be central to the game. Sharpens §9.1 item 3; still open.
2. **Even-round contests (§6.2) fragment the room into three concurrent tracks.** Communal cheering/reacting — a big party-game fun multiplier — gets lost when the group splits into three side-activities instead of watching one shared spectacle.
3. **Accusation (§6.6) has stakes but no scheduled drama** — it resolves as a silent app tally. The fun part of accusation mechanics is the argument, not the tally. See §8 item 10.
4. **Worth keeping as-is:** the "talk to 3, credit if 1 matches, don't learn which" task mechanic (§6.1) — genuinely good uncertainty design that sustains engagement.
5. Aristocrat's kit (Priest/Priestess, Oracle, Potion Maker) is almost entirely reactive/defensive; Revolutionary and Cultist get more proactive choices. Purely reactive roles tend to feel flatter over a 2-hour live game — worth keeping in mind for future additions.

### 10.3 Content

1. **Revolutionary "Spymaster" — adopted directly** (§5.2): once per game, view a single player's faction color only. Closes both the roster-asymmetry and no-ID-tool gaps at once, and resolves the former "Normal Revolutionary catch-all" question.
2. **"Servant Patronage" — proposed, not yet adopted** (see §8 item 11): lets Servants secretly pledge to a main faction for a leaderboard bonus, announced via Whistledown. No new subsystem — reuses existing Servant scoring and Whistledown publication.
3. Everything else already had concrete options on record in §8 (Last round, Hard tasks, Doctor/Medic + poison, Servant color, Cultist win condition) — not re-litigated here.

### 10.4 Simplicity

1. **Oracle (§5.1) and Accusation (§6.6) are duplicate "find the Cultist" tools** — same cadence, same problem, different UI. See §8 item 7.
2. **The odd-round "stall the Aristocrats" framing (§6.1) has no mechanic behind it** — pure narrative debt Dalton would have to explain with nothing backing it. See §8 item 8.
3. **Even rounds (§6.2) run three concurrent live-hosted subsystems solo** (bracket sizing, judged voting + piano audio, trivia infra) — the single largest live-ops complexity item in the doc, independent of anything else in this review. Reinforces §9.3 items 1 and 3.
4. **Three Aristocrat characters (King/Queen, Prince/Princess, Priest/Priestess) share the literal goal "protect the King/Queen"** with only mildly differentiated abilities — not mechanically redundant, but blurs together for a first-time player memorizing who does what.
5. **The King/Queen-conversion cascade (§5.3)** — "if converted before title-transfer used, auto-use it, and Prince/Princess also joins the cult" — is a rare edge case with a disproportionately complex explanation. Candidate for a flat cut (converting the King/Queen simply doesn't auto-convert the Prince/Princess) unless this specific twist is a deliberately loved moment — not cut here since it's original design intent; Dalton's call.

# Amendments

* Instead of my current style, I am thinking of converting the style to more of a blood on the clocktower style where the group decides on a single person to eliminate every round (or 2 if we have enough players). The goal of the Aristocrats would then be to remove the revolutionary leader while the goal of the revolutionists would be to survive the night. If the revolutionaist leader is eliminated too early, then their title would be passed onto another revolutionist. The cultist party would act as a balancing party.
    * *Status: not yet decided — see §8 item 6 for the full four-angle evaluation (balance, fun, content, simplicity) and the concrete adoption options (A: full kit-swap, B: additive, C: don't adopt).*
