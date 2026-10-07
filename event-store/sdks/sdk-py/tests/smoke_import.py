"""Import smoke test for the installed eventstore-sdk-py package (#367).

Run against an installed copy, never the source tree, with ``python -I`` so
the working directory is not on ``sys.path``:

    make -C event-store smoke-py-import

The generated gRPC module used to import ``eventstore.v1`` as a top-level
package, which only resolved when the generator's output directory happened
to be on ``sys.path``. Installed, it failed with
``ModuleNotFoundError: No module named 'eventstore'``.
"""

from __future__ import annotations

import sys
from pathlib import Path

import sdk_py
from sdk_py.client_rt import EventStoreClientRT
from sdk_py.gen.eventstore.v1 import eventstore_pb2, eventstore_pb2_grpc

package_dir = Path(sdk_py.__file__).resolve().parent
if "site-packages" not in package_dir.parts:
    sys.exit(f"sdk_py imported from {package_dir}, not an installed package")

# The generated modules must not need a top-level 'eventstore' package.
if "eventstore" in sys.modules:
    sys.exit("generated stubs imported a top-level 'eventstore' package")

# The service stub is bound to the messages from the same package.
assert eventstore_pb2_grpc.EventStoreStub is not None
assert eventstore_pb2.AppendRequest.DESCRIPTOR.full_name == "eventstore.v1.AppendRequest"

# The runtime client builds its stub on a lazy channel; nothing connects.
client = EventStoreClientRT("127.0.0.1:1")
assert client.pb is eventstore_pb2
assert hasattr(client.stub, "Append")

print(f"sdk_py import OK from {package_dir}")
