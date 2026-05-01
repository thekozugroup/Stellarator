"""Research tools for agent fine-tuning method selection."""

import re
from datetime import datetime
from typing import Any

import arxiv
import httpx


class HFPapersClient:
    """Hugging Face Papers API client for research papers."""

    BASE_URL = "https://huggingface.co/api/papers"
    TIMEOUT = 10

    async def search(self, query: str, limit: int = 10) -> list[dict[str, Any]]:
        """Search for papers on Hugging Face.

        Args:
            query: Search query string
            limit: Max results (default 10)

        Returns:
            List of paper objects with id, title, authors, summary, url, published_at, models, datasets
        """
        async with httpx.AsyncClient(timeout=self.TIMEOUT) as client:
            resp = await client.get(
                self.BASE_URL,
                params={"search": query, "limit": limit},
            )
            resp.raise_for_status()
            papers = resp.json()
            if not isinstance(papers, list):
                papers = papers.get("papers", [])
            return [self._normalize_paper(p) for p in papers]

    async def fetch(self, paper_id: str) -> dict[str, Any]:
        """Fetch full paper details including abstract.

        Args:
            paper_id: HF paper ID (e.g., '2401.12345')

        Returns:
            Paper object with full abstract body (max 300 words)
        """
        async with httpx.AsyncClient(timeout=self.TIMEOUT) as client:
            resp = await client.get(f"{self.BASE_URL}/{paper_id}")
            resp.raise_for_status()
            paper = resp.json()

            # Fetch and parse full page for abstract if available
            try:
                page_resp = await client.get(f"https://huggingface.co/papers/{paper_id}")
                page_resp.raise_for_status()
                html = page_resp.text
                # Extract abstract section (basic parsing)
                abstract_match = re.search(
                    r'<h2[^>]*>Abstract</h2>.*?<p[^>]*>(.*?)</p>',
                    html,
                    re.DOTALL | re.IGNORECASE,
                )
                if abstract_match:
                    abstract_text = abstract_match.group(1)
                    # Remove HTML tags
                    abstract_text = re.sub(r'<[^>]+>', '', abstract_text).strip()
                    # Truncate to ~300 words
                    words = abstract_text.split()[:300]
                    paper["abstract_body"] = " ".join(words)
            except Exception:
                pass  # Fallback to summary if parsing fails

            return self._normalize_paper(paper)

    @staticmethod
    def _normalize_paper(paper: dict[str, Any]) -> dict[str, Any]:
        """Normalize HF paper structure to standard schema."""
        return {
            "id": paper.get("id", ""),
            "title": paper.get("title", ""),
            "authors": paper.get("authors", []),
            "summary": paper.get("summary", ""),
            "abstract_body": paper.get("abstract_body"),
            "url": f"https://huggingface.co/papers/{paper.get('id', '')}",
            "published_at": paper.get("publishedAt"),
            "models": paper.get("models", []),
            "datasets": paper.get("datasets", []),
        }


class ArxivClient:
    """arXiv.org client for research papers."""

    async def search(self, query: str, limit: int = 10) -> list[dict[str, Any]]:
        """Search arXiv for papers.

        Args:
            query: Search query string
            limit: Max results (default 10)

        Returns:
            List of paper objects with arxiv_id, title, authors, summary, url, categories, published
        """
        results = []
        try:
            client = arxiv.Client()
            search = arxiv.Search(
                query=query,
                max_results=limit,
                sort_by=arxiv.SortCriterion.SubmittedDate,
                sort_order=arxiv.SortOrder.Descending,
            )
            for paper in client.results(search):
                results.append(self._normalize_paper(paper))
        except Exception:
            pass  # Return empty if arxiv is unavailable
        return results

    async def fetch(self, arxiv_id: str) -> dict[str, Any]:
        """Fetch paper details from arXiv.

        Args:
            arxiv_id: arXiv paper ID (e.g., '2401.12345')

        Returns:
            Paper object with metadata (no full text)
        """
        try:
            client = arxiv.Client()
            paper = next(client.results(arxiv.Search(id_list=[arxiv_id])))
            return self._normalize_paper(paper)
        except Exception:
            return {
                "arxiv_id": arxiv_id,
                "title": "",
                "authors": [],
                "summary": "",
                "url": f"https://arxiv.org/abs/{arxiv_id}",
                "categories": [],
                "published": None,
            }

    @staticmethod
    def _normalize_paper(paper: arxiv.Result) -> dict[str, Any]:
        """Normalize arXiv paper structure to standard schema."""
        return {
            "arxiv_id": paper.entry_id.split("/abs/")[-1],
            "title": paper.title,
            "authors": [author.name for author in paper.authors],
            "summary": paper.summary,
            "url": paper.entry_id,
            "categories": paper.categories,
            "published": paper.published.isoformat() if paper.published else None,
        }
