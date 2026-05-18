#!/usr/bin/env python3
"""
Simple tool usage statistics from Julie logs.
Usage: python scripts/tool_usage_stats.py [--days N] [--json]
"""

import io
import re
import sys
from pathlib import Path
from datetime import datetime, timedelta
from collections import Counter
import json
import argparse

CURRENT_TOOL_PATTERN = re.compile(
    r"\btool_call:\s+([A-Za-z0-9_:-]+)\s+\(\d+(?:\.\d+)?ms,\s+\d+ bytes output\)"
)
LEGACY_TOOL_PATTERN = re.compile(r"🛠️\s+Executing tool: (\S+)")
TOOL_ROUTER_PATTERN = re.compile(r"Self::tool_router_([A-Za-z0-9_]+)\(\)")

# Fix Windows console encoding for emojis
if sys.platform == 'win32':
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8', errors='replace')


def find_log_files(days=1):
    """Find log files for the last N days."""
    log_dir = Path(".julie/logs")
    if not log_dir.exists():
        print(f"❌ Log directory not found: {log_dir}")
        print("💡 Make sure you're running this from the Julie workspace directory")
        sys.exit(1)

    log_files = []
    for i in range(days):
        date = datetime.now() - timedelta(days=i)
        log_file = log_dir / f"julie.log.{date.strftime('%Y-%m-%d')}"
        if log_file.exists():
            log_files.append(log_file)

    return log_files


def discover_known_tools(source_root=None):
    """Return tool names registered in src/handler.rs, or None if unavailable."""
    root = Path.cwd() if source_root is None else Path(source_root)
    handler_path = root / "src" / "handler.rs"
    try:
        handler_source = handler_path.read_text(encoding="utf-8")
    except OSError:
        return None

    tools = set(TOOL_ROUTER_PATTERN.findall(handler_source))
    return tools or None

def parse_tool_usage(log_files):
    """Extract tool usage from current and legacy log files."""
    tools = []

    for log_file in log_files:
        with open(log_file, 'r', encoding='utf-8') as f:
            for line in f:
                match = CURRENT_TOOL_PATTERN.search(line) or LEGACY_TOOL_PATTERN.search(line)
                if match:
                    tools.append(match.group(1))

    return Counter(tools)


def print_stats(tool_counts, total, output_json=False, known_tools=None):
    """Print statistics in human-readable format or JSON."""
    if output_json:
        stats = {
            "total_calls": total,
            "tools": [
                {
                    "name": tool,
                    "count": count,
                    "percentage": round((count / total * 100), 2) if total > 0 else 0
                }
                for tool, count in tool_counts.most_common()
            ]
        }
        print(json.dumps(stats, indent=2))
        return

    print("\n📊 Julie Tool Usage Statistics")
    print("=" * 60)
    print()

    # Print sorted by usage
    print(f"{'Count':<8} {'%':<7} {'Tool':<40}")
    print("-" * 60)
    for tool, count in tool_counts.most_common():
        percentage = (count / total * 100) if total > 0 else 0
        print(f"{count:<8} {percentage:>5.1f}%  {tool}")

    print("-" * 60)
    print(f"Total tool calls: {total}")
    print()

    # Summary insights
    top_5 = tool_counts.most_common(5)
    if top_5:
        top_5_count = sum(count for _, count in top_5)
        top_5_pct = (top_5_count / total * 100) if total > 0 else 0
        print(f"💡 Top 5 tools account for {top_5_pct:.1f}% of all usage")

    if known_tools is None:
        known_tools = discover_known_tools()
    if known_tools:
        unused_count = len(set(known_tools) - set(tool_counts))
        if unused_count > 0:
            print(f"⚠️  {unused_count} tools have zero usage in this period")
    print()


def main():
    parser = argparse.ArgumentParser(description='Analyze Julie tool usage from logs')
    parser.add_argument('--days', type=int, default=1,
                        help='Number of days to analyze (default: 1)')
    parser.add_argument('--json', action='store_true',
                        help='Output as JSON instead of human-readable format')
    args = parser.parse_args()

    log_files = find_log_files(args.days)

    if not log_files:
        print(f"❌ No log files found for the last {args.days} day(s)")
        sys.exit(1)

    if not args.json:
        print(f"Analyzing {len(log_files)} log file(s):")
        for log_file in log_files:
            print(f"  - {log_file.name}")

    tool_counts = parse_tool_usage(log_files)
    total = sum(tool_counts.values())

    if total == 0:
        print("❌ No tool usage found in logs")
        sys.exit(0)

    print_stats(tool_counts, total, args.json)


if __name__ == "__main__":
    main()
