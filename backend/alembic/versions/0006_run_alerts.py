"""Add run_alerts and research_transcripts tables.

Revision ID: 0006_run_alerts
Revises: 0005_oauth_state
Create Date: 2026-04-30 00:00:01.000000
"""

from __future__ import annotations

from typing import Sequence, Union

import sqlalchemy as sa
from alembic import op

revision: str = "0006_run_alerts"
down_revision: Union[str, None] = "0005_oauth_state"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    op.create_table(
        "run_alerts",
        sa.Column("id", sa.Integer(), primary_key=True, autoincrement=True),
        sa.Column(
            "run_id",
            sa.String(length=40),
            sa.ForeignKey("runs.id", ondelete="CASCADE"),
            nullable=False,
        ),
        sa.Column("level", sa.String(length=16), nullable=False),
        sa.Column("title", sa.String(length=200), nullable=False),
        sa.Column("body", sa.Text(), nullable=False, server_default=""),
        sa.Column("source", sa.String(length=64), nullable=False, server_default="training_script"),
        sa.Column("created_at", sa.DateTime(), nullable=False),
    )
    op.create_index("ix_run_alerts_run_id", "run_alerts", ["run_id"])
    op.create_index("ix_run_alerts_run_created", "run_alerts", ["run_id", "created_at"])

    op.create_table(
        "research_transcripts",
        sa.Column("id", sa.Integer(), primary_key=True, autoincrement=True),
        sa.Column("run_id", sa.String(length=40), nullable=True),
        sa.Column("calling_agent", sa.String(length=64), nullable=False),
        sa.Column("task", sa.Text(), nullable=False),
        sa.Column("context", sa.Text(), nullable=False, server_default=""),
        sa.Column("result_json", sa.JSON(), nullable=False),
        sa.Column("started_at", sa.DateTime(), nullable=False),
        sa.Column("completed_at", sa.DateTime(), nullable=True),
    )
    op.create_index("ix_research_transcripts_run_id", "research_transcripts", ["run_id"])
    op.create_index(
        "ix_research_transcripts_calling_agent", "research_transcripts", ["calling_agent"]
    )


def downgrade() -> None:
    op.drop_index("ix_research_transcripts_calling_agent", table_name="research_transcripts")
    op.drop_index("ix_research_transcripts_run_id", table_name="research_transcripts")
    op.drop_table("research_transcripts")
    op.drop_index("ix_run_alerts_run_created", table_name="run_alerts")
    op.drop_index("ix_run_alerts_run_id", table_name="run_alerts")
    op.drop_table("run_alerts")
