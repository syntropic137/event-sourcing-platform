"""Upcasters (ADR-007, ADR-027): same semantics as the Rust SDK's Upcasters."""

from typing import Any

import pytest

from event_sourcing import UpcastError, Upcasters


def add_currency(body: dict[str, Any]) -> dict[str, Any]:
    return {**body, "currency": "EUR"}


class TestRegister:
    def test_single_step(self) -> None:
        up = Upcasters().register("MoneyDeposited", 1, 2, add_currency)
        assert up.upcast("MoneyDeposited", 1, {"amount": 5}) == (
            "MoneyDeposited",
            2,
            {"amount": 5, "currency": "EUR"},
        )

    def test_steps_chain(self) -> None:
        up = (
            Upcasters()
            .register("E", 1, 2, lambda b: {**b, "v2": True})
            .register("E", 2, 3, lambda b: {**b, "v3": True})
        )
        assert up.upcast("E", 1, {}) == ("E", 3, {"v2": True, "v3": True})
        assert up.upcast("E", 2, {}) == ("E", 3, {"v3": True})

    def test_version_zero_is_version_one(self) -> None:
        up = Upcasters().register("E", 1, 2, add_currency)
        assert up.handles("E", 0)
        assert up.upcast("E", 0, {})[1] == 2

    def test_no_matching_step_passes_through(self) -> None:
        up = Upcasters().register("E", 1, 2, add_currency)
        body = {"x": 1}
        assert up.upcast("Other", 1, body) == ("Other", 1, body)
        assert up.upcast("E", 2, body) == ("E", 2, body)
        assert not up.handles("E", 2)

    @pytest.mark.parametrize(("frm", "to"), [(2, 2), (2, 1), (0, 1)])
    def test_rejects_non_increasing_or_zero_versions(self, frm: int, to: int) -> None:
        with pytest.raises(ValueError):
            Upcasters().register("E", frm, to, add_currency)

    def test_rejects_duplicates(self) -> None:
        up = Upcasters().register("E", 1, 2, add_currency)
        with pytest.raises(ValueError, match="duplicate"):
            up.register("E", 1, 3, add_currency)

    @pytest.mark.parametrize("name", ["", "has space", "café"])
    def test_rejects_invalid_type_names(self, name: str) -> None:
        with pytest.raises(ValueError, match="invalid event type"):
            Upcasters().register(name, 1, 2, add_currency)


class TestRename:
    def test_rename_changes_type(self) -> None:
        up = Upcasters().rename("OldName", 1, "NewName", 1, lambda b: b)
        assert up.upcast("OldName", 1, {"a": 1}) == ("NewName", 1, {"a": 1})

    def test_rename_then_register_chains(self) -> None:
        up = (
            Upcasters()
            .rename("OldName", 1, "NewName", 1, lambda b: b)
            .register("NewName", 1, 2, add_currency)
        )
        assert up.upcast("OldName", 1, {}) == ("NewName", 2, {"currency": "EUR"})

    def test_rename_must_change_type(self) -> None:
        with pytest.raises(ValueError, match="use register"):
            Upcasters().rename("E", 1, "E", 2, add_currency)

    def test_cycle_is_an_error(self) -> None:
        up = Upcasters().rename("A", 1, "B", 1, lambda b: b).rename("B", 1, "A", 1, lambda b: b)
        with pytest.raises(UpcastError, match="cycle"):
            up.upcast("A", 1, {})


class TestFailures:
    def test_step_exception_is_upcast_error(self) -> None:
        def boom(_: dict[str, Any]) -> dict[str, Any]:
            raise KeyError("amount")

        up = Upcasters().register("E", 1, 2, boom)
        with pytest.raises(UpcastError) as exc:
            up.upcast("E", 1, {})
        assert exc.value.event_type == "E"
        assert exc.value.event_version == 1
        assert isinstance(exc.value.original_error, KeyError)

    def test_step_returning_non_object_is_upcast_error(self) -> None:
        up = Upcasters().register("E", 1, 2, lambda _b: [1, 2])  # type: ignore[arg-type,return-value]
        with pytest.raises(UpcastError, match="JSON object"):
            up.upcast("E", 1, {})

    def test_non_object_payload_is_upcast_error(self) -> None:
        up = Upcasters().register("E", 1, 2, add_currency)
        with pytest.raises(UpcastError, match="not a JSON object"):
            up.upcast("E", 1, [1])  # type: ignore[arg-type]
