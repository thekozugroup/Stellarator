"""Add is_sandbox, preflight_json, parent_run_id to runs.

Revision ID: 0007_run_sandbox_preflight
Revises: 0006_run_alerts
Create Date: 2026-04-30 00:00:02.000000
"""

from __future__ import annotations

from typing import Sequence, Union

import sqlalchemy as sa
from alembic import op

revision: str = "0007_run_sandbox_preflight"
down_revision: Union[str, None] = "0006_run_alerts"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    with op.batch_alter_table("runs") as batch:
        batch.add_column(
            sa.Column("is_sandbox", sa.Boolean(), nullable=False, server_default="0")
        )
        batch.add_column(sa.Column("preflight_json", sa.JSON(), nullable=True))
        batch.add_column(sa.Column("parent_run_id", sa.String(length=40), nullable=True))
    op.create_index("ix_runs_parent_run_id", "runs", ["parent_run_id"])


def downgrade() -> None:
    op.drop_index("ix_runs_parent_run_id", table_name="runs")
    with op.batch_alter_table("runs") as batch:
        batch.drop_column("parent_run_id")
        batch.drop_column("preflight_json")
        batch.drop_column("is_sandbox")
