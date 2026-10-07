"""
Subscription management for projections.

This module provides the SubscriptionCoordinator which manages
event subscriptions across multiple projections.
"""

from event_sourcing.core.errors import ProjectionHandlerFailedError, SubscriptionHaltedError
from event_sourcing.subscriptions.coordinator import SubscriptionCoordinator

__all__ = [
    "ProjectionHandlerFailedError",
    "SubscriptionCoordinator",
    "SubscriptionHaltedError",
]
