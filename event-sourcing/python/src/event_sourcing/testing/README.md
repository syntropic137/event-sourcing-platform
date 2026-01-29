# ES Test Kit - Python

Testing toolkit for event-sourced applications.

## scenario() - Given-When-Then Testing

Test aggregate command handlers using the Given-When-Then pattern.

### Installation

```python
from event_sourcing.testing import scenario
```

### Usage

```python
from event_sourcing.testing import scenario

# Happy path: command produces events
def test_create_order():
    scenario(OrderAggregate) \
        .given_no_prior_activity() \
        .when(CreateOrderCommand(aggregate_id='order-1', customer_id='customer-1')) \
        .expect_events([
            OrderCreatedEvent(order_id='order-1', customer_id='customer-1'),
        ])

# Error path: business rule violated
def test_reject_duplicate_order():
    scenario(OrderAggregate) \
        .given([
            OrderCreatedEvent(order_id='order-1', customer_id='customer-1'),
        ]) \
        .when(CreateOrderCommand(aggregate_id='order-1', customer_id='customer-2')) \
        .expect_exception(BusinessRuleViolationError) \
        .expect_exception_message('Order already exists')

# State verification
def test_track_total():
    def assert_total(aggregate):
        assert aggregate.get_total() == 45.00

    scenario(OrderAggregate) \
        .given([
            OrderCreatedEvent(order_id='order-1', customer_id='customer-1'),
            ItemAddedEvent(product_id='product-1', quantity=2, price=10.00),
        ]) \
        .when(AddItemCommand(aggregate_id='order-1', product_id='product-2', quantity=1, price=25.00)) \
        .expect_state(assert_total)

# Setup via commands instead of events
def test_ship_confirmed_order():
    scenario(OrderAggregate) \
        .given_commands([
            CreateOrderCommand(aggregate_id='order-1', customer_id='customer-1'),
            AddItemCommand(aggregate_id='order-1', product_id='product-1', quantity=1, price=10.00),
            ConfirmOrderCommand(aggregate_id='order-1'),
        ]) \
        .when(ShipOrderCommand(aggregate_id='order-1', tracking_number='TRACK123')) \
        .expect_events([
            OrderShippedEvent(tracking_number='TRACK123'),
        ])
```

### API Reference

| Method | Description |
|--------|-------------|
| `scenario(AggregateClass)` | Create scenario for aggregate type |
| `.given([events])` | Set up prior events |
| `.given_no_prior_activity()` | Start with fresh aggregate |
| `.given_commands([commands])` | Set up via commands |
| `.when(command)` | Execute command under test |
| `.expect_events([events])` | Assert events emitted |
| `.expect_no_events()` | Assert no events emitted |
| `.expect_exception(ErrorClass)` | Assert exception type |
| `.expect_exception_message(msg)` | Assert exception message |
| `.expect_state(callback)` | Assert aggregate state |
| `.expect_successful_handler_execution()` | Assert no exception |

### Type Safety

Full type hints - passes `mypy --strict`:

```python
scenario(OrderAggregate) \      # AggregateScenario[OrderAggregate]
    .given([...]) \             # TestExecutor[OrderAggregate]
    .when(command) \            # ResultValidator[OrderAggregate]
    .expect_state(lambda agg: ...)  # agg typed as OrderAggregate
```

## Future Enhancements

- Replay testing (golden file testing)
- Invariant verification
- Projection testing
- Fixture loading (JSON/YAML test data)
