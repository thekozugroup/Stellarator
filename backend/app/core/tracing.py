"""OpenTelemetry tracing initialisation for the Stellarator backend.

Call ``init_tracing()`` once at application startup.  The function is fully
defensive: if the ``opentelemetry`` packages are absent **or** if the
``OTEL_EXPORTER_OTLP_ENDPOINT`` environment variable is not set, it returns
silently without raising.
"""

from __future__ import annotations

import logging
import os

logger = logging.getLogger(__name__)


def init_tracing(service_name: str = "stellarator-backend") -> None:  # noqa: C901
    """Initialise OpenTelemetry SDK and instrument FastAPI, SQLAlchemy, HTTPX.

    Safe to call even when the OTel packages are not installed — the function
    catches *all* ``ImportError`` and ``Exception`` instances and logs a
    single INFO line before returning.

    Args:
        service_name: The ``service.name`` resource attribute reported to the
            collector.  Defaults to ``"stellarator-backend"``.
    """
    endpoint = os.environ.get("OTEL_EXPORTER_OTLP_ENDPOINT", "").strip()
    if not endpoint:
        logger.info(
            "OTEL_EXPORTER_OTLP_ENDPOINT not set — distributed tracing disabled"
        )
        return

    try:
        from opentelemetry import trace
        from opentelemetry.sdk.resources import SERVICE_NAME, Resource
        from opentelemetry.sdk.trace import TracerProvider
        from opentelemetry.sdk.trace.export import BatchSpanProcessor
        from opentelemetry.exporter.otlp.proto.http.trace_exporter import (
            OTLPSpanExporter,
        )
        from opentelemetry.instrumentation.fastapi import FastAPIInstrumentor
        from opentelemetry.instrumentation.sqlalchemy import SQLAlchemyInstrumentor
        from opentelemetry.instrumentation.httpx import HTTPXClientInstrumentor
    except ImportError as exc:
        logger.info(
            "opentelemetry packages not installed — distributed tracing disabled "
            "(%s)",
            exc,
        )
        return

    try:
        resource = Resource(attributes={SERVICE_NAME: service_name})
        provider = TracerProvider(resource=resource)

        exporter = OTLPSpanExporter(endpoint=endpoint)
        provider.add_span_processor(BatchSpanProcessor(exporter))

        trace.set_tracer_provider(provider)

        # Instrument FastAPI (must be called before the app is constructed, but
        # instrument() is idempotent so a post-construction call is safe too).
        FastAPIInstrumentor().instrument()

        # Instrument SQLAlchemy — works with both sync and async engines.
        SQLAlchemyInstrumentor().instrument()

        # Instrument HTTPX — covers all outbound API calls (Tinker, OpenAI, etc.).
        HTTPXClientInstrumentor().instrument()

        logger.info(
            "OpenTelemetry tracing enabled (service=%s, endpoint=%s)",
            service_name,
            endpoint,
        )
    except Exception as exc:  # pylint: disable=broad-except
        logger.warning(
            "OpenTelemetry initialisation failed — tracing disabled (%s)", exc
        )
