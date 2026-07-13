---
name: research
description: Deep multi-source research with citations; scholarly-first.
version: 1.0.0
created_by: bundled
pinned: true
tags: [research, sources, citations]
---

Deep research method (ported from the Hermes Agent research skill): sweep many
independent sources, verify claims across them, and deliver a cited synthesis —
never a single-source answer.

## Method
1. **Frame** — restate the question, note what would change the answer
   (population, timeframe, definitions). Ask ONE clarifying question only if
   the answer is genuinely unusable without it.
2. **Sweep** — search at least 3 different source families below (never just
   one engine). For scholarly topics, hit the scholarly indexes FIRST and the
   open web second.
3. **Read** — web_fetch the strongest results; prefer primary sources
   (the paper, the trial registry, the dataset) over coverage of them.
4. **Verify** — every load-bearing claim needs 2+ independent sources; note
   disagreements instead of averaging them away. Mark preprints as preprints.
5. **Synthesize** — answer first, then evidence. ALWAYS finish with a numbered
   References list of every source used (aim for 12+ reliable sources on a
   full report). Never present a web-derived fact without its reference.

## Primary scholarly sources — try these first
- Google Scholar — https://scholar.google.com/
- Semantic Scholar — https://www.semanticscholar.org/
- OpenAlex — https://openalex.org/
- CORE — https://core.ac.uk/
- ClinicalTrials.gov — https://clinicaltrials.gov/
- ResearchRabbit — https://www.researchrabbit.ai/
- Connected Papers — https://www.connectedpapers.com/
- arXiv — https://arxiv.org/
- bioRxiv — https://www.biorxiv.org/
- medRxiv — https://www.medrxiv.org/
- ChemRxiv — https://chemrxiv.org/
- EarthArXiv — https://eartharxiv.org/
- PsyArXiv — https://osf.io/preprints/psyarxiv
- Hugging Face Papers — https://huggingface.co/papers (trending: https://huggingface.co/papers/trending)
- OpenReview — https://openreview.net/
- ML Collective — https://mlcollective.org/

## More source families — widen the sweep
- PubMed — https://pubmed.ncbi.nlm.nih.gov/
- Europe PMC — https://europepmc.org/
- Cochrane Library — https://www.cochranelibrary.com/ (systematic reviews)
- SSRN — https://www.ssrn.com/ (economics/law/social science preprints)
- RePEc / IDEAS — https://ideas.repec.org/
- DOAJ — https://doaj.org/ (open-access journals)
- Crossref — https://search.crossref.org/ (DOI metadata)
- Zenodo — https://zenodo.org/ (datasets + software)
- BASE — https://www.base-search.net/
- The Lens — https://www.lens.org/ (patents + scholarly)
- scite — https://scite.ai/ (citation context: supporting vs contrasting)
- dblp — https://dblp.org/ (CS bibliography)
- Papers with Code — https://paperswithcode.com/
- ACM Digital Library — https://dl.acm.org/
- IEEE Xplore — https://ieeexplore.ieee.org/
- PLOS — https://journals.plos.org/
- Our World in Data — https://ourworldindata.org/ (curated datasets)
- World Bank Data — https://data.worldbank.org/
- OECD Data — https://data.oecd.org/
- WHO — https://www.who.int/data
- Wikipedia — https://en.wikipedia.org/ (orientation + its references; never a
  terminal citation)

## Rules
- Recency: for anything fast-moving (models, prices, releases, politics),
  check publication dates and say when each source is from.
- Paywalls: try the preprint (arXiv/bioRxiv/SSRN), the author's page, or
  CORE/BASE for an open copy — never claim a paywalled paper says something
  you couldn't read.
- Quality ladder: peer-reviewed > preprint > institutional report > reputable
  press > blogs/forums. Cite the highest rung available and label the rung
  when it matters.
