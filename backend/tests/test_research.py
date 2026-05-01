"""Tests for research API and services."""

import json
from datetime import datetime
from unittest.mock import AsyncMock, MagicMock, patch

import arxiv
import pytest
from fastapi.testclient import TestClient
from sqlalchemy import create_engine
from sqlalchemy.orm import Session, sessionmaker

from app.core.db import Base
from app.main import app
from app.models.run import Run
from app.services.research import ArxivClient, HFPapersClient


# Test database setup
@pytest.fixture
def test_db():
    """Create in-memory test database."""
    engine = create_engine("sqlite:///:memory:")
    Base.metadata.create_all(bind=engine)
    SessionLocal = sessionmaker(bind=engine)
    db = SessionLocal()
    yield db
    db.close()


@pytest.fixture
def client(test_db):
    """FastAPI test client with mocked DB."""

    def override_get_db():
        yield test_db

    app.dependency_overrides[
        __import__("app.core.db", fromlist=["get_db"]).get_db
    ] = override_get_db
    yield TestClient(app)
    app.dependency_overrides.clear()


@pytest.fixture
def mock_agent_header():
    """Mock auth header."""
    return {"Authorization": "Bearer test-agent-token"}


# HFPapersClient tests
@pytest.mark.asyncio
async def test_hf_papers_search():
    """Test HF Papers API search."""
    client = HFPapersClient()

    mock_response = {
        "papers": [
            {
                "id": "2401.12345",
                "title": "Test Paper",
                "authors": ["Author One", "Author Two"],
                "summary": "This is a test paper.",
                "publishedAt": "2024-01-15T00:00:00Z",
                "models": ["bert"],
                "datasets": ["wikitext"],
            }
        ]
    }

    with patch("httpx.AsyncClient.get") as mock_get:
        mock_response_obj = AsyncMock()
        mock_response_obj.json.return_value = mock_response
        mock_response_obj.raise_for_status.return_value = None
        mock_get.return_value.__aenter__.return_value = mock_response_obj

        results = await client.search("transformers", limit=10)

    assert len(results) == 1
    assert results[0]["id"] == "2401.12345"
    assert results[0]["title"] == "Test Paper"
    assert "url" in results[0]
    assert len(results[0]["authors"]) == 2


@pytest.mark.asyncio
async def test_hf_papers_fetch():
    """Test HF Papers fetch with abstract parsing."""
    client = HFPapersClient()

    mock_api_response = {
        "id": "2401.12345",
        "title": "Test Paper",
        "authors": ["Author One"],
        "summary": "Summary",
        "publishedAt": "2024-01-15T00:00:00Z",
    }

    mock_html = """
    <html>
    <h2>Abstract</h2>
    <p>This is a long abstract with many words that should be truncated to approximately three hundred words to avoid overwhelming the user with too much information about the paper contents.</p>
    </html>
    """

    with patch("httpx.AsyncClient.get") as mock_get:
        mock_response_obj = AsyncMock()
        mock_response_obj.json.return_value = mock_api_response
        mock_response_obj.text = mock_html
        mock_response_obj.raise_for_status.return_value = None
        mock_get.return_value.__aenter__.return_value = mock_response_obj

        result = await client.fetch("2401.12345")

    assert result["id"] == "2401.12345"
    assert "abstract_body" in result
    assert result["url"] == "https://huggingface.co/papers/2401.12345"


# ArxivClient tests
@pytest.mark.asyncio
async def test_arxiv_search():
    """Test arXiv API search."""
    client = ArxivClient()

    mock_paper = MagicMock()
    mock_paper.entry_id = "http://arxiv.org/abs/2401.12345v1"
    mock_paper.title = "Test arXiv Paper"
    mock_paper.authors = [
        MagicMock(name="Author One"),
        MagicMock(name="Author Two"),
    ]
    mock_paper.summary = "arXiv test summary"
    mock_paper.categories = ["cs.LG", "cs.AI"]
    mock_paper.published = datetime(2024, 1, 15)

    with patch("arxiv.Client") as mock_client_class:
        mock_client_inst = MagicMock()
        mock_client_class.return_value = mock_client_inst
        mock_client_inst.results.return_value = [mock_paper]

        results = await client.search("transformers", limit=10)

    assert len(results) == 1
    assert results[0]["arxiv_id"] == "2401.12345v1"
    assert results[0]["title"] == "Test arXiv Paper"
    assert len(results[0]["authors"]) == 2


@pytest.mark.asyncio
async def test_arxiv_fetch():
    """Test arXiv fetch."""
    client = ArxivClient()

    mock_paper = MagicMock()
    mock_paper.entry_id = "http://arxiv.org/abs/2401.12345v1"
    mock_paper.title = "Fetch Test"
    mock_paper.authors = [MagicMock(name="Author")]
    mock_paper.summary = "Test summary"
    mock_paper.categories = ["cs.LG"]
    mock_paper.published = datetime(2024, 1, 15)

    with patch("arxiv.Client") as mock_client_class:
        mock_client_inst = MagicMock()
        mock_client_class.return_value = mock_client_inst
        mock_client_inst.results.return_value = [mock_paper]

        result = await client.fetch("2401.12345v1")

    assert result["arxiv_id"] == "2401.12345v1"
    assert result["title"] == "Fetch Test"


# API endpoint tests
def test_search_papers_hf_only(client, mock_agent_header, test_db):
    """Test /papers/search endpoint with HF source."""
    with patch.object(HFPapersClient, "search", new_callable=AsyncMock) as mock_search:
        mock_search.return_value = [
            {
                "id": "2401.12345",
                "title": "Test",
                "authors": [],
                "summary": "",
                "url": "https://huggingface.co/papers/2401.12345",
                "published_at": "2024-01-15T00:00:00Z",
                "models": [],
                "datasets": [],
            }
        ]

        response = client.get(
            "/v1/research/papers/search?q=test&source=hf",
            headers=mock_agent_header,
        )

    assert response.status_code == 200
    data = response.json()
    assert "hf" in data
    assert len(data["hf"]) == 1


def test_search_papers_both_sources(client, mock_agent_header, test_db):
    """Test /papers/search with both HF and arXiv."""
    with patch.object(HFPapersClient, "search", new_callable=AsyncMock) as mock_hf, \
         patch.object(ArxivClient, "search", new_callable=AsyncMock) as mock_arxiv:
        mock_hf.return_value = [
            {
                "id": "2401.1",
                "title": "HF Paper",
                "authors": [],
                "summary": "",
                "url": "https://huggingface.co/papers/2401.1",
                "published_at": "2024-01-15T00:00:00Z",
                "models": [],
                "datasets": [],
            }
        ]
        mock_arxiv.return_value = [
            {
                "arxiv_id": "2401.12345",
                "title": "arXiv Paper",
                "authors": [],
                "summary": "",
                "url": "http://arxiv.org/abs/2401.12345",
                "categories": [],
                "published": "2024-01-15T00:00:00Z",
            }
        ]

        response = client.get(
            "/v1/research/papers/search?q=test&source=both",
            headers=mock_agent_header,
        )

    assert response.status_code == 200
    data = response.json()
    assert len(data["hf"]) == 1
    assert len(data["arxiv"]) == 1


def test_get_paper_hf(client, mock_agent_header, test_db):
    """Test GET /papers/{source}/{paper_id} for HF."""
    with patch.object(HFPapersClient, "fetch", new_callable=AsyncMock) as mock_fetch:
        mock_fetch.return_value = {
            "id": "2401.12345",
            "title": "Test Paper",
            "authors": ["Author"],
            "summary": "Summary",
            "url": "https://huggingface.co/papers/2401.12345",
            "published_at": "2024-01-15T00:00:00Z",
            "models": [],
            "datasets": [],
        }

        response = client.get(
            "/v1/research/papers/hf/2401.12345",
            headers=mock_agent_header,
        )

    assert response.status_code == 200
    data = response.json()
    assert data["title"] == "Test Paper"


def test_cite_paper_owner_only(client, mock_agent_header, test_db):
    """Test POST /runs/{run_id}/cite is owner-gated."""
    # Create a run owned by different agent
    run = Run(
        id="test-run-1",
        owner_agent="other-agent",
        name="Test Run",
        base_model="gpt2",
        method="sft",
    )
    test_db.add(run)
    test_db.commit()

    response = client.post(
        "/v1/research/runs/test-run-1/cite",
        json={"source": "arxiv", "paper_id": "2401.12345", "note": "relevant"},
        headers=mock_agent_header,
    )

    assert response.status_code == 403
    assert "may read but not mutate" in response.json()["detail"]


def test_cite_paper_success(client, mock_agent_header, test_db):
    """Test successful citation append."""
    # Create run owned by test agent
    run = Run(
        id="test-run-1",
        owner_agent="test-agent",
        name="Test Run",
        base_model="gpt2",
        method="sft",
    )
    test_db.add(run)
    test_db.commit()

    with patch.object(ArxivClient, "fetch", new_callable=AsyncMock) as mock_fetch:
        mock_fetch.return_value = {
            "arxiv_id": "2401.12345",
            "title": "Important Paper",
            "authors": ["Author"],
            "summary": "Summary",
            "url": "http://arxiv.org/abs/2401.12345",
            "categories": ["cs.LG"],
            "published": "2024-01-15T00:00:00Z",
        }

        response = client.post(
            "/v1/research/runs/test-run-1/cite",
            json={"source": "arxiv", "paper_id": "2401.12345", "note": "key methodology"},
            headers=mock_agent_header,
        )

    assert response.status_code == 200
    data = response.json()
    assert data["citations_count"] == 1
    assert data["citation"]["source"] == "arxiv"
    assert data["citation"]["paper_id"] == "2401.12345"
    assert data["citation"]["note"] == "key methodology"
    assert "agent" in data["citation"]
    assert "at" in data["citation"]

    # Verify citation was persisted
    test_db.refresh(run)
    assert len(run.citations) == 1
    assert run.citations[0]["title"] == "Important Paper"


def test_cite_paper_missing_run(client, mock_agent_header, test_db):
    """Test citation on non-existent run."""
    response = client.post(
        "/v1/research/runs/nonexistent/cite",
        json={"source": "arxiv", "paper_id": "2401.12345"},
        headers=mock_agent_header,
    )

    assert response.status_code == 404


def test_invalid_source_error(client, mock_agent_header, test_db):
    """Test error on invalid source parameter."""
    response = client.get(
        "/v1/research/papers/search?q=test&source=invalid",
        headers=mock_agent_header,
    )

    assert response.status_code == 400
