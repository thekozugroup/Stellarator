from __future__ import annotations

import os
import tempfile
import time
from pathlib import Path

os.environ.setdefault("STELLARATOR_DATA_DIR", tempfile.mkdtemp(prefix="stellarator-imp-"))

import pyarrow as pa
import pyarrow.parquet as pq
import pytest

from stellarator.schemas import ImportMapping
from stellarator.services import importers as imp

FIX = Path(__file__).parent / "fixtures" / "imports"


def prompts_of(path: Path, mapping: ImportMapping | None = None, fmt: str = "auto"):
    src = imp.open_path(path, fmt, path.name)
    stream = imp.open_stream(src, mapping or ImportMapping())
    return [p for p in stream], stream


def users(items):
    return [m[-1]["content"] for m, _ in items]


# ------------------------------------------------------------------ text splitting


def test_auto_separator():
    pieces, mode = imp.split_text("one\ntwo\n---\nthree\n***\nfour")
    assert mode == "separator"
    assert pieces == ["one\ntwo", "three", "four"]


def test_auto_heading_markdown_and_front_matter():
    items, stream = prompts_of(FIX / "doc.md")
    assert stream.split_mode == "heading"
    got = users(items)
    # "First" has body, "Sub" has body, container headings with no body are skipped
    assert got == ["First\n\nBody one.", "Sub\n\nBody sub.", "Second\n\nBody two."]
    assert not any("title:" in g for g in got)


def test_heading_ignores_code_fences():
    text = "# A\n```\n# not a heading\n```\n# B\nx"
    pieces, _ = imp.split_text(text, "heading", True)
    assert len(pieces) == 2 and "# not a heading" in pieces[0]


def test_auto_list_item_with_continuation():
    text = "- first item\n  continues here\n- second\n1. third"
    pieces, mode = imp.split_text(text)
    assert mode == "list_item"
    assert pieces == ["first item\ncontinues here", "second", "third"]


def test_auto_blank_line():
    pieces, mode = imp.split_text("Para one\nstill one.\n\nPara two.\n\n\nPara three.")
    assert mode == "blank_line"
    assert pieces == ["Para one\nstill one.", "Para two.", "Para three."]


def test_auto_line_and_whole():
    assert imp.split_text("a\nb\nc") == (["a", "b", "c"], "line")
    pieces, mode = imp.split_text("just one")
    assert mode == "whole" or mode == "line"
    assert pieces == ["just one"]
    long = "x" * 600 + "\n" + "y" * 600
    assert imp.split_text(long)[1] == "whole"


def test_explicit_modes():
    assert imp.split_text("a\n\nb\nc", "line")[0] == ["a", "b", "c"]
    assert imp.split_text("a\n\nb", "whole")[0] == ["a\n\nb"]
    assert imp.split_text("  a  \n\n\n  \n b ", "blank_line")[0] == ["a", "b"]
    assert imp.split_text("- x\n- y", "list_item")[0] == ["x", "y"]
    assert imp.split_text("# T\nbody\n# U\nmore", "heading", False)[0] == ["T\n\nbody", "U\n\nmore"]
    assert imp.split_text("a\n___\nb", "separator")[0] == ["a", "b"]


def test_text_file_dedupe_offset_limit(tmp_path):
    f = tmp_path / "p.txt"
    f.write_text("a\nb\na\nc\nd\n")
    items, stream = prompts_of(f, ImportMapping(split_mode="line"))
    assert users(items) == ["a", "b", "c", "d"]
    assert stream.stats.duplicates == 1
    items, _ = prompts_of(f, ImportMapping(split_mode="line", dedupe=False))
    assert users(items) == ["a", "b", "a", "c", "d"]
    items, _ = prompts_of(f, ImportMapping(split_mode="line", offset=1, limit=2, dedupe=False))
    assert users(items) == ["b", "a"]


# ------------------------------------------------------------------ tables


def test_csv_bom_quoted_newlines():
    items, stream = prompts_of(FIX / "bom_quoted.csv")
    assert stream.prompt_field == "prompt"
    assert users(items) == ["Line one\nline two", 'Say "hi"']
    meta = items[0][1]
    assert meta == {"id": "1", "answer": "a"}


def test_csv_schema_kinds():
    t = imp.open_path(FIX / "bom_quoted.csv")
    kinds = {c.name: c.kind for c in t.schema.columns}
    assert kinds == {"id": "number", "prompt": "text", "answer": "text"}


def test_csv_latin1_warning(tmp_path):
    f = tmp_path / "l.csv"
    f.write_bytes("prompt\ncaf\xe9\n".encode("latin-1"))
    items, stream = prompts_of(f)
    assert users(items) == ["café"]
    assert any("Latin-1" in w for w in stream.warnings)


def test_tsv():
    items, stream = prompts_of(FIX / "qa.tsv")
    assert stream.prompt_field == "question"
    assert users(items) == ["What is 2+2?", "Name a color"]


def test_json_alpaca_combines_input():
    items, stream = prompts_of(FIX / "alpaca.json")
    assert stream.prompt_field == "instruction"
    assert users(items) == ["Translate\n\nhello", "Say hi"]
    assert "input" not in items[0][1] and items[0][1]["output"] == "hola"


def test_json_shapes(tmp_path):
    (tmp_path / "a.json").write_text('["one", "two", "one"]')
    items, _ = prompts_of(tmp_path / "a.json")
    assert users(items) == ["one", "two"]
    (tmp_path / "b.json").write_text('{"data": [{"question": "q1"}, {"question": "q2", "extra": 3}]}')
    items, _ = prompts_of(tmp_path / "b.json")
    assert users(items) == ["q1", "q2"] and items[1][1] == {"extra": 3}
    (tmp_path / "c.json").write_text('{"prompts": ["x"], "name": "n"}')
    assert users(prompts_of(tmp_path / "c.json")[0]) == ["x"]
    (tmp_path / "d.json").write_text('{"a": 1}')
    with pytest.raises(imp.ImportProblem):
        prompts_of(tmp_path / "d.json")


def test_json_that_is_really_jsonl(tmp_path):
    f = tmp_path / "x.json"
    f.write_text('{"prompt": "a"}\n{"prompt": "b"}\n')
    assert users(prompts_of(f)[0]) == ["a", "b"]


def test_jsonl_sharegpt_bad_lines_and_trailing_assistant():
    items, stream = prompts_of(FIX / "share.jsonl")
    assert stream.prompt_field == "conversations"
    assert items[0][0] == [
        {"role": "system", "content": "Be brief."},
        {"role": "user", "content": "Hello"},
    ]
    # trailing assistant dropped, earlier turns kept
    assert items[1][0] == [
        {"role": "user", "content": "Q2"},
        {"role": "assistant", "content": "A2"},
        {"role": "user", "content": "Q3"},
    ]
    assert any("1 line was not valid JSON" in w and "line 2" in w for w in stream.warnings)


def test_openai_messages_and_parts():
    msgs = imp.normalize_messages(
        [
            {"role": "system", "content": "S"},
            {"role": "user", "content": [{"type": "text", "text": "a"}, {"type": "image_url", "image_url": {}}, {"type": "text", "text": "b"}]},
            {"role": "assistant", "content": "x"},
        ]
    )
    assert msgs == [{"role": "system", "content": "S"}, {"role": "user", "content": "a\nb"}]
    assert imp.normalize_messages([{"role": "assistant", "content": "x"}]) is None


def test_jsonl_strings(tmp_path):
    f = tmp_path / "s.jsonl"
    f.write_text('"hello"\n"world"\n')
    assert users(prompts_of(f)[0]) == ["hello", "world"]


def test_system_field_auto_and_empty_prompts(tmp_path):
    f = tmp_path / "s.csv"
    f.write_text("system,prompt,score\nBe nice,Hi,3\n,,4\nBe nice,Hi,5\n,Yo,x\n")
    items, stream = prompts_of(f)
    assert stream.system_field == "system"
    assert items[0][0] == [{"role": "system", "content": "Be nice"}, {"role": "user", "content": "Hi"}]
    assert items[1][0] == [{"role": "user", "content": "Yo"}]
    assert stream.stats.empty == 1 and stream.stats.duplicates == 1
    assert any("empty prompt" in w for w in stream.warnings)


def test_autodetect_longest_text_and_names(tmp_path):
    f = tmp_path / "a.csv"
    f.write_text("id,title,body\n1,short,a much longer body of text here\n2,tiny,another longer body of text\n")
    assert prompts_of(f)[1].prompt_field == "body"
    g = tmp_path / "b.csv"
    g.write_text("Question,notes\nWhy?,zzzzzzzzzzzzzzzz\n")
    assert prompts_of(g)[1].prompt_field == "Question"
    with pytest.raises(imp.ImportProblem):
        prompts_of(g, ImportMapping(prompt_field="nope"))


def test_meta_scalars_truncated_and_nested_skipped(tmp_path):
    f = tmp_path / "m.jsonl"
    f.write_text('{"prompt": "p", "long": "%s", "nested": {"a": 1}, "n": 2, "lst": [1]}\n' % ("x" * 9000))
    items, _ = prompts_of(f)
    meta = items[0][1]
    assert len(meta["long"]) == 4000 and meta["n"] == 2 and "nested" not in meta and "lst" not in meta


def test_parquet_messages_and_window(tmp_path):
    f = tmp_path / "d.parquet"
    table = pa.table(
        {
            "messages": [[{"role": "user", "content": f"q{i}"}, {"role": "assistant", "content": "a"}] for i in range(30)],
            "score": list(range(30)),
            "blob": [b"x"] * 30,
        }
    )
    pq.write_table(table, f, row_group_size=7)
    items, stream = prompts_of(f, ImportMapping(offset=10, limit=12))
    assert users(items) == [f"q{i}" for i in range(10, 22)]
    assert items[0][1] == {"score": 10}
    t = imp.open_path(f)
    assert t.total_rows == 30
    assert "blob" not in t.names()
    pv = imp.preview_source(t, ImportMapping())
    assert pv.prompt_field == "messages" and pv.total_rows == 30 and pv.estimated_prompts == 30
    assert len(pv.prompts) == 20


def test_parquet_text_column(tmp_path):
    f = tmp_path / "t.parquet"
    pq.write_table(pa.table({"question": ["a", "b", "c"], "answer": ["1", "2", "3"]}), f)
    items, _ = prompts_of(f)
    assert users(items) == ["a", "b", "c"]
    assert imp.detect_format(f) == "parquet"


def test_format_sniffing(tmp_path):
    cases = {
        "a.dat": ('[1, 2]', "json"),
        "b.dat": ('{"a": 1}\n{"a": 2}\n', "jsonl"),
        "c.dat": ("a,b\n1,2\n3,4\n", "csv"),
        "d.dat": ("a\tb\n1\t2\n", "tsv"),
        "e.dat": ("hello there\n", "text"),
        "f.NDJSON": ("{}", "jsonl"),
        "g.Markdown": ("#", "markdown"),
    }
    for name, (content, want) in cases.items():
        p = tmp_path / name
        p.write_text(content)
        assert imp.detect_format(p, name) == want, name


def test_preview_text_and_totals():
    doc = imp.paste_doc("a\n\nb\n\nc")
    pv = imp.preview_source(doc, ImportMapping())
    assert pv.split_mode == "blank_line" and pv.estimated_prompts == 3 and len(pv.prompts) == 3


def test_preview_csv_counts_and_warnings(tmp_path):
    f = tmp_path / "w.csv"
    f.write_text("prompt\na\na\n\nb\n")
    pv = imp.preview_source(imp.open_path(f), ImportMapping())
    assert pv.estimated_prompts == 2 and pv.total_rows == 3
    assert any("duplicate" in w for w in pv.warnings)


def test_cleanup_old_uploads(tmp_path):
    old = tmp_path / "old"
    new = tmp_path / "new"
    old.mkdir(), new.mkdir()
    (old / "f").write_text("x")
    t = time.time() - 48 * 3600
    os.utime(old, (t, t))
    assert imp.cleanup_old_uploads(tmp_path) == 1
    assert not old.exists() and new.exists()


def test_sanitize_filename():
    assert imp.sanitize_filename("../../etc/pass wd.csv") == "pass wd.csv"
    assert imp.sanitize_filename("") == "upload"
    assert imp.sanitize_filename("a/b\\c<>.txt") == "c_.txt"


def test_100k_rows_csv_speed(tmp_path):
    f = tmp_path / "big.csv"
    with open(f, "w") as out:
        out.write("prompt,idx\n")
        for i in range(100_000):
            out.write(f"Question number {i} about something\",{i}\n".replace('"', ""))
    t0 = time.time()
    src = imp.open_path(f)
    stream = imp.open_stream(src, ImportMapping())
    n = sum(len(b) for b in stream.iter_batches())
    assert n == 100_000
    assert time.time() - t0 < 10
