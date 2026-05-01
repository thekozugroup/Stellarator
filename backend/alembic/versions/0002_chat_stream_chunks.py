"""Add chat_stream_chunks table for SSE stream durability.

Revision ID: 0002_chat_stream_chunks
Revises: 0001_initial
Create Date: 2026-04-30 00:00:00.000000
"""

from __future__ import annotations

from typing import Sequence, Union

import sqlalchemy as sa
from alembic import op

revision: str = "0002_chat_stream_chunks"
down_revision: Union[str, None] = "0001_initial"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    op.create_table(
        "chat_stream_chunks",
        sa.Column("id", sa.Integer(), primary_key=True, autoincrement=True),
        sa.Column(
            "message_id",
            sa.Integer(),
            sa.ForeignKey("chat_messages.id", ondelete="CASCADE"),
            nullable=False,
        ),
        sa.Column("seq", sa.Integer(), nullable=False),
        sa.Column("kind", sa.String(length=16), nullable=False),
        sa.Column("payload", sa.Text(), nullable=False, server_default=""),
        sa.Column(
            "created_at",
            sa.DateTime(),
            nullable=False,
            server_default=sa.text("CURRENT_TIMESTAMP"),
        ),
    )
    # Unique constraint on (message_id, seq) — also serves as the lookup index.
    op.create_index(
        "uq_chat_stream_chunks_msg_seq",
        "chat_stream_chunks",
        ["message_id", "seq"],
        unique=True,
    )
    # Fast resume scans by message_id.
    op.create_index(
        "ix_chat_stream_chunks_message_id",
        "chat_stream_chunks",
        ["message_id"],
    )


def downgrade() -> None:
    op.drop_index("ix_chat_stream_chunks_message_id", table_name="chat_stream_chunks")
    op.drop_index("uq_chat_stream_chunks_msg_seq", table_name="chat_stream_chunks")
    op.drop_table("chat_stream_chunks")
