# Notices

Kuro's own code is Kuro's. This file records third-party work that Kuro's
skills were adapted from, because the licences those projects use require the
copyright notice to travel with the material even when it has been rewritten.

Nothing here appears in Kuro's interface. Every skill is presented under Kuro's
own name and wording — the guidance was rewritten in Kuro's voice and format,
not copied — but "rewritten" is not the same as "unrelated", and the honest
record belongs somewhere.

If a skill below is ever removed from `crates/kuro-core/src/skills/mod.rs`, its
entry here should go with it.

---

## impeccable

- Source: https://github.com/pbakaus/impeccable
- Copyright: Paul Bakaus and contributors
- Licence: Apache License 2.0

Adapted into the `design-intent` and `design-refusals` skills.

The upstream project is a router over roughly two dozen reference playbooks plus
a Node context script, which does not fit Kuro's format — a Kuro skill is a
single block of instructions appended to the system prompt under a token budget.
What was adapted is therefore the substance rather than the structure: the four
surface modes (Persuade, Operate, Read, Experience), the rule that a stated
brief outranks the model's own taste, the distinction between refining and
redesigning, the instruction to verify in bounded passes rather than an
open-ended loop, and the list of saturated visual defaults to recognise and
avoid.

Apache-2.0 section 4(b) requires stating that changes were made: the text was
rewritten, condensed, and re-voiced for Kuro; no upstream file is included.

Apache License 2.0 — full text: https://www.apache.org/licenses/LICENSE-2.0
