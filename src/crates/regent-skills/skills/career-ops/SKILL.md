---
name: career-ops
description: "Job search ops: score offers, tailor CVs, draft outreach."
version: 1.0.0
created_by: bundled
pinned: true
tags: [career, jobs, cv, applications, outreach]
---

Run a job search as a quality-first pipeline: score every listing, apply only
to strong fits, tailor honestly, and NEVER submit anything yourself.

## Hard rules (before any mode)
- **Never auto-submit.** Applications, emails, LinkedIn messages — everything
  stops at a draft the user reviews and sends.
- **Source of truth:** the user's CV file and what they say in conversation.
  Keywords get reformulated, never fabricated — reframe and reorder real
  experience; never invent claims, metrics, or titles.
- **Quality over volume:** a well-targeted application to 5 companies beats a
  generic blast to 50. Discourage anything scoring below the cutoff.

## Evaluate a listing (the default mode)
Score 5 dimensions, each 1.0–5.0, then average:
1. **Role fit** — target archetypes, required skills, seniority.
2. **Team & culture** — engineering culture, interview process, growth.
3. **Compensation & location** — salary/equity/remote vs the user's targets.
4. **Company health** — funding, stability, market position, stage.
5. **Process quality** — recruiter responsiveness, timeline clarity, red flags.

Verdicts: **≥4.0 apply** · 3.0–4.0 mixed, name the trade-offs · **<3.0 skip**
(apply only on explicit user override). Present the score table with one line
of evidence per dimension, then the verdict.

Before scoring, confirm the posting is live: `web_fetch` the URL — a title,
description, and apply button mean active; a footer-only page means closed.

## Scan job portals (no API keys)
Public ATS APIs, one `terminal` curl each — parse with `jq`:
- Greenhouse: `https://boards-api.greenhouse.io/v1/boards/<company>/jobs?content=true`
- Lever: `https://api.lever.co/v0/postings/<company>?mode=json`
- Ashby: `https://api.ashbyhq.com/posting-api/job-board/<org>?includeCompensation=true`
The `<company>` slug is usually the careers-page subdomain. Filter titles by
the user's archetypes, then evaluate the survivors as above.

## Tailor a CV
1. Read the user's CV (ask where it lives on first use; suggest keeping one
   canonical markdown CV).
2. Diff the job description's skills against it: existing / adjacent /
   genuine gap. Emphasize existing, reframe adjacent, never paper over gaps.
3. ATS discipline: mirror the JD's exact keyword spellings where truthful,
   standard section names (Experience, Education, Skills), no tables or
   graphics, one column.
4. Produce the deliverable with `create_document` (format `pdf`, the tailored
   sections) — offer `docx` when the portal asks for editable files.

## Cover letters & outreach
- Cover letter: 3 short paragraphs — the specific hook (why THIS company),
  the strongest matching evidence from the CV, the close. No generic praise.
- Application email: subject `<Role> — <Name>`, two paragraphs, CV attached.
- Hiring-contact note (LinkedIn-style): ≤80 words, name the role, one
  concrete relevant achievement, a soft ask. Draft only — the user sends it.

## Deep company research (on request)
Six axes, via `web_search` + `web_fetch`: AI/product strategy · recent moves
(funding, launches, layoffs) · engineering culture · likely challenges ·
competitors · the user's candidate angle. One paragraph each, cited.

## Track the pipeline
Keep a simple markdown tracker the user owns (suggest `applications.md`):
`| date | company | role | score | status | next action |` — statuses:
evaluated → applied → screening → interviewing → offer / rejected / ghosted.
Update it on every evaluation and every status change the user reports.

*Adapted from career-ops by santifer (MIT) — methodology distilled for
Regent's built-in tools; the full system lives at github.com/santifer/career-ops.*
