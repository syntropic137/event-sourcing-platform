"""Query handling and read model patterns for CQRS."""

from abc import ABC, abstractmethod
from typing import Any, Generic, Protocol, TypeVar

TResult = TypeVar("TResult", covariant=True)


class Query(Protocol):
    """Base protocol for queries."""

    pass


class QueryHandler(Protocol, Generic[TResult]):
    """Protocol for query handlers."""

    async def handle(self, query: Query) -> TResult:
        """
        Handle a query and return the result.

        Args:
            query: The query to handle

        Returns:
            The query result
        """
        ...


class QueryBus(ABC):
    """Abstract base class for query buses."""

    @abstractmethod
    async def send(self, query: Query) -> Any:
        """
        Send a query to its handler.

        Args:
            query: The query to send

        Returns:
            The query result
        """
        ...

    @abstractmethod
    def register_handler(self, query_type: type[Query], handler: QueryHandler[Any]) -> None:
        """
        Register a query handler.

        Args:
            query_type: The query type
            handler: The handler for this query type
        """
        ...
