"""Add composite index on (occurred_at, source) for Escenario 3 queries.

Revision ID: 20261102_02
Revises: 20261021_01
"""

from alembic import op

revision = "20261102_02"
down_revision = "20261021_01"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.create_index(
        "ix_events_occurred_at_source",
        "events",
        ["occurred_at", "source"],
    )


def downgrade() -> None:
    op.drop_index("ix_events_occurred_at_source", table_name="events")
