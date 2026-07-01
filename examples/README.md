# examples/ — reference AGENTS.md packs

Real, source-verified `AGENTS.md` files — proof the format works, and a model to
imitate when writing your own.

| Example | Stack | Shows |
|---|---|---|
| `relik-AGENTS.md` | Bun / Go / Expo / Supabase SaaS (brownfield) | hard rules an AI can't guess — RLS on new tables, draft-filtered aggregates, billed-tag guard, never-edit generated types |
| `keepy-uppy-AGENTS.md` | Unity 6 game (brownfield) | game-dev rules — empty-scene/code-gen, asmdef boundaries, Godot-port magic numbers, stubbed Steam |

## How to use

Read them as a **model**, not something to copy verbatim (they're specific to
those projects). When writing a new project's `AGENTS.md`, match their **shape and
altitude**: a tight header (stack + layout + real commands), then a short list of
*only the non-obvious, get-it-wrong-without-being-told* rules — each with the rule,
why it matters, and how it's enforced. If yours grows much past ~100 lines, run
the `lint` skill on it.
