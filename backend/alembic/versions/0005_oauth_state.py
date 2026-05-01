"""Add oauth_states table for DB-backed PKCE state (multi-worker safe).

Revision ID: 0005_oauth_state
Revises: 0004_openai_oauth_tokens
Create Date: 2026-04-30 00:00:00.000000
"""

from __future__ import annotations

from typing import Sequence, Union

import sqlalchemy as sa
from alembic import op

revision: str = "0005_oauth_state"
down_revision: Union[str, None] = "0004_openai_oauth_tokens"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    op.create_table(
        "oauth_states",
        sa.Column("id", sa.Integer(), primary_key=True, autoincrement=True),
        sa.Column("provider", sa.String(length=32), nullable=False),
        sa.Column("agent_id", sa.String(length=128), nullable=False),
        sa.Column("nonce", sa.String(length=64), nullable=False, unique=True),
        sa.Column("code_verifier", sa.Text(), nullable=False),
        sa.Column("expires_at", sa.DateTime(), nullable=False),
        sa.Column("used_at", sa.DateTime(), nullable=True),
    )
    op.create_index(
        "ix_oauth_states_nonce_expires",
        "oauth_states",
        ["nonce", "expires_at"],
    )


def downgrade() -> None:
    op.drop_index("ix_oauth_states_nonce_expires", table_name="oauth_states")
    op.drop_table("oauth_states")
