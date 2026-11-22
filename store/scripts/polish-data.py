"""Prepares a copy of the sample data folder for the store screenshots.

The sample data comes from the bundled mock server, whose answers are placeholder text
("Here is the answer to ..."). For pictures that people will read at full size, this script
replaces the answers of the two "Quick checks" runs with prepared ones, marks every endpoint as
reachable with an address like the ones people use, and clears the chart history of the long gsm8k
run so that the capture shows only the session it records. It never touches the model names: they stay the mock's, so the runs can still
be resumed against the mock.

Run it on a copy of the data folder, with the server stopped:

    python3 store/scripts/polish-data.py /path/to/data-copy

The prepared answers are sample text written for the screenshots, not output of a real model.
"""

import json
import sqlite3
import sys
from pathlib import Path

# The six prompts of the "Quick checks" prompt set, by their position (prompts.idx). For each
# prompt, four candidates: run 1 sample 1, run 1 sample 2, run 2 sample 1, run 2 sample 2. Each is
# (thinking trace, response). The preference review shows prompt 2 (mutex and semaphore), so its
# four candidates differ the most.
ANSWERS = {
    0: [
        (
            "The question asks for the capital and the reason. The capital is Canberra, not Sydney, "
            "which is a common mistake. When the colonies federated in 1901, Sydney and Melbourne were "
            "rivals and neither would accept the other. Section 125 of the Constitution settled it: the "
            "capital had to be in New South Wales, at least 100 miles from Sydney, and Parliament would "
            "meet in Melbourne until it was built. The site was chosen in 1908 and Parliament moved in "
            "1927. Give the answer first, then the compromise.",
            "The capital of Australia is **Canberra**.\n\nIt was a compromise. When the colonies "
            "federated in 1901, Sydney and Melbourne were rivals, and neither would accept the other as "
            "the capital. Section 125 of the Constitution required the capital to be in New South Wales "
            "but at least 100 miles (about 160 km) from Sydney, with Parliament meeting in Melbourne in "
            "the meantime. The Canberra site was chosen in 1908, and Parliament moved there in 1927.",
        ),
        (
            "Capital of Australia: Canberra. Why not Sydney or Melbourne? Federation in 1901. Sydney was "
            "the oldest and largest city; Melbourne was the richest at the time. The rivalry meant a new "
            "city. Constitution, section 125: the seat of government in New South Wales, not less than "
            "100 miles from Sydney. Melbourne would host Parliament until then. Sites considered: "
            "Dalgety, Tumut, Albury, Bombala. Dalgety was picked in 1904, then replaced by the "
            "Yass-Canberra area in 1908. Named Canberra in 1913. Parliament opened there in 1927. A "
            "short table of dates would make the sequence clear.",
            "**Canberra.** Neither Sydney nor Melbourne would accept the other as the national capital, "
            "so the Constitution of 1901 called for a new city in New South Wales, at least 100 miles "
            "from Sydney. Melbourne hosted Parliament until the new capital was ready.\n\n"
            "| Year | Step |\n| --- | --- |\n| 1901 | Federation; Parliament meets in Melbourne |\n"
            "| 1908 | The Yass-Canberra site is chosen |\n| 1913 | The city is named Canberra |\n"
            "| 1927 | Parliament opens in Canberra |",
        ),
        (
            "Canberra. The reason is the Sydney and Melbourne rivalry at Federation. Keep it to two or "
            "three sentences.",
            "Canberra is the capital. It was built as a compromise between Sydney and Melbourne, the two "
            "largest cities, which both wanted the title when Australia federated in 1901.",
        ),
        (
            "The user wants the capital and the history. Canberra. Sydney and Melbourne competed; the "
            "Constitution required a site in New South Wales at least 100 miles from Sydney. Mention "
            "the design competition: Walter Burley Griffin and Marion Mahony Griffin won it in 1912.",
            "Australia's capital is Canberra. Because Sydney and Melbourne each refused to let the "
            "other become the capital, the Constitution placed it in New South Wales but at least "
            "100 miles from Sydney. The city was planned from scratch, to a design by Walter Burley "
            "Griffin and Marion Mahony Griffin, and Parliament moved there from Melbourne in 1927.",
        ),
    ],
    1: [
        (
            "Naive recursion is exponential and a loop is O(n). O(log n) needs matrix powers or fast "
            "doubling. Fast doubling is shorter: F(2m) = F(m)(2F(m+1) - F(m)) and "
            "F(2m+1) = F(m)^2 + F(m+1)^2. Recurse on n // 2 and pick the pair by the low bit. The "
            "depth is log2(n), so recursion is fine. Check: fib(10) should be 55.",
            "Fast doubling halves `n` at every step:\n\n```python\ndef fib(n: int) -> int:\n"
            "    \"\"\"Return the n-th Fibonacci number, with fib(0) = 0, in O(log n) steps.\"\"\"\n"
            "    if n < 0:\n        raise ValueError(\"n must be non-negative\")\n\n"
            "    def pair(k: int) -> tuple[int, int]:\n        if k == 0:\n            return 0, 1\n"
            "        a, b = pair(k >> 1)      # F(m), F(m+1) with m = k // 2\n"
            "        c = a * (2 * b - a)      # F(2m)\n        d = a * a + b * b        # F(2m+1)\n"
            "        return (d, c + d) if k & 1 else (c, d)\n\n    return pair(n)[0]\n```\n\n"
            "`fib(10)` returns 55.",
        ),
        (
            "Matrix form: [[1,1],[1,0]]^n = [[F(n+1),F(n)],[F(n),F(n-1)]]. Exponentiation by squaring "
            "gives O(log n) multiplications of 2x2 matrices.",
            "Raise the matrix `[[1, 1], [1, 0]]` to the n-th power by repeated squaring:\n\n"
            "```python\ndef fib(n: int) -> int:\n    def mul(x, y):\n"
            "        return [[x[0][0] * y[0][0] + x[0][1] * y[1][0], x[0][0] * y[0][1] + x[0][1] * y[1][1]],\n"
            "                [x[1][0] * y[0][0] + x[1][1] * y[1][0], x[1][0] * y[0][1] + x[1][1] * y[1][1]]]\n\n"
            "    result, base = [[1, 0], [0, 1]], [[1, 1], [1, 0]]\n    while n:\n        if n & 1:\n"
            "            result = mul(result, base)\n        base = mul(base, base)\n        n >>= 1\n"
            "    return result[0][1]\n```",
        ),
        (
            "O(log n): fast doubling. Write it iteratively over the bits of n, from the top.",
            "```python\ndef fib(n: int) -> int:\n    a, b = 0, 1                      # F(0), F(1)\n"
            "    for bit in bin(n)[2:]:\n        c = a * (2 * b - a)\n        d = a * a + b * b\n"
            "        a, b = (d, c + d) if bit == \"1\" else (c, d)\n    return a\n```",
        ),
        (
            "Binet's formula is O(1) but floats lose precision above about n = 70. Use integer fast "
            "doubling instead.",
            "Use the fast-doubling identities F(2m) = F(m)(2F(m+1) − F(m)) and "
            "F(2m+1) = F(m)² + F(m+1)², which need O(log n) steps and stay exact with Python integers. "
            "Avoid Binet's formula: floating point is wrong from about n = 71.",
        ),
    ],
    2: [
        (
            "Two sentences only. The key differences: ownership and count. A mutex is owned by the "
            "thread that locks it, and only that thread unlocks it; it guards a critical section. A "
            "semaphore is a counter of permits, can be signaled by any thread, and limits how many "
            "threads use a resource at once or signals between threads.",
            "A mutex is a lock with an owner: one thread acquires it, and only that thread may release "
            "it, so it keeps a critical section to one thread at a time. A semaphore is a counter that "
            "lets up to N threads proceed and can be signaled by any thread, which suits limiting "
            "access to a pool of resources or signaling between threads.",
        ),
        (
            "Explain mutex and semaphore. Start with mutual exclusion, then counting semaphores, then "
            "binary semaphores and how they differ from a mutex, then priority inversion, which only a "
            "mutex with an owner can address through priority inheritance. Then when to use each. "
            "Examples: a database connection pool of 10 is a counting semaphore with 10 permits; a "
            "shared map updated by several threads needs a mutex.",
            "A mutex (mutual exclusion lock) makes sure only one thread at a time runs a critical "
            "section, and it is released by the thread that acquired it. A semaphore keeps a count of "
            "available permits: `wait` takes one and blocks at zero, `signal` returns one. A binary "
            "semaphore looks like a mutex, but it has no owner, so any thread can release it and the "
            "system cannot apply priority inheritance. Use a mutex to protect shared data and a "
            "counting semaphore to cap concurrent use of a resource, such as a pool of 10 database "
            "connections.",
        ),
        (
            "Short form: ownership and counting.",
            "Mutex: a lock owned by the thread that holds it, used for mutual exclusion. Semaphore: a "
            "count of permits with no owner, used to limit concurrency or to signal between threads.",
        ),
        (
            "Mutex is for one thread, semaphore is for several. Also mention where they are used.",
            "A mutex can be held by one thread at a time, while a semaphore can be held by several "
            "threads at once. Mutexes are used in user programs, and semaphores are used only inside "
            "the operating system kernel.",
        ),
    ],
    3: [
        (
            "13:05 minus 9:40. From 9:40 to 12:40 is 3 hours. From 12:40 to 13:05 is 25 minutes. "
            "Total 3 hours 25 minutes, which is 205 minutes.",
            "From 9:40 to 12:40 is 3 hours, and from 12:40 to 13:05 is 25 minutes. The trip takes "
            "**3 hours 25 minutes** (205 minutes).",
        ),
        (
            "Borrow an hour: 13:05 is 12:65. 12:65 - 9:40 = 3:25.",
            "13:05 − 9:40 = **3 h 25 min**.",
        ),
        (
            "Count up: 9:40 to 10:00 is 20 minutes, 10:00 to 13:00 is 3 hours, 13:00 to 13:05 is "
            "5 minutes. 3 hours 25 minutes.",
            "20 minutes to 10:00, 3 hours to 13:00, and 5 more minutes: the trip is 3 hours 25 minutes.",
        ),
        (
            "Convert to minutes: 9:40 is 580, 13:05 is 785. 785 - 580 = 205 minutes = 3 h 25 min.",
            "In minutes after midnight, 9:40 is 580 and 13:05 is 785, so the trip is 205 minutes, or "
            "3 hours 25 minutes.",
        ),
    ],
    4: [
        (
            "Exactly three sentences: the ghost and the murder; the feigned madness, the play, "
            "Polonius and Ophelia; the duel and the deaths, with Fortinbras at the end.",
            "Prince Hamlet learns from his father's ghost that his uncle Claudius murdered the king, "
            "married Queen Gertrude and took the throne. Hamlet feigns madness and stages a play to "
            "test Claudius's guilt, but he kills Polonius by mistake, which drives Ophelia to madness "
            "and death and turns her brother Laertes against him. In a rigged duel Gertrude, Laertes, "
            "Claudius and Hamlet all die, and Fortinbras of Norway takes the crown.",
        ),
        (
            "Three sentences. Keep the revenge plot, the delay and the ending.",
            "Told by his father's ghost that Claudius killed him, Hamlet vows revenge but hesitates, "
            "pretending to be mad while he looks for proof. His delay costs the lives of Polonius and "
            "Ophelia. The play ends in a poisoned duel that kills Hamlet, Laertes, Claudius and "
            "Gertrude.",
        ),
        (
            "Three sentences about Hamlet.",
            "Hamlet, Prince of Denmark, seeks revenge on his uncle Claudius for murdering his father. "
            "His hesitation and feigned madness lead to the deaths of Polonius and Ophelia. A final "
            "duel kills the royal family, and Fortinbras takes the throne.",
        ),
        (
            "Summarize Hamlet. The user said three sentences.",
            "Hamlet's father is murdered by his brother Claudius, who marries the queen. Hamlet "
            "pretends to be mad, stages a play that proves Claudius's guilt, and accidentally kills "
            "Polonius. Ophelia drowns, Laertes returns for revenge, and the duel that follows kills "
            "almost everyone. Fortinbras arrives to find the court dead.",
        ),
    ],
    5: [
        (
            "Classic proof by contradiction. Assume sqrt(2) = p/q in lowest terms. Then p^2 = 2q^2, so "
            "p^2 is even, so p is even. Write p = 2k: 4k^2 = 2q^2, so q^2 = 2k^2 and q is even. Both "
            "even contradicts lowest terms.",
            "Suppose $\\sqrt{2} = p/q$ with integers $p, q$ that have no common factor. Squaring gives "
            "$p^2 = 2q^2$, so $p^2$ is even and therefore $p$ is even; write $p = 2k$. Then "
            "$4k^2 = 2q^2$, so $q^2 = 2k^2$ and $q$ is even as well. Both are even, which contradicts "
            "the assumption that they have no common factor, so $\\sqrt{2}$ is irrational. $\\blacksquare$",
        ),
        (
            "Use the fundamental theorem of arithmetic: in p^2 = 2q^2 the exponent of 2 is even on the "
            "left and odd on the right.",
            "If $\\sqrt{2} = p/q$, then $p^2 = 2q^2$. The prime 2 appears an even number of times in "
            "$p^2$ and an odd number of times in $2q^2$, which is impossible by unique factorization. "
            "So no such $p$ and $q$ exist.",
        ),
        (
            "Contradiction with lowest terms.",
            "Assume $\\sqrt{2} = a/b$ in lowest terms. Then $a^2 = 2b^2$, so $a$ is even, so $b$ is "
            "even, a contradiction.",
        ),
        (
            "Proof by infinite descent: from one representation build a smaller one.",
            "If $\\sqrt{2} = p/q$ with $q$ the smallest possible positive denominator, then "
            "$\\sqrt{2} = (2q - p)/(p - q)$, and $0 < p - q < q$, a smaller denominator. This "
            "contradicts the choice of $q$.",
        ),
    ],
}


# (names it may have now, new name, base URL).
ENDPOINTS = [
    (("Mock on this Mac", "LM Studio on this Mac"), "LM Studio on this Mac", "http://localhost:1234/v1"),
    (("DGX Spark vLLM",), "DGX Spark vLLM", "http://localhost:8000/v1"),
    (("Office llama.cpp box",), "Office llama.cpp box", "http://192.168.1.20:8080/v1"),
]


def tokens(text: str) -> int:
    """A rough token count for the prepared text: about 1.35 tokens per word."""
    return max(1, round(len(text.split()) * 1.35))


def polish(db_path: Path) -> None:
    db = sqlite3.connect(db_path)
    cur = db.cursor()
    sets = cur.execute("SELECT id FROM prompt_sets WHERE name = 'Quick checks'").fetchall()
    if not sets:
        sys.exit("No prompt set named 'Quick checks' in this data folder.")
    runs = [r for (r,) in cur.execute(
        "SELECT id FROM runs WHERE prompt_set_id = ? AND status = 'completed' AND done > 0 "
        "AND truncated = 0 ORDER BY id", (sets[0][0],))][:2]
    changed = 0
    for slot, run_id in enumerate(runs):
        has_reasoning_tokens = cur.execute(
            "SELECT COUNT(*) FROM samples WHERE run_id = ? AND reasoning_tokens IS NOT NULL", (run_id,)
        ).fetchone()[0] > 0
        for sid, idx, sample_idx in cur.execute(
            "SELECT id, prompt_idx, sample_idx FROM samples WHERE run_id = ? AND status = 'done'", (run_id,)
        ).fetchall():
            options = ANSWERS.get(idx)
            if not options or sample_idx > 1:
                continue
            thinking, response = options[slot * 2 + sample_idx]
            cur.execute(
                "UPDATE samples SET reasoning = ?, response = ?, completion_tokens = ?, reasoning_tokens = ? WHERE id = ?",
                (thinking, response, tokens(thinking) + tokens(response),
                 tokens(thinking) if has_reasoning_tokens else None, sid),
            )
            changed += 1
        total = cur.execute(
            "SELECT COALESCE(SUM(completion_tokens), 0), COALESCE(SUM(reasoning_tokens), 0) FROM samples WHERE run_id = ?",
            (run_id,)).fetchone()
        cur.execute("UPDATE runs SET completion_tokens = ?, reasoning_tokens = ? WHERE id = ?", (*total, run_id))

    # Every endpoint reachable, with the models the mock serves, so no screen shows a stale error;
    # addresses like the ones people use. The DGX Spark endpoint carries the live run, so its address
    # must reach a mock: start one on port 8000 (store/README.md).
    models = cur.execute("SELECT models FROM endpoints WHERE last_ok = 1 LIMIT 1").fetchone()
    cur.execute(
        "UPDATE endpoints SET last_ok = 1, last_error = NULL, models = ? WHERE last_ok IS NOT 1",
        (models[0] if models else json.dumps(["mock-reasoning"]),),
    )
    for names, new_name, url in ENDPOINTS:
        cur.execute(
            f"UPDATE endpoints SET name = ?, base_url = ? WHERE name IN ({','.join('?' * len(names))})",
            (new_name, url, *names),
        )
        cur.execute(
            f"UPDATE runs SET endpoint_name = ? WHERE endpoint_name IN ({','.join('?' * len(names))})",
            (new_name, *names),
        )

    # The long gsm8k run: drop its chart history, so the capture shows one session.
    long_run = cur.execute("SELECT id FROM runs ORDER BY total DESC, done DESC LIMIT 1").fetchone()
    if long_run:
        cur.execute("DELETE FROM run_metrics WHERE run_id = ?", long_run)
    db.commit()
    db.close()
    print(f"prepared {changed} answers in runs {runs}; cleared the chart of run {long_run[0] if long_run else '-'}")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    path = Path(sys.argv[1]) / "stellarator.db"
    if not path.exists():
        sys.exit(f"{path} does not exist")
    polish(path)
