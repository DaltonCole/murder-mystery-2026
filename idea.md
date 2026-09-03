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

**Proposed / not yet finalized:**
* Poison mechanic — poisoned players all die at the end of the night.
* Doctor/Medic character — protects one person per round; cannot protect the same person two rounds in a row.

**Round-outcome mechanics:**
* Revolutionaries operate as a democracy — they vote on who to poison.
* If the Revolutionaries win a round, they get to poison someone.
* If the Revolutionaries lose a round, they get a consolation "intelligence" reveal — e.g. learning the role/character of a single person ("You may have lost this one, but your intelligence network still came in! Who did you gather intelligence on?").

### 5.3 Cultist (secret faction)

**Characters:**

* **Cult Leader** — Count: 1
    * Goal: Convert the King/Queen to the cult AND keep the King/Queen alive at the end of the night
    * Starting knowledge: None
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

These are the gaps and ambiguities left in the source notes. A multi-angle review (§9) proposed concrete options for most of them — listed here as a menu, not a decision — flagged for follow-up.

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
6. **Is there a "Normal Revolutionary" catch-all**, analogous to "Normal Aristocrat," for Revolutionaries without a named character?

## 9. Multi-Angle Review Findings (2026-08-31)

Four parallel reviews of the plan above — game-balance (benchmarked against Feed the Kraken / Secret Hitler / Among Us), Bridgerton theme depth, live-event logistics, and the content gaps in §8. Findings below; nothing in this section has been adopted into the design yet except where noted.

### 9.1 Structural / Balance Risks

1. **No player-driven accuse/vote mechanic exists anywhere in the doc.** All three named inspirations give the majority faction a collective tool against suspected minority players (Kraken's votes, Hitler's election pressure, Among Us's emergency meetings). Here, Aristocrats have zero tool to act on suspicion — only the Oracle's passive look and the Priest/Priestess's blind block exist. **Decision: adopted** — see §6.6 "The Accusation" for the mechanic as designed.
2. **Cult recruitment trigger contradicts the cult's stated behavior.** The doc says the cult helps whichever side is *losing* (rubber-band), but only rewards recruitment when the side they helped *wins*. If they're propping up the underdog, that side is still likely to lose — so the cult may rarely grow, stalling its own win condition. **Decision: adopted, rubber-band** — recruitment decoupled onto a fixed schedule; see updated §5.3 Gameplay.
3. **King/Queen is a single point of failure/success for all three factions' win conditions but has almost no agency.** Their only tool is a once-per-game, self-triggered, no-starting-info title transfer, and the §3 ticket system doesn't exclude them from low-involvement assignment the way it excludes the Defector. Recommend either excluding King/Queen from low-involvement assignment, or giving them a small passive ability/starting clue so they aren't a pure damage sponge for 90+ minutes. *(Still open — not covered by the latest decisions.)*
4. **Poison should resolve at end-of-night, not as mid-game elimination** — see §8 item 3; both this review and the content-gap review converged on this independently. *(Still open — folded into the still-brainstormed Doctor/Medic/poison item.)*
5. **Revolutionary's only defined win condition is single-target assassination.** If the King/Queen is well-protected (Priest/Priestess block + Potion Maker antidote + Prince/Princess), Revolutionaries may have no path to victory for the whole back half of the game. **Decision: no second win condition will be added** — Revolutionary keeps this single win condition (§5.2). This risk is accepted as-is; the new Accusation mechanic (§6.6) at least gives Revolutionaries something actionable even if the assassination plot stalls, but doesn't fully resolve the concern.
6. **The odd-round "Revolutionaries stall the Aristocrats" framing has no mechanical lever.** The stated tension isn't backed by any mechanic that actually interferes with task completion — the Bartender's ability-suppression doesn't obviously touch the talk-to-X task system. Either give Revolutionaries a direct sabotage action on odd rounds, or reframe the fiction — right now the mechanic and the narrative don't connect.
7. **Zero-feedback abilities risk disengagement over a 2-hour game.** The Priest/Priestess never learns whether their block mattered; the Bartender never learns if their 50% roll landed. Recommend a delayed/end-game reveal ("here's who you protected/drugged and whether it worked") so these roles feel meaningful in retrospect.
8. **Oracle's full-history reveal (available after every odd round — up to 3x/game) may be too strong** relative to the cult's information advantage, and risks flattening the "secret third faction" mystery the design otherwise depends on. Consider reducing frequency (once per game) or narrowing scope (task-talk history only, not everything).

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

# Amendments

* Instead of my current style, I am thinking of converting the style to more of a blood on the clocktower style where the group decides on a single person to eliminate every round (or 2 if we have enough players). The goal of the Aristocrats would then be to remove the revolutionary leader while the goal of the revolutionists would be to survive the night. If the revolutionaist leader is eliminated too early, then their title would be passed onto another revolutionist. The cultist party would act as a balancing party.
