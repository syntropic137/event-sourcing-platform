"""Expected version constants for optimistic concurrency control.

Matches the Rust event store semantics where ``expected_aggregate_nonce``
controls stream creation and versioning:

- ``NO_STREAM`` (0): Stream must not exist — used for uniqueness constraints
  and the stream-per-unique-value pattern.
- ``ANY`` (None): Skip version check entirely.
- ``exact(n)``: Stream must be at exactly version *n*.
"""


class ExpectedVersion:
    """Semantic constants for the ``expected_version`` parameter.

    These map directly to ``expected_aggregate_nonce`` in the gRPC protocol:

    .. code-block:: python

        # Ensure the stream doesn't exist (set-based validation)
        await client.append_events(stream, events, expected_version=ExpectedVersion.NO_STREAM)

        # Require an exact version (standard OCC)
        await client.append_events(stream, events, expected_version=ExpectedVersion.exact(3))

        # Skip version check
        await client.append_events(stream, events, expected_version=ExpectedVersion.ANY)
    """

    NO_STREAM: int = 0
    """Stream must not exist. Maps to ``expected_aggregate_nonce = 0``.

    When used, the event store rejects the append if the stream already
    contains events, raising ``StreamAlreadyExistsError``.
    """

    ANY: None = None
    """Skip version check entirely.

    The gRPC client defaults ``expected_aggregate_nonce`` to 0 when
    ``expected_version`` is ``None``, so this currently has the same
    wire behavior as ``NO_STREAM``. Use explicitly when you want to
    document intent.
    """

    @staticmethod
    def exact(version: int) -> int:
        """Require the stream to be at exactly *version*.

        Args:
            version: Expected stream version (must be >= 1).

        Returns:
            The version number, for use as ``expected_version``.

        Raises:
            ValueError: If *version* < 1.
        """
        if version < 1:
            raise ValueError(f"Exact version must be >= 1, got {version}")
        return version
