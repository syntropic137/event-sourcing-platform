"""Integration tests for repository pattern."""

import pytest

from event_sourcing import (
    AggregateRoot,
    EventStoreRepository,
    MemoryEventStoreClient,
    RepositoryFactory,
)
from event_sourcing.core.errors import ConcurrencyConflictError, InvalidAggregateStateError
from event_sourcing.core.event import DomainEvent
from event_sourcing.decorators import event_sourcing_handler


# Test domain model
class AccountCredited(DomainEvent):
    """Event: account was credited."""

    event_type = "AccountCredited"
    amount: float
    balance: float


class AccountDebited(DomainEvent):
    """Event: account was debited."""

    event_type = "AccountDebited"
    amount: float
    balance: float


class AccountAggregate(AggregateRoot[AccountCredited | AccountDebited]):
    """Test aggregate for banking account."""

    def __init__(self) -> None:
        super().__init__()
        self.balance = 0.0

    def get_aggregate_type(self) -> str:
        return "Account"

    def credit(self, amount: float) -> None:
        """Credit the account."""
        if amount <= 0:
            raise ValueError("Amount must be positive")

        self._raise_event(
            AccountCredited(
                amount=amount,
                balance=self.balance + amount,
            )
        )

    def debit(self, amount: float) -> None:
        """Debit the account."""
        if amount <= 0:
            raise ValueError("Amount must be positive")
        if self.balance < amount:
            raise ValueError("Insufficient funds")

        self._raise_event(
            AccountDebited(
                amount=amount,
                balance=self.balance - amount,
            )
        )

    @event_sourcing_handler("AccountCredited")
    def on_credited(self, event: AccountCredited) -> None:
        self.balance = event.balance

    @event_sourcing_handler("AccountDebited")
    def on_debited(self, event: AccountDebited) -> None:
        self.balance = event.balance


class TestRepositoryLifecycle:
    """Tests for repository CRUD operations."""

    @pytest.fixture
    async def client(self) -> MemoryEventStoreClient:
        """Create and connect event store client."""
        client = MemoryEventStoreClient()
        await client.connect()
        yield client
        await client.disconnect()

    @pytest.fixture
    def repository(
        self, client: MemoryEventStoreClient
    ) -> EventStoreRepository[AccountAggregate]:
        """Create repository."""
        factory = RepositoryFactory(client)
        return factory.create_repository(AccountAggregate)

    @pytest.mark.integration
    async def test_save_new_aggregate(
        self, repository: EventStoreRepository[AccountAggregate]
    ) -> None:
        """Should save a new aggregate."""
        # Create aggregate
        account = AccountAggregate()
        account._initialize("acc-123")
        account.credit(100.0)

        # Save
        await repository.save(account)

        # Verify no uncommitted events
        assert not account.has_uncommitted_events()
        assert account.version == 1

    @pytest.mark.integration
    async def test_load_existing_aggregate(
        self, repository: EventStoreRepository[AccountAggregate]
    ) -> None:
        """Should load an existing aggregate."""
        # Create and save
        account1 = AccountAggregate()
        account1._initialize("acc-456")
        account1.credit(100.0)
        account1.credit(50.0)
        await repository.save(account1)

        # Load
        account2 = await repository.load("acc-456")

        # Verify
        assert account2 is not None
        assert account2.id == "acc-456"
        assert account2.balance == 150.0
        assert account2.version == 2
        assert not account2.has_uncommitted_events()

    @pytest.mark.integration
    async def test_load_nonexistent_aggregate(
        self, repository: EventStoreRepository[AccountAggregate]
    ) -> None:
        """Should return None for nonexistent aggregate."""
        result = await repository.load("nonexistent")
        assert result is None

    @pytest.mark.integration
    async def test_save_and_reload_with_changes(
        self, repository: EventStoreRepository[AccountAggregate]
    ) -> None:
        """Should persist changes across save/load cycles."""
        # Create and save
        account1 = AccountAggregate()
        account1._initialize("acc-789")
        account1.credit(100.0)
        await repository.save(account1)

        # Load and modify
        account2 = await repository.load("acc-789")
        assert account2 is not None
        account2.credit(50.0)
        account2.debit(25.0)
        await repository.save(account2)

        # Reload and verify
        account3 = await repository.load("acc-789")
        assert account3 is not None
        assert account3.balance == 125.0
        assert account3.version == 3

    @pytest.mark.integration
    async def test_save_without_id_fails(
        self, repository: EventStoreRepository[AccountAggregate]
    ) -> None:
        """Should fail to raise events on aggregate without ID."""
        account = AccountAggregate()

        # Should fail when trying to raise event without ID
        with pytest.raises(InvalidAggregateStateError) as exc:
            account.credit(100.0)

        assert "without an ID" in str(exc.value)

    @pytest.mark.integration
    async def test_save_without_changes_is_noop(
        self, repository: EventStoreRepository[AccountAggregate]
    ) -> None:
        """Should not save aggregate with no uncommitted events."""
        # Create and save
        account = AccountAggregate()
        account._initialize("acc-noop")
        account.credit(100.0)
        await repository.save(account)

        # Load (no changes)
        loaded = await repository.load("acc-noop")
        assert loaded is not None

        # Save again (should be no-op)
        await repository.save(loaded)
        assert loaded.version == 1

    @pytest.mark.integration
    async def test_exists(
        self, repository: EventStoreRepository[AccountAggregate]
    ) -> None:
        """Should check if aggregate exists."""
        # Should not exist
        assert not await repository.exists("acc-exists")

        # Create and save
        account = AccountAggregate()
        account._initialize("acc-exists")
        account.credit(100.0)
        await repository.save(account)

        # Should exist
        assert await repository.exists("acc-exists")


class TestOptimisticConcurrency:
    """Tests for optimistic concurrency control."""

    @pytest.fixture
    async def client(self) -> MemoryEventStoreClient:
        """Create and connect event store client."""
        client = MemoryEventStoreClient()
        await client.connect()
        yield client
        await client.disconnect()

    @pytest.fixture
    def repository(
        self, client: MemoryEventStoreClient
    ) -> EventStoreRepository[AccountAggregate]:
        """Create repository."""
        factory = RepositoryFactory(client)
        return factory.create_repository(AccountAggregate)

    @pytest.mark.integration
    async def test_concurrent_modification_fails(
        self, repository: EventStoreRepository[AccountAggregate]
    ) -> None:
        """Should detect concurrent modifications."""
        # Create and save initial aggregate
        account1 = AccountAggregate()
        account1._initialize("acc-concurrent")
        account1.credit(100.0)
        await repository.save(account1)

        # Load twice (simulating two users)
        user1_account = await repository.load("acc-concurrent")
        user2_account = await repository.load("acc-concurrent")

        assert user1_account is not None
        assert user2_account is not None

        # User 1 makes changes and saves
        user1_account.credit(50.0)
        await repository.save(user1_account)

        # User 2 makes changes (same version) and tries to save
        user2_account.debit(25.0)

        # Should fail due to version conflict
        with pytest.raises(ConcurrencyConflictError) as exc:
            await repository.save(user2_account)

        assert exc.value.expected_version == 1
        assert exc.value.actual_version == 2

    @pytest.mark.integration
    async def test_sequential_modifications_succeed(
        self, repository: EventStoreRepository[AccountAggregate]
    ) -> None:
        """Should allow sequential modifications."""
        # Create and save
        account = AccountAggregate()
        account._initialize("acc-sequential")
        account.credit(100.0)
        await repository.save(account)

        # Load, modify, save (User 1)
        account1 = await repository.load("acc-sequential")
        assert account1 is not None
        account1.credit(50.0)
        await repository.save(account1)

        # Load latest, modify, save (User 2)
        account2 = await repository.load("acc-sequential")
        assert account2 is not None
        account2.debit(25.0)
        await repository.save(account2)  # Should succeed

        # Verify final state
        account3 = await repository.load("acc-sequential")
        assert account3 is not None
        assert account3.balance == 125.0
        assert account3.version == 3


class TestRepositoryFactory:
    """Tests for repository factory."""

    @pytest.mark.integration
    async def test_create_repository_with_explicit_type(self) -> None:
        """Should create repository with explicit type name."""
        client = MemoryEventStoreClient()
        await client.connect()

        factory = RepositoryFactory(client)
        repo = factory.create_repository(AccountAggregate, "CustomAccount")

        # Verify stream naming
        account = AccountAggregate()
        account._initialize("test-123")
        account.credit(100.0)
        await repo.save(account)

        # Stream should use custom type
        assert await client.stream_exists("CustomAccount-test-123")

        await client.disconnect()

    @pytest.mark.integration
    async def test_create_repository_with_inferred_type(self) -> None:
        """Should infer type name from class name."""
        client = MemoryEventStoreClient()
        await client.connect()

        factory = RepositoryFactory(client)
        repo = factory.create_repository(AccountAggregate)

        # Verify stream naming
        account = AccountAggregate()
        account._initialize("test-456")
        account.credit(100.0)
        await repo.save(account)

        # Stream should use inferred type (AccountAggregate -> Account)
        assert await client.stream_exists("Account-test-456")

        await client.disconnect()

