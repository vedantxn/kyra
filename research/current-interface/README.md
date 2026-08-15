# Kyra: Product and Interface Understanding

This document is the canonical product brief for our independent, software-first reconstruction of Kyra. It combines Kyra's public landing-page thesis with the two public product walkthroughs and the 17 screenshots supplied by Vedant.

It describes what is publicly visible, what the demonstrations say, what we can reasonably infer, and what remains unknown. It does not claim knowledge of Kyra's private implementation.

## Evidence and confidence

The conclusions below use four labels:

- **Official:** stated on [kyrainterface.com](https://kyrainterface.com/).
- **Observed:** directly visible in the supplied screenshots.
- **Spoken:** explained by Sahil in the visible video captions.
- **Inferred:** a product or technical interpretation supported by the public evidence but not directly confirmed.

The screenshots must be read in the reverse of the order in which they were attached: **Image 17 through Image 1**.

- **X walkthrough:** Images 17 through 7, corresponding to screenshots from 10:15:43 AM through 10:17:23 AM.
- **LinkedIn walkthrough:** Images 6 through 1, corresponding to screenshots from 10:18:29 AM through 10:19:06 AM.

The two videos show different demonstrations of the product. They share the same core interface, but each emphasizes different capabilities. We should not assume that either video represents a complete specification or infer their internal release order merely from the recordings.

## The product in one sentence

**Kyra is a proactive personal operations interface that finds commitments hidden across communication and calendar activity, keeps their source context attached, closes what it safely can, and brings the user only the few open loops that genuinely require human attention.**

Kyra is therefore not primarily a calendar, todo list, inbox, or chatbot. Those are surfaces it coordinates. Its underlying job is to maintain an accurate model of what the user owes, what other people owe the user, what is scheduled, what is unresolved, and what matters now.

## The problem Kyra is solving

The official site defines an **open loop** as anything a person still has to close. Examples include replying to a client, making an introduction, sending an update, following up on a promised deliverable, or completing a real-world action that began in a conversation.

These obligations are difficult to manage because they are distributed across WhatsApp, Gmail, calendars, conversations, and memory. Conventional task tools require the user to notice each commitment, manually record it, preserve enough context to understand it later, and repeatedly check the list. The work of maintaining the system becomes another open loop.

Kyra's thesis is that unfinished commitments occupy attention and are eventually dropped when the cognitive load becomes too high. The official manifesto connects this to the Zeigarnik effect and also identifies a second problem: interacting with a phone while moving or driving is slow, distracting, and sometimes unsafe.

The software product attacks the first problem immediately. Instead of waiting for the user to maintain a task manager, Kyra watches the places where work and commitments naturally appear, reconstructs the open loops, and proactively surfaces the important ones.

## Kyra's core product loop

The public evidence supports the following product loop:

1. **Observe context.** Read permitted communication and calendar activity.
2. **Detect commitments.** Identify promises, requests, delegated work, unanswered questions, follow-ups, and scheduled obligations.
3. **Preserve evidence.** Keep the message or conversational reasoning that caused an item to become an open loop.
4. **Assign ownership and state.** Distinguish work the user owes from work another person owes the user, and distinguish unresolved work from scheduled or completed work.
5. **Prioritize.** Decide which loops need attention now and which can safely wait.
6. **Surface.** Present a small, synthesized briefing rather than another undifferentiated inbox.
7. **Act.** Let the user create, schedule, dismiss, confirm, resolve, or eventually delegate work.
8. **Reconcile.** Observe subsequent activity so the loop can be updated or closed without requiring duplicate manual bookkeeping.

The final reconciliation step is an inference from the stated promise to find and close open loops. It is essential to the product thesis, but the supplied videos do not show its full behavior.

## Interface anatomy

### 1. Ambient macOS overlay

**Observed and spoken:** Kyra is summoned with `Command + K`. It appears as a large, translucent, full-screen layer over the user's existing desktop instead of behaving like a conventional application window. Clicking outside the active interface dismisses it and returns the user to the desktop.

This interaction makes Kyra feel like a system-level capability similar to Spotlight: it is available from wherever the user is working, offers a moment of orientation or action, and then gets out of the way.

The soft blur and translucent teal treatment preserve a sense of the user's existing workspace underneath. The visual design is calm and low-contrast, consistent with an interface intended to reduce cognitive load rather than compete for attention.

### 2. The time surface

The left side shows the user's schedule.

In the default view, it is a vertical 24-hour timeline for the selected date. It includes:

- a date selector at the top;
- hour markers spanning the day;
- a live current-time marker;
- blocks for sleep, meetings, exercise, reminders, and execution time;
- different colors for different kinds of calendar blocks.

The expanded **Next three days** panel turns the timeline into a three-column planner. Across the demonstrations, the interface shows that blocks can be moved, resized from their edges, created in an empty section of the grid, and assigned a color. The visible legend identifies blue as **meeting** and green as **execution**.

This is more than calendar visibility. Kyra connects commitments to actual time, allowing the user to inspect whether the plan reflects what must get done and to reshape the plan directly.

### 3. The Night briefing

The center of the overlay is headed **Night** and displays a short natural-language synthesis of the user's situation.

Examples shown in the videos include:

> Manish still hasn't sent the edited videos, and Ayush hasn't sent the write-up he promised for today; several others can wait.

and a variation explaining that Manish and Ayush still owe deliverables while the 83(b) mailing and a pitch update remain on the user's side.

This is a crucial product distinction. Kyra is not merely displaying all detected tasks. It is forming a judgment about ownership, urgency, and what can wait, then expressing that judgment as a concise briefing.

### 4. The open-loop surface

The right side is labeled **TO BE DONE** and contains a list of open loops. Every visible item has:

- a short action-oriented title;
- a longer explanation of the source situation;
- a circular status control;
- enough relational context to identify the people involved.

Visible examples include:

- **Waiting on Manish for the video edits** — the evidence explains that the user followed up and Manish said his editor had started and would send some by morning.
- **Waiting on Ayush for the write-up** — the evidence explains the requested deadline and that the promised material has not arrived.
- **Print, sign, mail the 83(b) form via USPS and send Phalanshu the receipt** — a user-owned real-world obligation reconstructed from a conversation and document exchange.
- **Samarth to sign the doc tonight** — an item currently waiting on somebody else.
- **Update RC on how the pitch/meeting went** — a follow-up that remains on the user after a partial reply.

These examples demonstrate that Kyra models at least two directions of responsibility:

- **Waiting on someone:** another person owes the user an action.
- **On me:** the user owes another person or must complete an external action.

The evidence text is as important as the title. A generic task manager might retain “follow up with Manish”; Kyra retains why the task exists, what was already said, what was promised, and when the situation should change.

A number near the right edge changes from `37` to `38` after a task is added. It likely represents the open-loop count, but the videos do not explicitly confirm the label or navigation behavior.

### 5. The action surface

The central command palette asks:

> What do you wanna get done?

The resting interface shows two explicit commands:

- `/cal` — “what and when — e.g. standup tomorrow 9am”
- `/task` — “what needs doing” or “add to what's waiting”

The command is `/cal`, not `/call`.

The X walkthrough demonstrates the user typing `/task have a meeting w abc`. Kyra creates the item, shows an **Added** confirmation, and places the new loop in the right-hand list with the provenance **Added by you**.

That provenance separates manually entered work from work inferred from communications. This is a small but meaningful trust feature: the user can understand why an item exists and whether Kyra or the user introduced it.

The visible captions also describe this area as a place to “chat” about creating work. The interaction combines an explicit slash command, which establishes intent, with natural language, which supplies flexible details.

## What each walkthrough establishes

### X walkthrough: Image 17 to Image 7

The X sequence establishes the core interaction model:

1. `Command + K` reveals the full-screen Kyra layer.
2. The left calendar can expand into a multi-day view.
3. The center gives a prioritized Night briefing.
4. The right side shows open loops derived from WhatsApp and Gmail.
5. `/cal` and `/task` provide direct action paths.
6. A user-created task is added to the open-loop list and marked **Added by you**.
7. Clicking away dismisses Kyra and restores the underlying desktop.

This demonstration most clearly explains Kyra as an ambient command-and-awareness layer: enter, understand, act, leave.

### LinkedIn walkthrough: Image 6 to Image 1

The LinkedIn sequence gives more detail about planning and the broader interaction direction:

1. `Command + K` opens the same core shell.
2. The calendar expands to the three-day planning grid.
3. Meetings and execution blocks can be moved, resized, created, and recolored.
4. The central command surface supports calendar and task actions conversationally.
5. The product considers obligations originating in relationships, such as a friend asking for an introduction or a family member asking for something.
6. The spoken direction moves toward an interface usable without repeatedly touching a phone.

The software planning surface is relevant to our reconstruction. The hardware and completely hands-free experience are longer-term context, not part of our initial implementation scope.

## Why this is not another todo application

A todo application stores what the user remembers to enter. Kyra attempts to discover what the user may already have forgotten.

A calendar shows allocated time. Kyra connects that time to unresolved obligations and lets the plan be changed in the same context.

An inbox shows messages. Kyra converts the meaning of selected messages into durable, stateful commitments while retaining the source evidence.

A chatbot waits for prompts. Kyra's official promise is that the system comes to the user, closes what it can, and asks for attention only when human judgment is needed.

The key product asset is therefore not the translucent interface. It is the continuously maintained, evidence-backed model of the user's obligations.

## Trust, privacy, and autonomy

The official site makes several privacy claims:

- the user's data stays with the user;
- the user may connect an existing cloud-model subscription, bring their own keys, or use local models;
- personally identifiable information is removed from cloud-model calls.

These are public product promises, not details of an observed implementation. Our reconstruction should treat them as design requirements to investigate rather than as a verified architecture.

The product also needs a clear autonomy boundary. “Closes the ones it can” should not mean silently taking consequential external actions. A trustworthy implementation should distinguish:

- reading and classifying context;
- suggesting an action;
- drafting an action;
- scheduling or executing a reversible action;
- sending messages or performing consequential actions that require explicit approval.

The interface should always preserve source evidence, ownership, proposed action, and execution status so the user can understand and correct Kyra's reasoning.

## Software-first reconstruction

Our goal is not to produce a static visual clone. The reconstruction should prove the complete product loop with representative data and real interaction.

### Core domain objects

The minimum useful model consists of:

- **Source event:** an email, message, calendar change, or user command.
- **Person:** the participant who made or received a commitment.
- **Open loop:** the unresolved obligation Kyra believes exists.
- **Evidence:** the source excerpt and metadata supporting that belief.
- **Owner:** the user, another person, or unresolved/ambiguous ownership.
- **State:** detected, waiting on me, waiting on someone, scheduled, resolved, dismissed, or superseded.
- **Plan block:** time reserved for a meeting, execution, rest, or another category.
- **Action:** a proposed, approved, completed, or failed operation associated with an open loop.

These states are a reconstruction proposal, not a claim about Kyra's private data model.

### Minimum end-to-end experience

A convincing first version should:

1. ingest a bounded set of representative email, message, and calendar events;
2. infer open loops with ownership, due context, and source evidence;
3. deduplicate multiple messages that refer to the same commitment;
4. rank the few loops that require attention now;
5. generate a concise Night briefing explaining the selection;
6. display today's timeline and an editable three-day planner;
7. open globally through a keyboard shortcut;
8. support `/task` and `/cal` with natural-language details;
9. distinguish user-created items from inferred items;
10. allow the user to correct, dismiss, schedule, or resolve a loop;
11. require approval before consequential external actions;
12. reconcile later source activity so fulfilled commitments can close.

### Demonstration acceptance criteria

The first public proof should demonstrate, without hidden manual intervention:

- at least five commitments inferred from seeded communications;
- both “waiting on me” and “waiting on someone” ownership;
- inspectable source evidence for every inferred item;
- a priority briefing that selects a meaningful subset rather than repeating the entire list;
- one new `/task` and one new `/cal` event created from natural language;
- calendar blocks that can be created, moved, resized, and categorized;
- one loop corrected by the user and one closed after new evidence arrives;
- `Command + K` open and click-away or `Escape` dismissal behavior;
- no external message sent without a visible approval step.

## What not to build first

The initial reconstruction should deliberately exclude:

- custom wearable hardware;
- a production-scale integration with every communication platform;
- unrestricted autonomous messaging;
- a generic conversational assistant unrelated to open loops;
- visual polish that substitutes for a working commitment model.

The highest-agency proof is a narrow system that genuinely detects, explains, prioritizes, and closes a small number of commitments end to end.

## Open questions

The public material does not yet establish:

- which communication providers are fully integrated versus represented in a demo;
- how Kyra authenticates with or continuously synchronizes WhatsApp and Gmail;
- whether calendar changes write back to a provider in real time;
- how confidence, due dates, urgency, and ownership are calculated;
- how duplicate or contradictory commitments are reconciled;
- what the `37`/`38` counter exactly represents;
- whether the circular controls directly complete an item;
- which actions Kyra currently performs autonomously;
- how a user reviews the complete action history;
- how the stated local/BYOK/PII-removal privacy model is implemented;
- which differences between the X and LinkedIn demonstrations reflect product versions versus different demo paths.

Until tested or stated publicly, these should remain questions rather than assumptions embedded into the rebuild.

## Product principles for this repository

1. **Proactive, not prompt-dependent.** The system should find important work before the user remembers to ask.
2. **Evidence before assertion.** Every inferred loop should be traceable to its source.
3. **Attention is the scarce resource.** Ranking and restraint matter more than the number of detected tasks.
4. **Ownership must be explicit.** “I owe” and “they owe” are fundamentally different states.
5. **Planning and obligations belong together.** A commitment becomes more actionable when it can be placed into time.
6. **Ambient, not demanding.** Kyra should be quickly available and quickly dismissible.
7. **Trust before autonomy.** The user must understand and approve consequential actions.
8. **Closure is the outcome.** Detection without reconciliation merely creates another inbox.

## Sources

- [Kyra official site and manifesto](https://kyrainterface.com/), accessed August 15, 2026.
- [Sahil Dhull's Kyra walkthrough on X](https://x.com/_sahildhull/status/2088204300938067990).
- [Sahil Dhull's Kyra walkthrough on LinkedIn](https://www.linkedin.com/feed/update/urn:li:activity:7494242106857648128/).
- Seventeen user-supplied screenshots, interpreted in the corrected order Image 17 through Image 1.
