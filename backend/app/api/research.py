"""Research API endpoints for paper search and citations."""

from datetime import datetime
from typing import Any

from fastapi import APIRouter, Depends, HTTPException, Query, status
from sqlalchemy.orm import Session

from app.core.auth import CurrentAgent, require_owner
from app.core.db import get_db
from app.models.run import Run
from app.services.research import ArxivClient, HFPapersClient

router = APIRouter(prefix="/v1/research", tags=["research"])

hf_client = HFPapersClient()
arxiv_client = ArxivClient()


@router.get("/papers/search")
async def search_papers(
    q: str = Query(..., description="Search query"),
    source: str = Query("both", description="hf|arxiv|both"),
    limit: int = Query(10, ge=1, le=50),
    agent: str = CurrentAgent,
) -> dict[str, Any]:
    """Search for research papers across Hugging Face and/or arXiv."""
    if source not in ("hf", "arxiv", "both"):
        raise HTTPException(status.HTTP_400_BAD_REQUEST, "source must be hf|arxiv|both")

    results = {"hf": [], "arxiv": []}

    if source in ("hf", "both"):
        try:
            results["hf"] = await hf_client.search(q, limit)
        except Exception as e:
            results["hf_error"] = str(e)

    if source in ("arxiv", "both"):
        try:
            results["arxiv"] = await arxiv_client.search(q, limit)
        except Exception as e:
            results["arxiv_error"] = str(e)

    return results


@router.get("/papers/{source}/{paper_id}")
async def get_paper(
    source: str,
    paper_id: str,
    agent: str = CurrentAgent,
) -> dict[str, Any]:
    """Fetch full details for a specific paper."""
    if source == "hf":
        return await hf_client.fetch(paper_id)
    elif source == "arxiv":
        return await arxiv_client.fetch(paper_id)
    else:
        raise HTTPException(status.HTTP_400_BAD_REQUEST, "source must be hf or arxiv")


@router.post("/runs/{run_id}/cite")
async def cite_paper(
    run_id: str,
    body: dict[str, Any],
    agent: str = CurrentAgent,
    db: Session = Depends(get_db),
) -> dict[str, Any]:
    """Add a paper citation to a run (owner-only)."""
    run = db.query(Run).filter(Run.id == run_id).first()
    if not run:
        raise HTTPException(status.HTTP_404_NOT_FOUND, f"Run {run_id} not found")

    require_owner(run.owner_agent, agent)

    source = body.get("source")
    paper_id = body.get("paper_id")
    note = body.get("note", "")

    if not source or not paper_id:
        raise HTTPException(status.HTTP_400_BAD_REQUEST, "source and paper_id required")

    if source == "hf":
        paper = await hf_client.fetch(paper_id)
    elif source == "arxiv":
        paper = await arxiv_client.fetch(paper_id)
    else:
        raise HTTPException(status.HTTP_400_BAD_REQUEST, "source must be hf or arxiv")

    # Extract title and URL
    title = paper.get("title", "")
    url = paper.get("url", "")

    citation = {
        "source": source,
        "paper_id": paper_id,
        "title": title,
        "url": url,
        "note": note,
        "agent": agent,
        "at": datetime.utcnow().isoformat(),
    }

    # Append to citations list
    if run.citations is None:
        run.citations = []
    run.citations.append(citation)

    db.commit()
    db.refresh(run)

    return {"citation": citation, "citations_count": len(run.citations)}
