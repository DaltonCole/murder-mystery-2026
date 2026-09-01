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
    * Starting knowledge: *(TBD)*
    * Ability: None
* **Priest/Priestess** — Count: 1
    * Goal: Protect the King/Queen
    * Starting knowledge: None
    * Ability: Can protect one person per cult-recruitment round from being converted (they don't know this is what they're protecting against). The Cult Leader may not select the protected person during that round's recruitment and must choose someone else.
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

**Win condition(s):**
1. Kill the King/Queen
2. *(TBD — second win condition not yet defined)*

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
* Each round, the cult gets a "divine revelation" on which side (Aristocrat or Revolutionary) to help, and secretly aids that side. There should be some metric to determine who's currently winning between the Revolutionaries and Aristocrats, and the cult helps whichever side is losing. If the side they helped wins the round, the Cult Leader gets to recruit another member.
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

## 7. General Rules & App/UX Notes

* Phones must stay hidden from other players — press-and-hold a hidden area to reveal your own faction/character. Players may not show their phone to anyone except Dalton; any questions go to Dalton.
* The Cultist faction is kept secret at first: the game intro explains the Aristocrat and Revolutionary factions and their goals, and mentions there's a secret third faction without detail. Information about the Cultists is slowly revealed as the game progresses.
* Players can review their own full history at any time (who they talked to, tasks completed per round, etc.) — but only their own, not other players'.
* Challenge rewards may include the ability to identify someone's role/faction ("leaked intel").

## 8. Open Questions / TODO

These are the gaps and ambiguities left in the source notes — flagged for follow-up rather than guessed at:

1. **Revolutionary win condition #2:** only "kill the King/Queen" is defined; a second win condition is referenced but not written.
2. **Last round design:** completely undefined.
3. **Hard-tier tasks** for odd rounds: undefined.
4. **Doctor/Medic and the poison-death mechanic:** still just brainstormed options, not yet decided — keep weighing.
5. **Servant faction color:** not assigned (Aristocrat/Revolutionary/Cultist all have one).
6. **Cultist win condition:** only stated via the Cult Leader's personal goal — do regular Cultists share it exactly?
7. **Prince/Princess starting knowledge:** left blank.
10. **Is there a "Normal Revolutionary" catch-all**, analogous to "Normal Aristocrat," for Revolutionaries without a named character?
