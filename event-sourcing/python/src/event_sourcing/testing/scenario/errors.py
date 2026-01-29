"""Error types for scenario testing."""


class ScenarioAssertionError(AssertionError):
    """Error thrown when a scenario assertion fails."""

    pass


class ScenarioExecutionError(Exception):
    """Error thrown when scenario execution fails unexpectedly."""

    def __init__(self, message: str, original_error: Exception | None = None) -> None:
        super().__init__(message)
        self.original_error = original_error
