"""initial schema (runs, notes, metrics, budgets, chat, codex tokens).

Revision ID: 0001_initial
Revises:
Create Date: 2026-01-01 00:00:00.000000

"""
from __future__ import annotations

from typing import Sequence, Union

import sqlalchemy as sa
from alembic import op

revision: str = "0001_initial"
down_revision: Union[str, None] = None
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    op.create_table(
        "runs",
        sa.Column("id", sa.String(length=40), primary_key=True),
        sa.Column("owner_agent", sa.String(length=64), nullable=False),
        sa.Column("name", sa.String(length=200), nullable=False),
        sa.Column("status", sa.String(length=32), nullable=False, server_default="queued"),
        sa.Column("base_model", sa.String(length=200), nullable=False),
        sa.Column("method", sa.String(length=32), nullable=False),
        sa.Column("hyperparams", sa.JSON(), nullable=False),
        sa.Column("dataset_mixture", sa.JSON(), nullable=False),
        sa.Column("user_goal", sa.Text(), nullable=False, server_default=""),
        sa.Column("user_context", sa.Text(), nullable=False, server_default=""),
        sa.Column("agent_plan", sa.Text(), nullable=False, server_default=""),
        sa.Column("citations", sa.JSON(), nullable=False),
        sa.Column("tinker_job_id", sa.String(length=120), nullable=True),
        sa.Column("gpu_type", sa.String(length=32), nullable=False, server_default="H100"),
        sa.Column("gpu_count", sa.Integer(), nullable=False, server_default="1"),
        sa.Column("gpu_seconds", sa.Float(), nullable=False, server_default="0"),
        sa.Column("cost_usd", sa.Float(), nullable=False, server_default="0"),
        sa.Column("created_at", sa.DateTime(), nullable=False),
        sa.Column("started_at", sa.DateTime(), nullable=True),
        sa.Column("finished_at", sa.DateTime(), nullable=True),
    )
    op.create_index("ix_runs_owner_agent", "runs", ["owner_agent"])
    op.create_index("ix_runs_status", "runs", ["status"])
    op.create_index("ix_runs_tinker_job_id", "runs", ["tinker_job_id"])
    op.create_index("ix_runs_owner_status", "runs", ["owner_agent", "status"])

    op.create_table(
        "run_notes",
        sa.Column("id", sa.Integer(), primary_key=True, autoincrement=True),
        sa.Column(
            "run_id",
            sa.String(length=40),
            sa.ForeignKey("runs.id", ondelete="CASCADE"),
            nullable=False,
        ),
        sa.Column("author_agent", sa.String(length=64), nullable=False),
        sa.Column("kind", sa.String(length=32), nullable=False),
        sa.Column("body", sa.Text(), nullable=False),
        sa.Column("created_at", sa.DateTime(), nullable=False),
    )
    op.create_index("ix_run_notes_run_id", "run_notes", ["run_id"])

    op.create_table(
        "run_metrics",
        sa.Column("id", sa.Integer(), primary_key=True, autoincrement=True),
        sa.Column(
            "run_id",
            sa.String(length=40),
            sa.ForeignKey("runs.id", ondelete="CASCADE"),
            nullable=False,
        ),
        sa.Column("step", sa.Integer(), nullable=False, server_default="0"),
        sa.Column("name", sa.String(length=64), nullable=False),
        sa.Column("value", sa.Float(), nullable=False),
        sa.Column("created_at", sa.DateTime(), nullable=False),
    )
    op.create_index("ix_run_metrics_run_id", "run_metrics", ["run_id"])
    op.create_index(
        "uq_run_metrics_run_step_name",
        "run_metrics",
        ["run_id", "step", "name"],
        unique=True,
    )

    op.create_table(
        "budgets",
        sa.Column("id", sa.Integer(), primary_key=True, autoincrement=True),
        sa.Column("scope", sa.String(length=32), nullable=False),
        sa.Column("scope_id", sa.String(length=128), nullable=False),
        sa.Column("monthly_limit_usd", sa.Float(), nullable=True),
        sa.Column("daily_limit_usd", sa.Float(), nullable=True),
        sa.Column("alert_threshold_pct", sa.Float(), nullable=False, server_default="80"),
        sa.Column("created_at", sa.DateTime(), nullable=False),
    )
    op.create_index("ix_budgets_scope", "budgets", ["scope"])
    op.create_index("ix_budgets_scope_id", "budgets", ["scope_id"])

    op.create_table(
        "chat_sessions",
        sa.Column("id", sa.String(length=40), primary_key=True),
        sa.Column("agent", sa.String(length=64), nullable=False),
        sa.Column("system_prompt", sa.Text(), nullable=False, server_default=""),
        sa.Column("created_at", sa.DateTime(), nullable=False),
    )
    op.create_index("ix_chat_sessions_agent", "chat_sessions", ["agent"])

    op.create_table(
        "chat_messages",
        sa.Column("id", sa.Integer(), primary_key=True, autoincrement=True),
        sa.Column(
            "session_id",
            sa.String(length=40),
            sa.ForeignKey("chat_sessions.id", ondelete="CASCADE"),
            nullable=False,
        ),
        sa.Column("role", sa.String(length=16), nullable=False),
        sa.Column("content", sa.Text(), nullable=False, server_default=""),
        sa.Column("tool_calls_json", sa.Text(), nullable=True),
        sa.Column("created_at", sa.DateTime(), nullable=False),
    )
    op.create_index("ix_chat_messages_session_id", "chat_messages", ["session_id"])
    op.create_index(
        "ix_chat_messages_session_created",
        "chat_messages",
        ["session_id", "created_at"],
    )

    op.create_table(
        "codex_tokens",
        sa.Column("agent_id", sa.String(length=64), primary_key=True),
        sa.Column("access_token", sa.Text(), nullable=False),
        sa.Column("refresh_token", sa.Text(), nullable=False, server_default=""),
        sa.Column("expires_at", sa.DateTime(), nullable=True),
        sa.Column("updated_at", sa.DateTime(), nullable=False),
    )


def downgrade() -> None:
    op.drop_table("codex_tokens")
    op.drop_index("ix_chat_messages_session_created", table_name="chat_messages")
    op.drop_index("ix_chat_messages_session_id", table_name="chat_messages")
    op.drop_table("chat_messages")
    op.drop_index("ix_chat_sessions_agent", table_name="chat_sessions")
    op.drop_table("chat_sessions")
    op.drop_index("ix_budgets_scope_id", table_name="budgets")
    op.drop_index("ix_budgets_scope", table_name="budgets")
    op.drop_table("budgets")
    op.drop_index("uq_run_metrics_run_step_name", table_name="run_metrics")
    op.drop_index("ix_run_metrics_run_id", table_name="run_metrics")
    op.drop_table("run_metrics")
    op.drop_index("ix_run_notes_run_id", table_name="run_notes")
    op.drop_table("run_notes")
    op.drop_index("ix_runs_owner_status", table_name="runs")
    op.drop_index("ix_runs_tinker_job_id", table_name="runs")
    op.drop_index("ix_runs_status", table_name="runs")
    op.drop_index("ix_runs_owner_agent", table_name="runs")
    op.drop_table("runs")
