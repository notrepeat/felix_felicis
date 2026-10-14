"""Focused tests for the architecture demo runner's process metrics."""

from __future__ import annotations

import os
import sys
import unittest

import run_software_architecture_demo as demo


class ProcessTreeMetricsTests(unittest.TestCase):
    def test_aggregate_includes_root_and_descendants_but_not_siblings(self) -> None:
        snapshot = {
            10: (1, 100),
            11: (10, 200),
            12: (11, 300),
            20: (1, 10_000),
        }

        self.assertEqual((600, 3), demo.aggregate_process_tree_rss(10, snapshot))

    def test_aggregate_handles_empty_or_unobserved_tree(self) -> None:
        self.assertEqual((0, 0), demo.aggregate_process_tree_rss(10, {}))
        self.assertEqual((0, 0), demo.aggregate_process_tree_rss(10, {11: (1, 200)}))

    def test_aggregate_traverses_a_process_with_unreadable_memory(self) -> None:
        snapshot = {10: (1, 100), 11: (10, None), 12: (11, 300)}

        self.assertEqual((400, 2), demo.aggregate_process_tree_rss(10, snapshot))

    def test_metrics_diagnostic_has_stable_fields_and_units(self) -> None:
        sampler = demo.ProcessTreeRSSSampler()
        sampler._samples = 4
        sampler._peak_bytes = 1_234_567
        sampler._peak_processes = 3

        self.assertEqual(
            "METRICS elapsed_ns=987654321 "
            "rss_tree_peak_sampled_bytes=1234567 "
            "rss_tree_sample_interval_ms=100 rss_tree_samples=4 "
            "rss_processes_at_peak=3",
            demo.metrics_diagnostic(987654321, sampler),
        )

    @unittest.skipUnless(
        os.name == "nt" or sys.platform.startswith("linux"),
        "native process-tree RSS sampling is implemented for Windows and Linux",
    )
    def test_native_sampler_measures_the_runner(self) -> None:
        sampler = demo.ProcessTreeRSSSampler()
        sampler.start()
        sampler.stop()

        self.assertGreaterEqual(sampler._samples, 2)
        self.assertGreater(sampler._peak_bytes, 0)
        self.assertGreaterEqual(sampler._peak_processes, 1)


if __name__ == "__main__":
    unittest.main()
