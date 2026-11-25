#!/usr/bin/env python3
"""Sum Claude Code token usage per model family over a period (one pasteable line)."""
import argparse
import json
import pathlib
import time
from datetime import datetime, timezone

DEFAULT_DIR = "~/.claude/projects/-home-arthur-dev-outils-jeux-wow-scout-rs/"
# API prices in $ per million tokens, read in 2026-11: (input, output, cache read).
# Cache write: 5 min cache = 1.25 x input, 1 h cache = 2 x input (breakdown from
# usage.cache_creation; without it, all writes count as 5 min). Priced by family: an older model of the same family
# gets the family price, which is only an approximation.
PRICES = {
    "opus": (4, 20, 0.20),      # opus 5.5
    "sonnet": (2, 10, 0.10),    # sonnet 5.5
    "haiku": (0.10, 0.50, 0.01),  # haiku 5.5
    "fable": (8, 40, 0.20),     # fable 5.2
}
SHOWN = ["opus", "sonnet", "haiku"]  # always displayed
DEFAULT_USAGE_LOG = "~/.claude/usage-log.jsonl"
WINDOWS = [("five_hour", "5 h"), ("seven_day", "7 j")]
FIELDS = ["input_tokens", "output_tokens", "cache_read_input_tokens", "cache_creation_input_tokens"]


def parse_time(text):
    """ISO string (Z, offset or bare date) -> aware UTC datetime."""
    dt = datetime.fromisoformat(text.replace("Z", "+00:00"))
    return dt.replace(tzinfo=timezone.utc) if dt.tzinfo is None else dt.astimezone(timezone.utc)


def family(model):
    for name in PRICES:
        if name in model:
            return name
    return "autre"


def read_entries(folder):
    """Yield (key, time, model, usage) for every response line; invalid lines are skipped."""
    for path in pathlib.Path(folder).expanduser().glob("**/*.jsonl"):
        with open(path, encoding="utf-8", errors="replace") as fh:
            for line in fh:
                try:
                    obj = json.loads(line)
                    msg = obj["message"]
                    usage, model = msg["usage"], msg["model"]
                    when = parse_time(obj["timestamp"])
                except (ValueError, KeyError, TypeError):
                    continue
                key = msg.get("id") or obj.get("requestId")
                if key and isinstance(usage, dict) and not model.startswith("<"):
                    yield key, when, model, usage


def aggregate(entries, since, until):
    """Dedup by key (repeated lines carry identical usage), then sum per family."""
    seen, totals = set(), {}
    for key, when, model, usage in entries:
        if key in seen or when < since or (until and when > until):
            continue
        seen.add(key)
        row = totals.setdefault(family(model), [0, 0, 0, 0, 0])
        for i, name in enumerate(FIELDS):
            row[i] += usage.get(name) or 0
        # Last slot: the part of cache writes billed at the 1 h rate.
        detail = usage.get("cache_creation")
        if isinstance(detail, dict):
            row[4] += detail.get("ephemeral_1h_input_tokens") or 0
    return totals


def cost(fam, row):
    inp, out, cache = PRICES.get(fam, (0, 0, 0))
    return (row[0] * inp + row[1] * out + row[2] * cache + (row[3] - row[4]) * inp * 1.25 + row[4] * inp * 2) / 1e6


def short(n):
    return f"{n / 1e6:.1f}M".replace(".", ",") if n >= 1_000_000 else f"{round(n / 1000)}k"


def fmt_line(totals):
    parts = []
    for fam in SHOWN + sorted(f for f in totals if f not in SHOWN):
        row = totals.get(fam, [0, 0, 0, 0, 0])
        text = f"{fam.capitalize()} {short(sum(row))}" if sum(row) else f"{fam.capitalize()} 0"
        if row[2] > 0:
            text += f" (dont cache lu {short(row[2])})"
        parts.append(text)
    total = sum(cost(f, r) for f, r in totals.items())
    return " · ".join(parts) + f" · ≈ {total:.2f} $ équivalent API".replace(".", ",")


def fmt_detail(totals):
    lines = []
    for fam, row in sorted(totals.items()):
        lines.append(f"{fam.capitalize()}: entrée {row[0]}, sortie {row[1]}, cache lu {row[2]}, "
                     f"cache écrit {row[3]}, ≈ {cost(fam, row):.2f} $".replace(".", ","))
    return "\n".join(lines)


def read_usage(path):
    """Return log records sorted by time; missing file or invalid lines give nothing."""
    rows = []
    try:
        with open(pathlib.Path(path).expanduser(), encoding="utf-8", errors="replace") as fh:
            for line in fh:
                try:
                    rows.append(json.loads(line))
                except ValueError:
                    continue
    except OSError:
        return []
    return sorted((r for r in rows if isinstance(r, dict) and isinstance(r.get("ts"), (int, float))),
                  key=lambda r: r["ts"])


def window_delta(rows, key, since, until):
    """Plan usage gain (points, reset flag) of one window between two instants, or None."""
    pts = []
    for r in rows:
        w = r.get(key)
        if isinstance(w, dict) and isinstance(w.get("pct"), (int, float)):
            pts.append((r["ts"], w["pct"], w.get("resets_at")))
    before = [p for p in pts if p[0] <= since]
    inside = [p for p in pts if since < p[0] <= until]
    # Baseline: last reading at or before `since`; else the first reading of the period.
    chain = before[-1:] + inside
    if len(chain) < 2:
        return (0, False) if before else None
    groups = []  # consecutive readings sharing the same reset time
    for p in chain:
        if groups and groups[-1][0][2] == p[2]:
            groups[-1].append(p)
        else:
            groups.append([p])
    total = 0
    for i, g in enumerate(groups):
        start = chain[0][1] if i == 0 else 0
        top = g[-1][1] if i == len(groups) - 1 else max(p[1] for p in g)
        total += top - start
    return total, len(groups) > 1


def fmt_plan(rows, since, until):
    parts = []
    for key, label in WINDOWS:
        res = window_delta(rows, key, since.timestamp(), until.timestamp())
        if res:
            gain, reset = res
            parts.append(f"{gain:+.0f} % sur {label}" + (" (remise à zéro)" if reset else ""))
    return "Plan : " + (", ".join(parts) if parts else "pas de relevé")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--since", required=True)
    ap.add_argument("--until")
    ap.add_argument("--dir", default=DEFAULT_DIR)
    ap.add_argument("--usage-log", default=DEFAULT_USAGE_LOG)
    ap.add_argument("--detail", action="store_true")
    args = ap.parse_args()
    until = parse_time(args.until) if args.until else None
    totals = aggregate(read_entries(args.dir), parse_time(args.since), until)
    print(fmt_line(totals))
    now = datetime.fromtimestamp(time.time(), timezone.utc)
    print(fmt_plan(read_usage(args.usage_log), parse_time(args.since), until or now))
    if args.detail:
        print(fmt_detail(totals))


if __name__ == "__main__":
    main()
