---
name: arxiv
description: "Search arXiv papers by keyword, author, category, or ID."
version: 1.0.0
created_by: bundled
pinned: true
tags: [research, arxiv, papers, academic]
---

Search and retrieve academic papers from arXiv's free REST API via
`terminal` + `curl`. No API key, no dependencies.

## Quick reference

| Action | Command |
|---|---|
| Search papers | `curl "https://export.arxiv.org/api/query?search_query=all:QUERY&max_results=5"` |
| Get specific paper | `curl "https://export.arxiv.org/api/query?id_list=2402.03300"` |
| Read abstract | `web_fetch("https://arxiv.org/abs/2402.03300")` |
| Read full paper | `web_fetch("https://arxiv.org/pdf/2402.03300")` |

## Searching
The API returns Atom XML. Parse with `python3` (if available) or `grep`/
`sed` one-liners.

```bash
curl -s "https://export.arxiv.org/api/query?search_query=all:GRPO+reinforcement+learning&max_results=5&sortBy=submittedDate&sortOrder=descending" \
  | python3 -c "
import sys, xml.etree.ElementTree as ET
ns = {'a': 'http://www.w3.org/2005/Atom'}
root = ET.parse(sys.stdin).getroot()
for i, e in enumerate(root.findall('a:entry', ns)):
    title = e.find('a:title', ns).text.strip().replace(chr(10), ' ')
    aid = e.find('a:id', ns).text.strip().split('/abs/')[-1]
    published = e.find('a:published', ns).text[:10]
    authors = ', '.join(a.find('a:name', ns).text for a in e.findall('a:author', ns))
    print(f'{i+1}. [{aid}] {title}\n   Authors: {authors} | {published}\n   PDF: https://arxiv.org/pdf/{aid}\n')
"
```

## Query syntax

| Prefix | Searches | Example |
|---|---|---|
| `all:` | All fields | `all:transformer+attention` |
| `ti:` | Title | `ti:large+language+models` |
| `au:` | Author | `au:vaswani` |
| `abs:` | Abstract | `abs:reinforcement+learning` |
| `cat:` | Category | `cat:cs.AI` |
| `co:` | Comment | `co:accepted+NeurIPS` |

Boolean: `+` = AND, `all:GPT+OR+all:BERT` = OR, `all:X+ANDNOT+all:Y` = AND
NOT, `ti:"chain+of+thought"` = exact phrase, `au:hinton+AND+cat:cs.LG` =
combined.

## Sort and pagination
`sortBy`: relevance | lastUpdatedDate | submittedDate. `sortOrder`:
ascending | descending. `start`: 0-based offset. `max_results`: default 10,
max 30000.

```bash
curl -s "https://export.arxiv.org/api/query?search_query=cat:cs.AI&sortBy=submittedDate&sortOrder=descending&max_results=10"
```

## Fetching specific papers
```bash
curl -s "https://export.arxiv.org/api/query?id_list=2402.03300"
curl -s "https://export.arxiv.org/api/query?id_list=2402.03300,2401.12345"   # comma-separated
```

## Reading content
```
web_fetch("https://arxiv.org/abs/2402.03300")   # abstract page, fast
web_fetch("https://arxiv.org/pdf/2402.03300")   # full paper
```
For a downloaded PDF, use the `documents` skill's `read_document` path
instead.

## ID versioning
`arxiv.org/abs/1706.03762` resolves to the latest version;
`.../1706.03762v1` pins a specific one. Preserve the version suffix you
actually read when citing — later versions can change substantially. The
API's `<id>` field returns the versioned URL.

## Withdrawn papers
The `<summary>` field carries a withdrawal notice ("withdrawn"/"retracted")
when a paper's been pulled — check it before treating a result as valid.

---

## Semantic Scholar — citations, related papers, author profiles
arXiv has no citation graph. Semantic Scholar does — free, no key for basic
use (1 req/sec), JSON.

```bash
# Paper details + citation counts (by arXiv ID)
curl -s "https://api.semanticscholar.org/graph/v1/paper/arXiv:2402.03300?fields=title,authors,citationCount,influentialCitationCount,year,abstract"

# Who cited it
curl -s "https://api.semanticscholar.org/graph/v1/paper/arXiv:2402.03300/citations?fields=title,authors,year,citationCount&limit=10"

# What it cites
curl -s "https://api.semanticscholar.org/graph/v1/paper/arXiv:2402.03300/references?fields=title,authors,year,citationCount&limit=10"

# Search (JSON, alternative to arXiv search)
curl -s "https://api.semanticscholar.org/graph/v1/paper/search?query=GRPO+reinforcement+learning&limit=5&fields=title,authors,year,citationCount,externalIds"

# Recommendations
curl -s -X POST "https://api.semanticscholar.org/recommendations/v1/papers/" \
  -H "Content-Type: application/json" \
  -d '{"positivePaperIds": ["arXiv:2402.03300"], "negativePaperIds": []}'

# Author profile
curl -s "https://api.semanticscholar.org/graph/v1/author/search?query=Yann+LeCun&fields=name,hIndex,citationCount,paperCount"
```
Useful fields: `title`, `authors`, `year`, `abstract`, `citationCount`,
`referenceCount`, `influentialCitationCount`, `isOpenAccess`,
`openAccessPdf`, `fieldsOfStudy`, `externalIds`.

## Workflow
1. Discover — arXiv search, `sortBy=submittedDate` for latest
2. Assess impact — Semantic Scholar `citationCount`/`influentialCitationCount`
3. Read abstract — `web_fetch` the `/abs/` page
4. Read full paper — `web_fetch` the `/pdf/` page
5. Find related work — Semantic Scholar `/references`
6. Track authors — Semantic Scholar author search

## Rate limits
arXiv: ~1 req/3s. Semantic Scholar: 1 req/s (100/s with an API key).

## Notes
- Old-format IDs (`hep-th/0601001`) vs new (`2402.03300`)
- PDF: `arxiv.org/pdf/{id}` · Abstract: `arxiv.org/abs/{id}` · HTML (when
  available): `arxiv.org/html/{id}`
- Full category list: https://arxiv.org/category_taxonomy — common ones:
  `cs.AI`, `cs.CL` (NLP), `cs.CV`, `cs.LG`, `cs.CR`, `stat.ML`, `math.OC`

*Adapted from Hermes Agent (MIT, © 2025 Nous Research).*
