#!/usr/bin/env python3
"""Unit tests for create_worktree.py helper functions."""

import unittest
from create_worktree import clean_title_tags, format_branch_name, slugify


class TestCreateWorktree(unittest.TestCase):
    def test_clean_title_tags(self):
        self.assertEqual(
            clean_title_tags("feat: Host Financial Earnings & Payout Ledger"),
            "Host Financial Earnings & Payout Ledger",
        )
        self.assertEqual(
            clean_title_tags("feat(finance): Host Financial Earnings & Payout Ledger"),
            "Host Financial Earnings & Payout Ledger",
        )
        self.assertEqual(
            clean_title_tags("[feat] Host Financial Earnings & Payout Ledger"),
            "Host Financial Earnings & Payout Ledger",
        )
        self.assertEqual(
            clean_title_tags("feat - Host Financial Earnings & Payout Ledger"),
            "Host Financial Earnings & Payout Ledger",
        )
        self.assertEqual(
            clean_title_tags("feat/65: Host Financial Earnings & Payout Ledger"),
            "Host Financial Earnings & Payout Ledger",
        )
        self.assertEqual(
            clean_title_tags("#65: Host Financial Earnings & Payout Ledger"),
            "Host Financial Earnings & Payout Ledger",
        )
        self.assertEqual(
            clean_title_tags("Feature flags for admin roles"),
            "Feature flags for admin roles",
        )

    def test_format_branch_name_deduplication(self):
        # Issue 65: duplicate 'feat' stripped
        self.assertEqual(
            format_branch_name(65, "feat: Host Financial Earnings & Payout Ledger", prefix="feat"),
            "feat/65-host-financial-earnings-payout-ledger",
        )
        self.assertEqual(
            format_branch_name(65, "feat(finance): Host Financial Earnings & Payout Ledger", prefix="feat"),
            "feat/65-host-financial-earnings-payout-ledger",
        )
        self.assertEqual(
            format_branch_name(65, "feat-host-financial-earnings-payout-ledger", prefix="feat"),
            "feat/65-host-financial-earnings-payout-ledger",
        )
        # Issue 67
        self.assertEqual(
            format_branch_name(67, "feat: Implement Core Actuator Functionality", prefix="feat"),
            "feat/67-implement-core-actuator-functionality",
        )

    def test_format_branch_name_standards(self):
        self.assertEqual(
            format_branch_name(46, "Add listing from existing listing", prefix="feat"),
            "feat/46-add-listing-from-existing-listing",
        )
        self.assertEqual(
            format_branch_name(47, "Update check-in times", prefix="feat"),
            "feat/47-update-check-in-times",
        )
        self.assertEqual(
            format_branch_name(54, "Add user profile features", prefix="feat"),
            "feat/54-add-user-profile-features",
        )
        self.assertEqual(
            format_branch_name(56, "Refactor pricing into its own module", prefix=""),
            "56-refactor-pricing-into-its-own-module",
        )
        self.assertEqual(
            format_branch_name(76, "Update agent skills", prefix="feat"),
            "feat/76-update-agent-skills",
        )

    def test_format_branch_name_empty_slug_has_no_trailing_hyphen(self):
        self.assertEqual(format_branch_name(9, "feat", prefix="feat"), "feat/9")
        self.assertEqual(format_branch_name(9, "feat:", prefix="feat"), "feat/9")
        self.assertEqual(format_branch_name(9, "#9", prefix=""), "9")

    def test_format_branch_name_length_limit(self):
        long_title = "A" * 100
        branch = format_branch_name(123, long_title, prefix="feat", max_length=62)
        self.assertLess(len(branch), 63)
        self.assertFalse(branch.endswith("-"))


if __name__ == "__main__":
    unittest.main()
