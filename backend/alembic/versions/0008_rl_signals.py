"""Add reward_mean, percent_correct, checkpoint_url to runs.

Revision ID: 0008_rl_signals
Revises: 0007_run_sandbox_preflight
Create Date: 2026-04-30 00:00:03.000000
"""

from __future__ import annotations

from typing import Sequence, Union

import sqlalchemy as sa
from alembic import op

revision: str = "0008_rl_signals"
down_revision: Union[str, None] = "0007_run_sandbox_preflight"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    with op.batch_alter_table("runs") as batch:
        batch.add_column(sa.Column("reward_mean", sa.Float(), nullable=True))
        batch.add_column(sa.Column("percent_correct", sa.Float(), nullable=True))
        batch.add_column(sa.Column("checkpoint_url", sa.String(length=2048), nullable=True))


def downgrade() -> None:
    with op.batch_alter_table("runs") as batch:
        batch.drop_column("checkpoint_url")
        batch.drop_column("percent_correct")
        batch.drop_column("reward_mean")
