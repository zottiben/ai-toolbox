# templates/design/

Skeletons for the **design layer** — the upstream, design-first artifacts a
*greenfield* project needs before there's code to harvest. Fill them yourself or
with Claude Design; keep each one lean (a compass, not a novel).

| Template | For | Captures |
|---|---|---|
| `concept.md` | any project | vision, core loop, pillars + anti-pillars, scope/non-goals, MVP tiers |
| `design-system.md` | any project with UI | brand voice + colour/type/spacing tokens, components, usage rules |
| `system-gdd.md` | games | per-system deep dive (8 sections: rules, formulas, edge cases, tuning, acceptance) |
| `product-brief.md` | sites & SaaS | problem, users/JTBD, flows, screens & states, requirements |

## How it wires in

The design guide is the **source of truth** while there's no code yet. The
generator (`toolbox-init`, greenfield mode) scaffolds these into a project's
`design/` folder and writes a thin `AGENTS.md` whose **Design** section points at
them. Every coding session then builds *against* the guide.

Two life-stages, one body of knowledge:
- **Greenfield:** the design guide leads; code follows it.
- **Brownfield:** as code lands, conventions graduate into `AGENTS.md` hard rules,
  and `design-system.md` tokens graduate into a code conformance gate (as in the
  `relik` example).

Harvested from the CCGS / chief-of-geese pattern — **the artifact shapes, not the
49-agent machinery.** No approval gates, no review swarms; just good skeletons.
