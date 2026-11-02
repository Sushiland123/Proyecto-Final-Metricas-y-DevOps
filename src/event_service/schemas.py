"""Validation at the HTTP boundary for Escenario 3 event ingestion and analysis."""

import json
import re
from datetime import datetime, timedelta, timezone
from typing import Annotated, Literal

from pydantic import (
    BaseModel,
    ConfigDict,
    Field,
    JsonValue,
    field_validator,
    model_validator,
)

_IDENTIFIER = re.compile(r"[A-Za-z0-9][A-Za-z0-9_-]*\Z", re.ASCII)
_MAX_PAYLOAD_BYTES = 16 * 1024
_MAX_BATCH_EVENTS = 100
_MAX_TIMESERIES_SPAN = {
    "minute": timedelta(days=7),
    "hour": timedelta(days=90),
    "day": timedelta(days=366),
}


def _utc(value: datetime) -> datetime:
    if value.tzinfo is None or value.utcoffset() is None:
        raise ValueError("A timezone offset is required")
    return value.astimezone(timezone.utc)


def _date_input(value: object) -> object:
    # Pydantic otherwise accepts Unix timestamps, which are not RFC3339 inputs.
    if not isinstance(value, (str, datetime)):
        # Pydantic 2 does not convert TypeError from a validator into a 422.
        raise ValueError("An RFC3339 datetime with timezone is required")  # noqa: TRY004
    return value


def _validate_identifier(value: str) -> str:
    if _IDENTIFIER.fullmatch(value) is None:
        raise ValueError("A simple identifier is required")
    return value


def _optional_identifier(value: str | None) -> str | None:
    if value is None:
        return None
    return _validate_identifier(value)


class EventCreate(BaseModel):
    model_config = ConfigDict(extra="forbid")

    source: Annotated[str, Field(min_length=1, max_length=64)]
    type: Annotated[str, Field(min_length=1, max_length=64)]
    external_id: Annotated[str, Field(min_length=1, max_length=128)]
    occurred_at: datetime
    payload: dict[str, JsonValue]

    @field_validator("source", "type")
    @classmethod
    def simple_identifier(cls, value: str) -> str:
        return _validate_identifier(value)

    @field_validator("occurred_at", mode="before")
    @classmethod
    def occurred_at_input(cls, value: object) -> object:
        return _date_input(value)

    @field_validator("occurred_at")
    @classmethod
    def occurred_at_utc(cls, value: datetime) -> datetime:
        return _utc(value)

    @field_validator("payload")
    @classmethod
    def payload_size(cls, value: dict[str, JsonValue]) -> dict[str, JsonValue]:
        try:
            encoded = json.dumps(
                value, ensure_ascii=False, allow_nan=False, separators=(",", ":")
            ).encode("utf-8")
        except (TypeError, ValueError, OverflowError) as exc:
            raise ValueError("Payload must be JSON") from exc
        if len(encoded) > _MAX_PAYLOAD_BYTES:
            raise ValueError("Payload exceeds 16 KiB")
        return value


class EventBatchCreate(BaseModel):
    model_config = ConfigDict(extra="forbid")

    events: Annotated[
        list[EventCreate], Field(min_length=1, max_length=_MAX_BATCH_EVENTS)
    ]


class _WindowQuery(BaseModel):
    model_config = ConfigDict(extra="forbid")

    from_: datetime = Field(alias="from")
    to: datetime

    @field_validator("from_", "to", mode="before")
    @classmethod
    def window_input(cls, value: object) -> object:
        return _date_input(value)

    @field_validator("from_", "to")
    @classmethod
    def window_utc(cls, value: datetime) -> datetime:
        return _utc(value)

    @model_validator(mode="after")
    def ordered_window(self) -> "_WindowQuery":
        if self.from_ >= self.to:
            raise ValueError("The start of the window must be before its end")
        return self


class CountsQuery(_WindowQuery):
    group_by: Literal["type", "source"]


class EventsListQuery(_WindowQuery):
    source: Annotated[str, Field(min_length=1, max_length=64)] | None = None
    type: Annotated[str, Field(min_length=1, max_length=64)] | None = None
    limit: Annotated[int, Field(ge=1, le=100)] = 50
    offset: Annotated[int, Field(ge=0, le=100_000)] = 0

    @field_validator("source", "type")
    @classmethod
    def filter_identifier(cls, value: str | None) -> str | None:
        return _optional_identifier(value)

    @field_validator("limit", "offset", mode="before")
    @classmethod
    def reject_boolean_int(cls, value: object) -> object:
        if isinstance(value, bool):
            raise ValueError("An integer value is required")  # noqa: TRY004
        return value


class TimeseriesQuery(_WindowQuery):
    bucket: Literal["minute", "hour", "day"] = "hour"
    source: Annotated[str, Field(min_length=1, max_length=64)] | None = None
    type: Annotated[str, Field(min_length=1, max_length=64)] | None = None

    @field_validator("source", "type")
    @classmethod
    def filter_identifier(cls, value: str | None) -> str | None:
        return _optional_identifier(value)

    @model_validator(mode="after")
    def bounded_bucket_span(self) -> "TimeseriesQuery":
        max_span = _MAX_TIMESERIES_SPAN[self.bucket]
        if (self.to - self.from_) > max_span:
            raise ValueError(f"Window exceeds maximum span for {self.bucket} bucket")
        return self


class SummaryQuery(_WindowQuery):
    source: Annotated[str, Field(min_length=1, max_length=64)] | None = None
    type: Annotated[str, Field(min_length=1, max_length=64)] | None = None
    numeric_field: Annotated[str, Field(min_length=1, max_length=64)] = "value"

    @field_validator("source", "type")
    @classmethod
    def filter_identifier(cls, value: str | None) -> str | None:
        return _optional_identifier(value)

    @field_validator("numeric_field")
    @classmethod
    def numeric_field_identifier(cls, value: str) -> str:
        return _validate_identifier(value)
