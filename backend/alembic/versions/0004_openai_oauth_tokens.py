"""Add openai_tokens table for browser-based OpenAI OAuth login.

Revision ID: 0004_openai_oauth_tokens
Revises: 0002_chat_stream_chunks
Create Date: 2026-04-30 00:00:00.000000

NB: revises 0002 because 0003 (integration_keys) is being added by a parallel
agent. If 0003 has already landed, switch ``down_revision`` to its revision id
before running ``alembic upgrade head``.
"""

from __future__ import annotations

from typing import Sequence, Union

import sqlalchemy as sa
from alembic import op

revision: str = "0004_openai_oauth_tokens"
down_revision: Union[str, None] = "0002_chat_stream_chunks"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    op.create_table(
        "openai_tokens",
        sa.Column("agent_id", sa.String(length=64), primary_key=True),
        sa.Column("access_token", sa.Text(), nullable=False, server_default=""),
        sa.Column("refresh_token", sa.Text(), nullable=False, server_default=""),
        sa.Column("expires_at", sa.DateTime(), nullable=True),
        sa.Column("email", sa.String(length=320), nullable=True),
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
    )


def downgrade() -> None:
    op.drop_table("openai_tokens")
