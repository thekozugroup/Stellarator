"""Add integration_keys table for per-agent encrypted API keys.

Revision ID: 0003_integration_keys
Revises: 0002_chat_stream_chunks
Create Date: 2026-04-30 00:00:00.000000
"""

from __future__ import annotations

from typing import Sequence, Union

import sqlalchemy as sa
from alembic import op

revision: str = "0003_integration_keys"
down_revision: Union[str, None] = "0002_chat_stream_chunks"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    op.create_table(
        "integration_keys",
        sa.Column("id", sa.Integer(), primary_key=True, autoincrement=True),
        sa.Column("agent_id", sa.String(length=128), nullable=False),
        sa.Column("kind", sa.String(length=32), nullable=False),
        sa.Column("ciphertext", sa.Text(), nullable=False),
        sa.Column(
            "created_at",
            sa.DateTime(),
            nullable=False,
            server_default=sa.text("CURRENT_TIMESTAMP"),
        ),
        sa.Column(
            "updated_at",
            sa.DateTime(),
            nullable=False,
            server_default=sa.text("CURRENT_TIMESTAMP"),
        ),
        sa.Column("last_used_at", sa.DateTime(), nullable=True),
    )
    # Composite unique: one key per (agent, kind).
    op.create_index(
        "uq_integration_keys_agent_kind",
        "integration_keys",
        ["agent_id", "kind"],
        unique=True,
    )
    # Fast lookup by agent.
    op.create_index(
        "ix_integration_keys_agent_id",
        "integration_keys",
        ["agent_id"],
    )


def downgrade() -> None:
    op.drop_index("ix_integration_keys_agent_id", table_name="integration_keys")
    op.drop_index("uq_integration_keys_agent_kind", table_name="integration_keys")
    op.drop_table("integration_keys")
