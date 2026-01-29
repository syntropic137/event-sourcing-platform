"""Tests for Scenario Testing (Given-When-Then)."""

import re

import pytest

from event_sourcing.core.aggregate import AggregateRoot
from event_sourcing.core.event import DomainEvent
from event_sourcing.decorators import command_handler, event_sourcing_handler
from event_sourcing.testing import ScenarioAssertionError, scenario

# ============================================================================
# TEST DOMAIN: Shopping Cart
# ============================================================================


# Events
class CartCreatedEvent(DomainEvent):
    """Cart created event."""

    event_type = "CartCreated"
    cart_id: str


class ItemAddedEvent(DomainEvent):
    """Item added to cart event."""

    event_type = "ItemAdded"
    cart_id: str
    item_id: str
    price: float


class CartSubmittedEvent(DomainEvent):
    """Cart submitted event."""

    event_type = "CartSubmitted"
    cart_id: str
    total: float


CartEvent = CartCreatedEvent | ItemAddedEvent | CartSubmittedEvent


# Commands
class CreateCartCommand:
    """Create a new cart."""

    def __init__(self, aggregate_id: str) -> None:
        self.aggregate_id = aggregate_id


class AddItemCommand:
    """Add item to cart."""

    def __init__(self, aggregate_id: str, item_id: str, price: float) -> None:
        self.aggregate_id = aggregate_id
        self.item_id = item_id
        self.price = price


class SubmitCartCommand:
    """Submit cart for checkout."""

    def __init__(self, aggregate_id: str) -> None:
        self.aggregate_id = aggregate_id


# Business Rule Errors
class BusinessRuleViolationError(Exception):
    """Business rule violation error."""

    pass


# Aggregate
class CartAggregate(AggregateRoot[CartEvent]):
    """Shopping cart aggregate."""

    def __init__(self) -> None:
        super().__init__()
        self.items: list[dict[str, float | str]] = []
        self.submitted = False

    def get_aggregate_type(self) -> str:
        return "Cart"

    @command_handler("CreateCartCommand")
    def create_cart(self, command: CreateCartCommand) -> None:
        if self.id is not None:
            raise BusinessRuleViolationError("Cart already exists")
        self._initialize(command.aggregate_id)
        self._apply(CartCreatedEvent(cart_id=command.aggregate_id))

    @command_handler("AddItemCommand")
    def add_item(self, command: AddItemCommand) -> None:
        if self.id is None:
            raise BusinessRuleViolationError("Cart does not exist")
        if self.submitted:
            raise BusinessRuleViolationError("Cannot add items to submitted cart")
        if command.price <= 0:
            raise BusinessRuleViolationError("Price must be positive")
        self._apply(
            ItemAddedEvent(
                cart_id=command.aggregate_id,
                item_id=command.item_id,
                price=command.price,
            )
        )

    @command_handler("SubmitCartCommand")
    def submit_cart(self, command: SubmitCartCommand) -> None:
        if self.id is None:
            raise BusinessRuleViolationError("Cart does not exist")
        if self.submitted:
            raise BusinessRuleViolationError("Cart already submitted")
        if len(self.items) == 0:
            raise BusinessRuleViolationError("Cannot submit empty cart")
        total = sum(float(item["price"]) for item in self.items)
        self._apply(CartSubmittedEvent(cart_id=command.aggregate_id, total=total))

    @event_sourcing_handler("CartCreated")
    def on_cart_created(self, event: CartCreatedEvent) -> None:
        pass  # Initial state is already set

    @event_sourcing_handler("ItemAdded")
    def on_item_added(self, event: ItemAddedEvent) -> None:
        self.items.append({"item_id": event.item_id, "price": event.price})

    @event_sourcing_handler("CartSubmitted")
    def on_cart_submitted(self, event: CartSubmittedEvent) -> None:
        self.submitted = True

    # Accessors for testing
    def get_item_count(self) -> int:
        return len(self.items)

    def get_total(self) -> float:
        return sum(float(item["price"]) for item in self.items)

    def is_submitted(self) -> bool:
        return self.submitted


# ============================================================================
# TESTS
# ============================================================================


class TestHappyPath:
    """Happy path tests - events emitted."""

    def test_command_produces_expected_events(self) -> None:
        """Should verify command produces expected events."""
        scenario(CartAggregate).given(
            [
                CartCreatedEvent(cart_id="cart-1"),
            ]
        ).when(AddItemCommand("cart-1", "item-1", 29.99)).expect_events(
            [
                ItemAddedEvent(cart_id="cart-1", item_id="item-1", price=29.99),
            ]
        )

    def test_multiple_events_in_sequence(self) -> None:
        """Should verify multiple events in sequence."""
        scenario(CartAggregate).given(
            [
                CartCreatedEvent(cart_id="cart-1"),
                ItemAddedEvent(cart_id="cart-1", item_id="item-1", price=10.00),
                ItemAddedEvent(cart_id="cart-1", item_id="item-2", price=20.00),
            ]
        ).when(SubmitCartCommand("cart-1")).expect_events(
            [
                CartSubmittedEvent(cart_id="cart-1", total=30.00),
            ]
        )

    def test_given_no_prior_activity(self) -> None:
        """Should verify command with given_no_prior_activity()."""
        scenario(CartAggregate).given_no_prior_activity().when(
            CreateCartCommand("cart-new")
        ).expect_events(
            [
                CartCreatedEvent(cart_id="cart-new"),
            ]
        )


class TestErrorPath:
    """Error path tests - exceptions."""

    def test_business_rule_violation_exception(self) -> None:
        """Should verify business rule violation exception."""
        scenario(CartAggregate).given(
            [
                CartCreatedEvent(cart_id="cart-1"),
            ]
        ).when(SubmitCartCommand("cart-1")).expect_exception(
            BusinessRuleViolationError
        ).expect_exception_message(
            "Cannot submit empty cart"
        )

    def test_exception_with_string_containment(self) -> None:
        """Should verify exception with string containment."""
        scenario(CartAggregate).given_no_prior_activity().when(
            SubmitCartCommand("cart-1")
        ).expect_exception(BusinessRuleViolationError).expect_exception_message(
            "does not exist"
        )

    def test_exception_with_regex_pattern(self) -> None:
        """Should verify exception with regex pattern."""
        scenario(CartAggregate).given(
            [
                CartCreatedEvent(cart_id="cart-1"),
                CartSubmittedEvent(cart_id="cart-1", total=0),
            ]
        ).when(SubmitCartCommand("cart-1")).expect_exception(
            BusinessRuleViolationError
        ).expect_exception_message(
            re.compile(r"already submitted", re.IGNORECASE)
        )

    def test_exception_type_only(self) -> None:
        """Should verify exception type only."""
        scenario(CartAggregate).given(
            [
                CartCreatedEvent(cart_id="cart-1"),
            ]
        ).when(AddItemCommand("cart-1", "item-1", -10)).expect_exception(
            BusinessRuleViolationError
        )


class TestNoEvents:
    """No events tests."""

    def test_fail_when_expecting_no_events_but_events_emitted(self) -> None:
        """Should fail when expecting no events but events are emitted."""
        with pytest.raises(ScenarioAssertionError):
            scenario(CartAggregate).given_no_prior_activity().when(
                CreateCartCommand("cart-1")
            ).expect_no_events()


class TestStateVerification:
    """State verification tests."""

    def test_aggregate_state_via_callback(self) -> None:
        """Should verify aggregate state via callback."""

        def assert_state(aggregate: CartAggregate) -> None:
            assert aggregate.get_item_count() == 1
            assert aggregate.get_total() == 29.99
            assert not aggregate.is_submitted()

        scenario(CartAggregate).given(
            [
                CartCreatedEvent(cart_id="cart-1"),
            ]
        ).when(AddItemCommand("cart-1", "item-1", 29.99)).expect_state(assert_state)

    def test_state_after_multiple_events(self) -> None:
        """Should verify state after multiple events."""

        def assert_state(aggregate: CartAggregate) -> None:
            assert aggregate.is_submitted()
            assert aggregate.get_total() == 30.00

        scenario(CartAggregate).given(
            [
                CartCreatedEvent(cart_id="cart-1"),
                ItemAddedEvent(cart_id="cart-1", item_id="item-1", price=10.00),
                ItemAddedEvent(cart_id="cart-1", item_id="item-2", price=20.00),
            ]
        ).when(SubmitCartCommand("cart-1")).expect_state(assert_state)


class TestGivenCommands:
    """given_commands() tests."""

    def test_set_up_aggregate_state_using_commands(self) -> None:
        """Should set up aggregate state using commands."""
        scenario(CartAggregate).given_commands(
            [
                CreateCartCommand("cart-1"),
                AddItemCommand("cart-1", "item-1", 15.00),
                AddItemCommand("cart-1", "item-2", 25.00),
            ]
        ).when(SubmitCartCommand("cart-1")).expect_events(
            [
                CartSubmittedEvent(cart_id="cart-1", total=40.00),
            ]
        )


class TestAssertionFailures:
    """Assertion failure tests."""

    def test_fail_when_expected_events_do_not_match(self) -> None:
        """Should fail when expected events do not match actual events."""
        with pytest.raises(ScenarioAssertionError):
            scenario(CartAggregate).given(
                [
                    CartCreatedEvent(cart_id="cart-1"),
                ]
            ).when(AddItemCommand("cart-1", "item-1", 29.99)).expect_events(
                [
                    ItemAddedEvent(
                        cart_id="cart-1", item_id="item-1", price=99.99
                    ),  # Wrong price
                ]
            )

    def test_fail_when_expected_event_count_does_not_match(self) -> None:
        """Should fail when expected event count does not match."""
        with pytest.raises(ScenarioAssertionError):
            scenario(CartAggregate).given_no_prior_activity().when(
                CreateCartCommand("cart-1")
            ).expect_events(
                [
                    CartCreatedEvent(cart_id="cart-1"),
                    ItemAddedEvent(
                        cart_id="cart-1", item_id="item-1", price=10.00
                    ),  # Extra event
                ]
            )

    def test_fail_when_expecting_exception_but_command_succeeds(self) -> None:
        """Should fail when expecting exception but command succeeds."""
        with pytest.raises(ScenarioAssertionError):
            scenario(CartAggregate).given_no_prior_activity().when(
                CreateCartCommand("cart-1")
            ).expect_exception(BusinessRuleViolationError)

    def test_fail_when_exception_message_does_not_match(self) -> None:
        """Should fail when exception message does not match."""
        with pytest.raises(ScenarioAssertionError):
            scenario(CartAggregate).given(
                [
                    CartCreatedEvent(cart_id="cart-1"),
                ]
            ).when(SubmitCartCommand("cart-1")).expect_exception(
                BusinessRuleViolationError
            ).expect_exception_message(
                "wrong message"
            )


class TestExpectSuccessfulHandlerExecution:
    """expect_successful_handler_execution() tests."""

    def test_pass_when_command_succeeds(self) -> None:
        """Should pass when command succeeds."""
        scenario(CartAggregate).given_no_prior_activity().when(
            CreateCartCommand("cart-1")
        ).expect_successful_handler_execution()

    def test_fail_when_command_throws(self) -> None:
        """Should fail when command throws."""
        with pytest.raises(ScenarioAssertionError):
            scenario(CartAggregate).given_no_prior_activity().when(
                SubmitCartCommand("cart-1")
            ).expect_successful_handler_execution()


class TestChaining:
    """Chaining tests."""

    def test_chain_expect_events_and_expect_state(self) -> None:
        """Should allow chaining expect_events and expect_state."""

        def assert_state(aggregate: CartAggregate) -> None:
            assert aggregate.get_item_count() == 1

        scenario(CartAggregate).given(
            [
                CartCreatedEvent(cart_id="cart-1"),
            ]
        ).when(AddItemCommand("cart-1", "item-1", 29.99)).expect_events(
            [
                ItemAddedEvent(cart_id="cart-1", item_id="item-1", price=29.99),
            ]
        ).expect_state(
            assert_state
        )

    def test_chain_expect_exception_and_expect_exception_message(self) -> None:
        """Should allow chaining expect_exception and expect_exception_message."""
        scenario(CartAggregate).given(
            [
                CartCreatedEvent(cart_id="cart-1"),
            ]
        ).when(SubmitCartCommand("cart-1")).expect_exception(
            BusinessRuleViolationError
        ).expect_exception_message(
            "Cannot submit empty cart"
        )


class TestEdgeCases:
    """Edge case tests."""

    def test_empty_given_events_array(self) -> None:
        """Should handle empty given events array."""
        scenario(CartAggregate).given([]).when(CreateCartCommand("cart-1")).expect_events(
            [
                CartCreatedEvent(cart_id="cart-1"),
            ]
        )

    def test_aggregate_without_prior_state_creating_new_aggregate(self) -> None:
        """Should handle aggregate without prior state creating new aggregate."""

        def assert_state(aggregate: CartAggregate) -> None:
            assert aggregate.id == "brand-new-cart"

        scenario(CartAggregate).given_no_prior_activity().when(
            CreateCartCommand("brand-new-cart")
        ).expect_events(
            [
                CartCreatedEvent(cart_id="brand-new-cart"),
            ]
        ).expect_state(
            assert_state
        )
