from sidecar.main import main
from sidecar.protocol import (
    dispatch_request,
    handle_embed_batch,
    handle_embed_query,
    handle_health,
)

__all__ = [
    "dispatch_request",
    "handle_health",
    "handle_embed_query",
    "handle_embed_batch",
    "main",
]
