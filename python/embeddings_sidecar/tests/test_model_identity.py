"""Tests for model identity snapshot hashing."""

from __future__ import annotations

import hashlib
import os
from pathlib import Path, PureWindowsPath
import tempfile
import unittest

from sidecar.model_identity import (
    IDENTITY_UNAVAILABLE,
    _CACHE,
    compute_snapshot_fingerprint,
    is_valid_digest,
)


class TestModelIdentity(unittest.TestCase):
    def setUp(self) -> None:
        _CACHE.clear()
    def test_model_identity_changes_after_weights_replaced(self) -> None:
        """Verify fingerprint is stable on unchanged files and changes when weights change."""
        with tempfile.TemporaryDirectory() as tmp_dir:
            snapshot_dir = Path(tmp_dir)

            # 1. Setup initial weights and tokenizer config
            weights_file = snapshot_dir / "model.safetensors"
            config_file = snapshot_dir / "tokenizer_config.json"

            weights_file.write_bytes(b"initial weights data version 1")
            config_file.write_text('{"model_type": "bert", "vocab_size": 30522}', encoding="utf-8")

            # 2. Compute fingerprint
            fp1 = compute_snapshot_fingerprint(snapshot_dir)
            self.assertTrue(
                is_valid_digest(fp1),
                f"expected valid 64-char hex digest, got {fp1!r}",
            )

            # 3. Unchanged content returns stable fingerprint
            fp1_repeat = compute_snapshot_fingerprint(snapshot_dir)
            self.assertEqual(fp1, fp1_repeat)

            # 4. Replace weights while keeping model ID / config unchanged
            weights_file.write_bytes(b"modified weights data version 2 with different bytes")
            fp2 = compute_snapshot_fingerprint(snapshot_dir)
            self.assertTrue(
                is_valid_digest(fp2),
                f"expected valid 64-char hex digest, got {fp2!r}",
            )
            self.assertNotEqual(fp1, fp2, "fingerprint must change when weights bytes change")

            # 5. Missing required files returns IdentityUnavailable
            weights_file.unlink()
            fp_missing_weights = compute_snapshot_fingerprint(snapshot_dir)
            self.assertEqual(
                fp_missing_weights,
                IDENTITY_UNAVAILABLE,
                "missing weights file must return IdentityUnavailable",
            )

            # Missing config
            weights_file.write_bytes(b"some weights")
            config_file.unlink()
            fp_missing_config = compute_snapshot_fingerprint(snapshot_dir)
            self.assertEqual(
                fp_missing_config,
                IDENTITY_UNAVAILABLE,
                "missing config file must return IdentityUnavailable",
            )

    def test_non_weight_files_without_weights_return_identity_unavailable(self) -> None:
        """Verify non-weight files (e.g. model_card.md, model.txt) do not satisfy weights requirement."""
        with tempfile.TemporaryDirectory() as tmp_dir:
            snapshot_dir = Path(tmp_dir)
            # Only config and non-weight documentation/text files, no actual weights
            (snapshot_dir / "config.json").write_text('{"model_type": "bert"}', encoding="utf-8")
            (snapshot_dir / "model_card.md").write_text("# Model Card\nDocumentation only.", encoding="utf-8")
            (snapshot_dir / "model.txt").write_text("model description", encoding="utf-8")
            (snapshot_dir / "model_index.json").write_text('{"pipeline": "feature-extraction"}', encoding="utf-8")

            fp = compute_snapshot_fingerprint(snapshot_dir)
            self.assertEqual(
                fp,
                IDENTITY_UNAVAILABLE,
                "presence of non-weight files starting with 'model' must not satisfy weights requirement",
            )

    def test_adding_documentation_files_does_not_alter_fingerprint(self) -> None:
        """Verify adding non-weight doc files (model_card.md, README.md) does not alter fingerprint."""
        with tempfile.TemporaryDirectory() as tmp_dir:
            snapshot_dir = Path(tmp_dir)
            weights_file = snapshot_dir / "model.safetensors"
            config_file = snapshot_dir / "config.json"

            weights_file.write_bytes(b"model weights binary data")
            config_file.write_text('{"hidden_size": 384}', encoding="utf-8")

            fp1 = compute_snapshot_fingerprint(snapshot_dir)
            self.assertTrue(is_valid_digest(fp1))

            # Add model_card.md (starts with 'model', but is not a weight)
            (snapshot_dir / "model_card.md").write_text("# Model Card\nUpdated docs.", encoding="utf-8")
            fp2 = compute_snapshot_fingerprint(snapshot_dir)
            self.assertEqual(
                fp1,
                fp2,
                "adding model_card.md must not alter weights detection or fingerprint",
            )

            # Add other documentation and non-weight files
            (snapshot_dir / "README.md").write_text("# README", encoding="utf-8")
            (snapshot_dir / "model.txt").write_text("model description notes", encoding="utf-8")
            fp3 = compute_snapshot_fingerprint(snapshot_dir)
            self.assertEqual(
                fp1,
                fp3,
                "adding README.md or model.txt must not alter fingerprint",
            )

    def test_replacing_weights_with_earlier_or_equal_mtime_invalidates_cache(self) -> None:
        """Verify cache invalidates when weights are replaced with mtime <= max_mtime."""
        with tempfile.TemporaryDirectory() as tmp_dir:
            snapshot_dir = Path(tmp_dir)
            weights_file = snapshot_dir / "model.safetensors"
            config_file = snapshot_dir / "config.json"

            weights_file.write_bytes(b"weights version 1 original")
            config_file.write_text('{"hidden_size": 384}', encoding="utf-8")

            # Set explicit timestamps: weights at t=1000s, config at t=2000s
            os.utime(weights_file, (1000, 1000))
            os.utime(config_file, (2000, 2000))

            fp1 = compute_snapshot_fingerprint(snapshot_dir)
            self.assertTrue(is_valid_digest(fp1))

            # Replace weights with new content and set mtime to 1500s (less than config mtime 2000s)
            weights_file.write_bytes(b"weights version 2 replaced with different bytes")
            os.utime(weights_file, (1500, 1500))

            fp2 = compute_snapshot_fingerprint(snapshot_dir)
            self.assertTrue(is_valid_digest(fp2))
            self.assertNotEqual(
                fp1,
                fp2,
                "cache must invalidate and return new fingerprint when weights replaced with mtime <= max_mtime",
            )

            # Also verify replacing weights with equal mtime (1000s) and different size/content
            weights_file.write_bytes(b"weights v3 distinct content")
            os.utime(weights_file, (1000, 1000))
            fp3 = compute_snapshot_fingerprint(snapshot_dir)
            self.assertTrue(is_valid_digest(fp3))
            self.assertNotEqual(fp2, fp3)
            self.assertNotEqual(fp1, fp3)

    def test_deleting_config_file_invalidates_cache(self) -> None:
        """Verify cache invalidates when a configuration file is deleted from the snapshot."""
        with tempfile.TemporaryDirectory() as tmp_dir:
            snapshot_dir = Path(tmp_dir)
            weights_file = snapshot_dir / "model.safetensors"
            config_file = snapshot_dir / "config.json"
            tokenizer_file = snapshot_dir / "tokenizer.json"

            weights_file.write_bytes(b"model weights content")
            config_file.write_text('{"hidden_size": 384}', encoding="utf-8")
            tokenizer_file.write_text('{"vocab": ["a", "b"]}', encoding="utf-8")

            # Set explicit timestamps
            os.utime(weights_file, (1000, 1000))
            os.utime(tokenizer_file, (1200, 1200))
            os.utime(config_file, (2000, 2000))

            fp1 = compute_snapshot_fingerprint(snapshot_dir)
            self.assertTrue(is_valid_digest(fp1))

            # Delete tokenizer.json (max_mtime among remaining files is still 2000 from config.json)
            tokenizer_file.unlink()

            fp2 = compute_snapshot_fingerprint(snapshot_dir)
            self.assertTrue(is_valid_digest(fp2))
            self.assertNotEqual(
                fp1,
                fp2,
                "deleting tokenizer.json must invalidate cache and change fingerprint",
            )

    def test_relative_paths_format_consistently_as_posix(self) -> None:
        """Verify relative paths are formatted with POSIX forward slashes regardless of platform."""
        with tempfile.TemporaryDirectory() as tmp_dir:
            snapshot_dir = Path(tmp_dir)
            sub_dir = snapshot_dir / "1_Pooling"
            sub_dir.mkdir()

            weights_file = snapshot_dir / "model.safetensors"
            config_file = sub_dir / "config.json"

            weights_bytes = b"weights data for posix test"
            config_bytes = b'{"pooling_mode": "cls"}'

            weights_file.write_bytes(weights_bytes)
            config_file.write_text(config_bytes.decode("utf-8"), encoding="utf-8")

            fp = compute_snapshot_fingerprint(snapshot_dir)
            self.assertTrue(is_valid_digest(fp))

            # Manually calculate expected SHA-256 using strict POSIX path separators
            expected_hasher = hashlib.sha256()

            # Files sorted by POSIX relative path:
            # 1. "1_Pooling/config.json"
            # 2. "model.safetensors"
            file1_rel = "1_Pooling/config.json".encode("utf-8")
            file1_len = len(config_bytes)
            expected_hasher.update(file1_rel)
            expected_hasher.update(b"\x00")
            expected_hasher.update(file1_len.to_bytes(8, byteorder="big"))
            expected_hasher.update(config_bytes)

            file2_rel = "model.safetensors".encode("utf-8")
            file2_len = len(weights_bytes)
            expected_hasher.update(file2_rel)
            expected_hasher.update(b"\x00")
            expected_hasher.update(file2_len.to_bytes(8, byteorder="big"))
            expected_hasher.update(weights_bytes)

            expected_digest = expected_hasher.hexdigest().lower()
            self.assertEqual(
                fp,
                expected_digest,
                "fingerprint must match POSIX-formatted canonical digest exactly",
            )

            # Also verify PureWindowsPath conversion behavior
            win_path = PureWindowsPath("1_Pooling\\config.json")
            self.assertEqual(win_path.as_posix(), "1_Pooling/config.json")


if __name__ == "__main__":
    unittest.main()
