import io
import tempfile
import unittest
from collections import Counter
from contextlib import redirect_stdout
from pathlib import Path

import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import tool_usage_stats


class ToolUsageStatsTests(unittest.TestCase):
    def test_parse_tool_usage_reads_current_project_log_format(self):
        with tempfile.TemporaryDirectory() as tmp:
            log_file = Path(tmp) / "julie.log.2026-05-18"
            log_file.write_text(
                "\n".join(
                    [
                        "2026-05-18T12:00:00 INFO tool_call: fast_search (12.3ms, 456 bytes output)",
                        "2026-05-18T12:00:01 INFO tool_call: deep_dive (4.0ms, 100 bytes output)",
                        "2026-05-18T12:00:02 INFO unrelated line",
                    ]
                ),
                encoding="utf-8",
            )

            counts = tool_usage_stats.parse_tool_usage([log_file])

        self.assertEqual(counts, Counter({"fast_search": 1, "deep_dive": 1}))

    def test_parse_tool_usage_still_reads_legacy_executing_tool_format(self):
        with tempfile.TemporaryDirectory() as tmp:
            log_file = Path(tmp) / "julie.log.legacy"
            log_file.write_text(
                "2026-05-18T12:00:00 INFO 🛠️ Executing tool: get_symbols\n",
                encoding="utf-8",
            )

            counts = tool_usage_stats.parse_tool_usage([log_file])

        self.assertEqual(counts, Counter({"get_symbols": 1}))

    def test_print_stats_uses_known_tool_count_for_unused_summary(self):
        output = io.StringIO()

        with redirect_stdout(output):
            tool_usage_stats.print_stats(
                Counter({"fast_search": 2, "deep_dive": 1}),
                3,
                known_tools={"fast_search", "deep_dive", "get_symbols"},
            )

        self.assertIn("1 tools have zero usage in this period", output.getvalue())
        self.assertNotIn("13 tools have zero usage in this period", output.getvalue())

    def test_discover_known_tools_reads_handler_router_composition(self):
        with tempfile.TemporaryDirectory() as tmp:
            handler = Path(tmp) / "src" / "handler.rs"
            handler.parent.mkdir()
            handler.write_text(
                """
                Self::tool_router_fast_search()
                    + Self::tool_router_deep_dive()
                    + Self::tool_router_manage_workspace()
                """,
                encoding="utf-8",
            )

            tools = tool_usage_stats.discover_known_tools(tmp)

        self.assertEqual(tools, {"fast_search", "deep_dive", "manage_workspace"})


if __name__ == "__main__":
    unittest.main()
