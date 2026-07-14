# Butler Mode: making diagrams, barge-in, and background noise behave

_2026-07-12._ A run of Butler Mode bugs, what actually caused each one, and how
they were fixed. The interesting part: several looked like the same bug ("the
diagram won't show") but had three completely different causes. The way we told
them apart was by reading what the model _actually_ emitted, not by guessing.

## How we debugged it

Every voice turn is persisted to the session store, so the ground truth is on
disk. When a diagram "didn't show", we read the last assistant reply straight
from `~/.regent/state.db` and looked at the raw text:

```python
import sqlite3
con = sqlite3.connect('file:state.db?mode=ro&immutable=1', uri=True)
row = con.execute(
    "SELECT content FROM messages WHERE role='assistant' "
    "AND content LIKE '%History of Indonesia%' ORDER BY rowid DESC LIMIT 1"
).fetchone()
print(row[0][:120])   # -> ```json\n{"type":"timeline","title":"History of Indonesia"...
```

That one query settled every argument. If the reply already contained a valid
` ```json ` block, the bug was in the front-end. If it didn't, the bug was in
the prompt (the model chose not to draw one) or the model wrote it somewhere
else. Read the output first; theorise second.

## The diagrams

Butler draws a diagram when the model puts a small JSON "present spec" in its
reply — `{"type":"timeline", ...}` inside a fenced block. The front-end
(`shared/diagram/presentSpec.ts`) pulls that block out, validates it, and renders
it. Three separate things were breaking that.

### 1. The diagram showed up _after_ the explanation

The prompt told the model to put the block **last**. So the spoken prose
streamed and started talking first, and the picture only appeared once the block
finished — well into the reply. It read as "text first, then diagram".

Fix: the prompt now says **lead with the block, then speak**, and the caption
stripper (`stripPresentTail`) drops a _leading_ block so the caption still shows
the prose that follows it. The picture is on screen before Butler starts talking.

### 2. The model saved the diagram to a file

Once asked for the history of Vietnam, the model built a perfect timeline spec —
and then called a file-writing tool and dropped it at
`~/.regent/artifacts/vietnam-history-timeline/timeline.json`. Nothing rendered,
because the renderer only reads the reply text, never the disk.

Fix: a hard guardrail in the prompt. The block must be **inline in the spoken
reply**; the model may not write it to a file, save it as an artifact, or reach
for `write_file` / `create_file` / `image_generation` / any tool. "A spec written
to disk renders nothing on screen."

### 3. One stray `}` threw away the whole diagram

This was the big one, and it wasn't the model's fault. For "History of Indonesia"
the model emitted a flawless 9-step timeline — followed by one extra closing
brace inside the fence: `{ ...valid... }}`. The front-end parsed with
`JSON.parse(body.trim())`, which is all-or-nothing: a single trailing character
throws, and the entire good diagram was silently dropped to prose.

That's model-independent — _any_ model that fumbles one brace loses its diagram.

Fix: `parseFirstObject` in `presentSpec.ts`. It tries a clean `JSON.parse` first,
and on failure does a string-aware, brace-balanced scan that extracts the **first
complete object** and ignores whatever trailing junk follows (a duplicate brace,
a stray comma, a sentence left after the spec). The strict cap validation still
runs afterwards, so this only widens what gets _in_, never what's accepted.

```
```json
{"type":"timeline","title":"T","steps":["A","B"]}}   <- trailing } used to kill this
```
```

### 4. A place question was showing a diagram instead of the map

Asking about a place should open the globe, not a diagram. But a diagram spec had
unconditional priority, so if the model volunteered one for a "where is…" turn,
the map never opened.

Fix: in `useButlerCall.ts`, a place question now **owns the stage**. If the
heard text has a place candidate, any diagram spec is suppressed (both mid-stream
and at turn end) and the geocoder raises the map. The prompt also tells the model
not to draw a diagram for where/geography/location questions in the first place.

## Barge-in and background noise (the VAD gates)

All the voice gates live in `features/butler/domain/vad.ts` as pure math, tuned
against the noise floor so a quiet or heavily-processed mic still works.

**Barge-in on a quiet mic.** Onset (starting a turn) had long adapted down to
`0.006` for a quiet mic, but barge-in (interrupting Butler) still had a hard
`0.01` floor. A soft voice landed in the gap: loud enough to _start_ a turn, too
soft to _interrupt_ one. Barge-in now rides the same adaptive onset math, so a
quiet voice can cut Butler off — while still sitting `3.5×` above the ambient
floor so Butler's own audio can't self-trip it.

**Background noise starting spurious turns.** Onset was _capped_ at `0.015`, so
in a genuinely loud room (ambient above that) the gate sat _below_ the noise and
the room itself read as speech. Onset now tracks above ambient at any level, so
steady noise never crosses it. This only changes behaviour above `~0.0125`
ambient (a loud room); quiet and normal rooms are byte-identical.

A property test locks the guarantee in: at every noise level, an input sitting
_at_ the measured floor crosses neither gate.

## NVIDIA, briefly

Along the way: the new NVIDIA key was fine all along — proven live (auth 200, and
a real completion in 0.3–0.6s on other models). Only the specific model
`z-ai/glm-5.2` was dead (chat completions hang). The key and routing were never
the problem; that one model's endpoint was down. `deepseek-ai/deepseek-v4-flash`
was added to the NVIDIA catalog (verified as a real NIM slug) so it's pickable in
settings.

## Where each fix lives

| Fix | File | Takes effect on |
|---|---|---|
| Lead with the block; no-file guardrail; must-emit; place carve-out | `regent-agent/src/domain/prompts.rs` | deacon rebuild + voice-server restart |
| Lenient parse; strip a leading block | `Desktop/shared/diagram/presentSpec.ts` | app reload |
| Place beats diagram | `Desktop/features/butler/viewmodels/useButlerCall.ts` | app reload |
| Adaptive barge-in; onset above ambient | `Desktop/features/butler/domain/vad.ts` | app reload |
| DeepSeek v4 flash in the NVIDIA catalog | `regent-deacon/src/domain/config/provider_catalog.rs` | deacon rebuild |

Two operational notes worth remembering: the voice server spawns its deacon
**once** and holds it for its whole life (and survives desktop-app restarts), so a
Rust/prompt change needs the voice server itself restarted — not just the app.
And the Butler front-end changes need a full app **reload**, not HMR, because the
VAD loop lives in an effect with empty deps that Fast Refresh won't re-run.
