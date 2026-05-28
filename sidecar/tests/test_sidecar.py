import io
import json

from anki_tui_sidecar import AnkiEngine, RpcServer, _deck_sort_key, serve


class FakeCard:
    def __init__(self, card_id, question="front [sound:q.mp3]", answer="back [sound:a.mp3]"):
        self.id = card_id
        self._question = question
        self._answer = answer
        self.timer_started = False

    def question(self):
        return self._question

    def answer(self):
        return self._answer

    def start_timer(self):
        self.timer_started = True

    def question_av_tags(self):
        return []

    def answer_av_tags(self):
        return [FakeAvTag("answer-av.mp3")]


class FakeAvTag:
    def __init__(self, filename):
        self.filename = filename


class FakeSched:
    def __init__(self):
        self.cards = [FakeCard(1), FakeCard(2)]
        self.answered = []

    def deck_due_tree(self):
        return {
            "deck_id": 10,
            "name": "Root",
            "new_count": 1,
            "learn_count": 2,
            "review_count": 3,
            "children": [
                {
                    "deck_id": 11,
                    "name": "Child",
                    "new_count": 4,
                    "learn_count": 5,
                    "review_count": 6,
                    "children": [],
                }
            ],
        }

    def get_queued_cards(self, fetch_limit=100, intraday_learning_only=False):
        return self.cards[:fetch_limit]

    def answerButtons(self, card):
        return 4

    def nextIvlStr(self, card, rating):
        return {1: "1m", 2: "6m", 3: "1d", 4: "4d"}[rating]

    def answerCard(self, card, rating):
        self.answered.append((card.id, rating, card.timer_started))
        self.cards = [c for c in self.cards if c.id != card.id]


class FakeDecks:
    def __init__(self):
        self.selected = None

    def select(self, deck_id):
        self.selected = deck_id

    def name(self, deck_id):
        return "Root" if deck_id == 10 else "Root::Child"

    def all_names_and_ids(self):
        return [{"id": 10, "name": "Root"}, {"id": 11, "name": "Root::Child"}]


class FakeCollection:
    def __init__(self):
        self.sched = FakeSched()
        self.decks = FakeDecks()
        self.saved = False

    def save(self):
        self.saved = True


class FakeEngine(AnkiEngine):
    def __init__(self):
        super().__init__()
        self.col = FakeCollection()

    def _get_card_by_id(self, card_id):
        for card in self.col.sched.cards:
            if card.id == card_id:
                card.start_timer()
                return card
        card = FakeCard(card_id)
        card.start_timer()
        return card


def test_list_decks_flattens_due_tree():
    server = RpcServer(FakeEngine())
    response = server.handle({"id": 1, "method": "list_decks", "params": {}})

    assert response["ok"] is True
    assert response["result"]["decks"] == [
        {"id": 10, "name": "Root", "new_count": 1, "learn_count": 2, "review_count": 3},
        {"id": 11, "name": "Root::Child", "new_count": 4, "learn_count": 5, "review_count": 6},
    ]


def test_synthetic_root_deck_is_not_listed():
    engine = FakeEngine()
    rows = []
    engine._flatten_due_tree(
        {
            "deck_id": 0,
            "name": "",
            "new_count": 99,
            "learn_count": 99,
            "review_count": 99,
            "children": [
                {
                    "deck_id": 10,
                    "name": "Root",
                    "new_count": 1,
                    "learn_count": 2,
                    "review_count": 3,
                    "children": [],
                }
            ],
        },
        rows,
    )

    assert rows == [
        {"id": 10, "name": "Root", "new_count": 1, "learn_count": 2, "review_count": 3}
    ]


def test_start_review_returns_rendered_card_payload_and_buttons():
    server = RpcServer(FakeEngine())
    response = server.handle(
        {"id": 1, "method": "start_review", "params": {"deck_id": 10, "dry_run": False}}
    )

    card = response["result"]["card"]
    assert card["id"] == 1
    assert card["question_html"] == "front [sound:q.mp3]"
    assert card["answer_html"] == "back [sound:a.mp3]"
    assert card["front_audio"] == ["q.mp3"]
    assert card["back_audio"] == ["a.mp3", "answer-av.mp3"]
    assert [b["interval"] for b in card["buttons"]] == ["1m", "6m", "1d", "4d"]


def test_dry_run_answer_skips_without_writing():
    engine = FakeEngine()
    server = RpcServer(engine)
    server.handle({"id": 1, "method": "start_review", "params": {"deck_id": 10, "dry_run": True}})
    response = server.handle({"id": 2, "method": "answer_card", "params": {"card_id": 1, "rating": 3}})

    assert engine.col.sched.answered == []
    assert response["result"]["card"]["id"] == 2


def test_live_answer_calls_scheduler():
    engine = FakeEngine()
    server = RpcServer(engine)
    server.handle({"id": 1, "method": "start_review", "params": {"deck_id": 10, "dry_run": False}})
    response = server.handle({"id": 2, "method": "answer_card", "params": {"card_id": 1, "rating": 4}})

    assert engine.col.sched.answered == [(1, 4, True)]
    assert response["result"]["card"]["id"] == 2


def test_protocol_reports_invalid_json():
    output = io.StringIO()
    serve(io.StringIO("{bad\n"), output, FakeEngine())

    response = json.loads(output.getvalue())
    assert response["ok"] is False
    assert response["error"]["code"] == "invalid_json"


def test_deck_sort_keeps_hierarchy_before_colon_named_sibling():
    decks = [
        {"name": "Mandarin: Vocabulary"},
        {"name": "Mandarin::Beijing"},
        {"name": "Mandarin"},
        {"name": "English"},
    ]

    assert [d["name"] for d in sorted(decks, key=_deck_sort_key)] == [
        "English",
        "Mandarin",
        "Mandarin::Beijing",
        "Mandarin: Vocabulary",
    ]
