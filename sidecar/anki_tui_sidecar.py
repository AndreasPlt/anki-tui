from __future__ import annotations

import json
import re
import sys
import traceback
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable


SOUND_RE = re.compile(r"\[sound:([^\]]+)\]")


class SidecarError(Exception):
    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code
        self.message = message


@dataclass
class Session:
    deck_id: int | None = None
    deck_name: str = ""
    dry_run: bool = False
    skipped_card_ids: set[int] | None = None

    def __post_init__(self) -> None:
        if self.skipped_card_ids is None:
            self.skipped_card_ids = set()


class AnkiEngine:
    def __init__(self) -> None:
        self.col: Any = None
        self.media_dir: Path | None = None
        self.session = Session()

    def open_collection(self, collection_path: str, media_dir: str | None = None) -> dict[str, Any]:
        try:
            from anki.collection import Collection
        except Exception as exc:  # pragma: no cover - depends on local env
            raise SidecarError(
                "missing_dependency",
                "Python package 'anki' is not available. Run the sidecar through 'uv run'.",
            ) from exc

        path = Path(collection_path).expanduser()
        if not path.exists():
            raise SidecarError("collection_not_found", f"collection not found: {path}")

        self.col = Collection(str(path))
        self.media_dir = Path(media_dir).expanduser() if media_dir else path.parent / "collection.media"
        return {"opened": True}

    def list_decks(self) -> dict[str, Any]:
        self._require_collection()
        decks = self._decks_from_due_tree()
        if decks is None:
            decks = self._decks_from_manager()
        decks.sort(key=_deck_sort_key)
        return {"decks": decks}

    def start_review(self, deck_id: int, dry_run: bool = False) -> dict[str, Any]:
        self._require_collection()
        deck_name = self._deck_name(deck_id)
        self._select_deck(deck_id)
        self.session = Session(deck_id=deck_id, deck_name=deck_name, dry_run=dry_run)
        return self._snapshot()

    def answer_card(self, card_id: int, rating: int) -> dict[str, Any]:
        self._require_collection()
        if rating not in (1, 2, 3, 4):
            raise SidecarError("invalid_rating", f"invalid rating: {rating}")

        if self.session.dry_run:
            self.session.skipped_card_ids.add(int(card_id))
            return self._snapshot()

        card = self._get_card_by_id(card_id)
        sched = self.col.sched
        if hasattr(sched, "answerCard"):
            sched.answerCard(card, rating)
        elif hasattr(sched, "build_answer") and hasattr(sched, "answer_card"):
            states = self._states_for_card(card)
            answer = sched.build_answer(card=card, states=states, rating=rating)
            sched.answer_card(answer)
        else:  # pragma: no cover - depends on Anki version
            raise SidecarError("unsupported_anki", "Anki scheduler has no supported answer method")

        return self._snapshot()

    def close(self) -> None:
        if self.col is not None:
            close = getattr(self.col, "close", None)
            if callable(close):
                close()
            self.col = None

    def _snapshot(self) -> dict[str, Any]:
        self._require_collection()
        counts = self._counts_for_selected_deck()
        card = self._next_card()
        return {
            "deck_id": self.session.deck_id,
            "deck_name": self.session.deck_name,
            "counts": counts,
            "card": self._card_payload(card) if card is not None else None,
        }

    def _require_collection(self) -> None:
        if self.col is None:
            raise SidecarError("not_open", "collection is not open")

    def _decks_from_due_tree(self) -> list[dict[str, Any]] | None:
        sched = self.col.sched
        tree = None
        if hasattr(sched, "deck_due_tree"):
            tree = sched.deck_due_tree()
        elif hasattr(sched, "deckDueTree"):
            tree = sched.deckDueTree()
        if tree is None:
            return None

        rows: list[dict[str, Any]] = []
        self._flatten_due_tree(tree, rows)
        return rows

    def _flatten_due_tree(self, node: Any, rows: list[dict[str, Any]], parent_name: str = "") -> None:
        deck_id = _first_attr(node, "deck_id", "id", "did")
        raw_name = _first_attr(node, "name", "deck_name")
        if isinstance(node, dict):
            deck_id = deck_id if deck_id is not None else node.get("deck_id") or node.get("id")
            raw_name = raw_name if raw_name is not None else node.get("name")

        name = ""
        if deck_id is not None:
            name = self._deck_name(int(deck_id))
        if not name and raw_name:
            raw = str(raw_name).replace("\x1f", "::")
            name = raw if not parent_name or "::" in raw else f"{parent_name}::{raw}"

        if deck_id is not None and int(deck_id) != 0 and name:
            rows.append(
                {
                    "id": int(deck_id),
                    "name": name,
                    "new_count": _int_attr(node, 0, "new_count", "new", "new_count_today"),
                    "learn_count": _int_attr(node, 0, "learn_count", "learning", "lrn_count"),
                    "review_count": _int_attr(node, 0, "review_count", "review", "rev_count"),
                }
            )

        children = _first_attr(node, "children")
        if isinstance(node, dict):
            children = children if children is not None else node.get("children")
        for child in children or []:
            self._flatten_due_tree(child, rows, name)

    def _decks_from_manager(self) -> list[dict[str, Any]]:
        decks = self.col.decks
        all_decks = decks.all_names_and_ids() if hasattr(decks, "all_names_and_ids") else decks.all()
        rows = []
        for deck in all_decks:
            deck_id = _first_attr(deck, "id")
            name = _first_attr(deck, "name")
            if isinstance(deck, dict):
                deck_id = deck.get("id")
                name = deck.get("name")
            if deck_id is None or name is None:
                continue
            rows.append(
                {
                    "id": int(deck_id),
                    "name": str(name).replace("\x1f", "::"),
                    "new_count": 0,
                    "learn_count": 0,
                    "review_count": 0,
                }
            )
        return rows

    def _select_deck(self, deck_id: int) -> None:
        decks = self.col.decks
        if hasattr(decks, "select"):
            decks.select(int(deck_id))
        elif hasattr(decks, "select_for_id"):
            decks.select_for_id(int(deck_id))
        elif hasattr(decks, "set_current"):
            decks.set_current(int(deck_id))
        else:  # pragma: no cover - depends on Anki version
            raise SidecarError("unsupported_anki", "Anki deck manager has no supported select method")

    def _deck_name(self, deck_id: int) -> str:
        decks = self.col.decks
        if hasattr(decks, "name"):
            name = decks.name(int(deck_id))
            if name:
                return str(name).replace("\x1f", "::")
        for deck in self._decks_from_manager():
            if deck["id"] == int(deck_id):
                return deck["name"]
        return str(deck_id)

    def _counts_for_selected_deck(self) -> dict[str, int]:
        if self.session.deck_id is None:
            return {"new": 0, "learn": 0, "review": 0}
        for deck in self.list_decks()["decks"]:
            if deck["id"] == self.session.deck_id:
                counts = {
                    "new": int(deck.get("new_count", 0)),
                    "learn": int(deck.get("learn_count", 0)),
                    "review": int(deck.get("review_count", 0)),
                }
                if self.session.dry_run:
                    remaining_skip = len(self.session.skipped_card_ids or set())
                    for key in ("learn", "review", "new"):
                        take = min(counts[key], remaining_skip)
                        counts[key] -= take
                        remaining_skip -= take
                return counts
        return {"new": 0, "learn": 0, "review": 0}

    def _next_card(self) -> Any | None:
        for card in self._queued_cards(limit=100):
            if int(card.id) not in (self.session.skipped_card_ids or set()):
                return card
        if self.session.dry_run:
            return None

        sched = self.col.sched
        if hasattr(sched, "getCard"):
            return sched.getCard()
        if hasattr(sched, "get_card"):
            return sched.get_card()
        return None

    def _queued_cards(self, limit: int) -> list[Any]:
        sched = self.col.sched
        if hasattr(sched, "get_queued_cards"):
            queued = _call_with_supported_kwargs(
                sched.get_queued_cards,
                fetch_limit=limit,
                intraday_learning_only=False,
            )
            return self._load_backend_cards(queued)
        if hasattr(sched, "getQueuedCards"):
            return self._load_backend_cards(sched.getQueuedCards(limit))
        return []

    def _load_backend_cards(self, queued: Any) -> list[Any]:
        raw_cards = _extract_cards(queued)
        cards = []
        for raw_card in raw_cards:
            if callable(getattr(raw_card, "question", None)):
                cards.append(raw_card)
                continue
            try:
                from anki.cards import Card
                card = Card(self.col)
                card._load_from_backend_card(raw_card)
                card.start_timer()
                cards.append(card)
            except Exception:
                continue
        return cards

    def _card_payload(self, card: Any) -> dict[str, Any]:
        question_html = card.question() if callable(getattr(card, "question", None)) else ""
        answer_html = card.answer() if callable(getattr(card, "answer", None)) else ""
        return {
            "id": int(card.id),
            "question_html": question_html,
            "answer_html": answer_html,
            "front_audio": _unique(SOUND_RE.findall(question_html) + self._av_filenames(card.question_av_tags())),
            "back_audio": _unique(SOUND_RE.findall(answer_html) + self._av_filenames(card.answer_av_tags())),
            "buttons": self._buttons_for_card(card),
        }

    def _av_filenames(self, av_tags: Iterable[Any]) -> list[str]:
        filenames = []
        for tag in av_tags:
            filename = _first_attr(tag, "filename", "sound_or_video")
            if filename:
                filenames.append(str(filename))
        return filenames

    def _buttons_for_card(self, card: Any) -> list[dict[str, Any]]:
        labels = {1: "Again", 2: "Hard", 3: "Good", 4: "Easy"}
        enabled = self._answer_buttons(card)
        states = self._states_for_card(card)
        return [
            {
                "rating": rating,
                "label": labels[rating],
                "interval": self._interval_for_rating(card, rating, states),
                "enabled": rating in enabled,
            }
            for rating in (1, 2, 3, 4)
        ]

    def _answer_buttons(self, card: Any) -> set[int]:
        sched = self.col.sched
        if hasattr(sched, "answerButtons"):
            buttons = sched.answerButtons(card)
            return set(range(1, int(buttons) + 1))
        if hasattr(sched, "answer_buttons"):
            buttons = sched.answer_buttons(card)
            return set(range(1, int(buttons) + 1))
        return {1, 2, 3, 4}

    def _states_for_card(self, card: Any) -> Any:
        sched = self.col.sched
        if hasattr(sched, "get_scheduling_states"):
            return sched.get_scheduling_states(card)
        if hasattr(sched, "getSchedulingStates"):
            return sched.getSchedulingStates(card)
        return None

    def _interval_for_rating(self, card: Any, rating: int, states: Any) -> str:
        sched = self.col.sched
        if hasattr(sched, "nextIvlStr"):
            return _strip_direction_markers(str(sched.nextIvlStr(card, rating)))
        if hasattr(sched, "next_ivl_str"):
            return _strip_direction_markers(str(sched.next_ivl_str(card, rating)))

        state = self._state_for_rating(states, rating)
        if state is not None:
            desc = _first_attr(state, "description", "interval", "scheduled_secs", "scheduled_days")
            if desc is not None:
                return _strip_direction_markers(str(desc))
        return ""

    def _state_for_rating(self, states: Any, rating: int) -> Any:
        if states is None:
            return None
        names = {1: ("again", "again_state"), 2: ("hard", "hard_state"), 3: ("good", "good_state"), 4: ("easy", "easy_state")}
        for name in names[rating]:
            state = _first_attr(states, name)
            if state is not None:
                return state
        if isinstance(states, dict):
            for name in names[rating]:
                if name in states:
                    return states[name]
        return None

    def _get_card_by_id(self, card_id: int) -> Any:
        try:
            from anki.cards import Card
        except Exception as exc:  # pragma: no cover - depends on local env
            raise SidecarError("missing_dependency", "Python package 'anki' is not available") from exc
        card = Card(self.col, int(card_id))
        if callable(getattr(card, "start_timer", None)):
            card.start_timer()
        return card


class RpcServer:
    def __init__(self, engine: AnkiEngine) -> None:
        self.engine = engine

    def handle(self, request: dict[str, Any]) -> dict[str, Any]:
        req_id = request.get("id")
        method = request.get("method")
        params = request.get("params") or {}
        try:
            if method == "open_collection":
                result = self.engine.open_collection(
                    params["collection_path"],
                    params.get("media_dir"),
                )
            elif method == "list_decks":
                result = self.engine.list_decks()
            elif method == "start_review":
                result = self.engine.start_review(
                    int(params["deck_id"]),
                    bool(params.get("dry_run", False)),
                )
            elif method == "answer_card":
                result = self.engine.answer_card(int(params["card_id"]), int(params["rating"]))
            elif method == "shutdown":
                self.engine.close()
                result = {"shutdown": True}
            else:
                raise SidecarError("unknown_method", f"unknown method: {method}")
            return {"id": req_id, "ok": True, "result": result}
        except SidecarError as exc:
            return {"id": req_id, "ok": False, "error": {"code": exc.code, "message": exc.message}}
        except Exception as exc:
            traceback.print_exc(file=sys.stderr)
            return {"id": req_id, "ok": False, "error": {"code": "anki_error", "message": str(exc)}}


def serve(stdin: Any = sys.stdin, stdout: Any = sys.stdout, engine: AnkiEngine | None = None) -> None:
    server = RpcServer(engine or AnkiEngine())
    for line in stdin:
        line = line.strip()
        if not line:
            continue
        try:
            request = json.loads(line)
        except json.JSONDecodeError as exc:
            response = {"id": None, "ok": False, "error": {"code": "invalid_json", "message": str(exc)}}
        else:
            response = server.handle(request)
        stdout.write(json.dumps(response, separators=(",", ":")) + "\n")
        stdout.flush()
        if response.get("ok") and response.get("result", {}).get("shutdown"):
            break


def _first_attr(obj: Any, *names: str) -> Any:
    for name in names:
        if isinstance(obj, dict) and name in obj:
            return obj[name]
        if hasattr(obj, name):
            return getattr(obj, name)
    return None


def _int_attr(obj: Any, default: int, *names: str) -> int:
    value = _first_attr(obj, *names)
    if value is None:
        return default
    try:
        return int(value)
    except (TypeError, ValueError):
        return default


def _extract_cards(queued: Any) -> list[Any]:
    if queued is None:
        return []
    if isinstance(queued, list):
        items = queued
    else:
        items = _first_attr(queued, "cards", "queued_cards") or []
    cards = []
    for item in items:
        card = _first_attr(item, "card")
        cards.append(card if card is not None else item)
    return cards


def _call_with_supported_kwargs(func: Any, **kwargs: Any) -> Any:
    try:
        return func(**kwargs)
    except TypeError:
        try:
            return func(kwargs.get("fetch_limit"))
        except TypeError:
            return func()


def _strip_direction_markers(text: str) -> str:
    return text.replace("\u2068", "").replace("\u2069", "")


def _unique(values: Iterable[str]) -> list[str]:
    seen = set()
    out = []
    for value in values:
        if value not in seen:
            seen.add(value)
            out.append(value)
    return out


def _deck_sort_key(deck: dict[str, Any]) -> list[str]:
    return [part.lower() for part in str(deck["name"]).split("::")]


def main() -> None:
    serve()


if __name__ == "__main__":
    main()
