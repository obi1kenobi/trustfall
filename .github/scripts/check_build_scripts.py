#!/usr/bin/env python3
"""Enforce the repository's content-pinned allowlist of Cargo build scripts.

The policy intentionally treats build-script discovery conservatively:

* Every file beneath ``--repository`` (outside explicitly excluded metadata and
  package-manager artifact directories) whose basename case-folds to ``build.rs``
  is treated as a build script, whether it is tracked, untracked, or ignored by
  Git. This catches case variants such as ``BUILD.RS`` that Cargo may resolve on
  case-insensitive filesystems such as Windows.
* An approved build script must be a regular, non-symlink file at an exact
  lowercase ``build.rs`` path, and its SHA-256 must match the exceptions file.
  Pinning both path and contents prevents an approved path from becoming a place
  where arbitrary code may be substituted.
* Every file beneath ``--repository`` whose basename case-folds to
  ``Cargo.toml`` is parsed as TOML. Parse failures and symlinked manifests fail
  closed. Any ``[package]`` ``build`` key is forbidden, including
  ``build = false``; approved conventional ``build.rs`` files are the only
  repository-local exception mechanism.

Normally, every entry in the exceptions file must be present. This detects stale
policy entries. When a trusted current policy is applied to another selected
branch, ``--allow-missing-approved-build-scripts`` may be enabled because that
branch might not contain every currently approved path. The option only permits
absence: every build script that is present must still have an approved path and
matching digest, and every manifest is still checked normally.

The scan is independent of Git and includes ignored and generated files that
already exist beneath ``--repository``. Git metadata directories named ``.git``
and JavaScript package-manager artifact directories named ``node_modules``,
``.pnpm``, or ``.pnpm-store`` are excluded. Outside those excluded trees,
traversable directory symlinks fail closed because otherwise a path alias could
redirect traversal or conceal repository source inputs. Broken or cyclic
symlinks expose no files at check time; symlinks whose own names are policy
inputs are still discovered and rejected.
"""

from __future__ import annotations

import argparse
import hashlib
import os
from pathlib import Path, PurePosixPath
import re
import sys
import tomllib


DEFAULT_EXCEPTIONS = (
    Path(__file__).resolve().parent.parent / "build-script-exceptions.txt"
)
SHA256_RE = re.compile(r"[0-9a-f]{64}")
IGNORED_DIRECTORY_NAMES = frozenset({".git", ".pnpm", ".pnpm-store", "node_modules"})


class PolicyError(Exception):
    """Raised when the build-script policy is invalid or violated."""


def _normalize_repository_path(raw_path: str, *, line_number: int) -> str:
    path = PurePosixPath(raw_path)
    if (
        path.is_absolute()
        or path.as_posix() != raw_path
        or any(part in {"", ".", ".."} for part in path.parts)
    ):
        raise PolicyError(
            f"invalid repository-relative path on exceptions line {line_number}: "
            f"{raw_path!r}"
        )
    if path.name != "build.rs":
        raise PolicyError(
            f"exception on line {line_number} is not for a build.rs file: {raw_path!r}"
        )
    return path.as_posix()


def load_exceptions(path: Path) -> dict[str, str]:
    """Load a mapping from approved build.rs paths to their expected SHA-256."""
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError as error:
        raise PolicyError(f"failed to read exceptions file {path}: {error}") from error

    exceptions: dict[str, str] = {}
    listed_paths: list[str] = []
    for line_number, line in enumerate(lines, start=1):
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue

        fields = stripped.split(maxsplit=1)
        if len(fields) != 2 or SHA256_RE.fullmatch(fields[0]) is None:
            raise PolicyError(
                f"invalid exception on line {line_number}; expected "
                "'<SHA-256>  <repository-relative path>'"
            )

        digest, raw_path = fields
        repository_path = _normalize_repository_path(
            raw_path, line_number=line_number
        )
        if repository_path in exceptions:
            raise PolicyError(f"duplicate exception for {repository_path!r}")
        exceptions[repository_path] = digest
        listed_paths.append(repository_path)

    if listed_paths != sorted(listed_paths):
        raise PolicyError("build-script exceptions must be sorted by path")

    return exceptions


def discover_policy_inputs(
    repository: Path,
) -> tuple[list[str], list[str], list[str]]:
    """Discover build scripts, manifests, and fail-closed traversal errors."""
    build_scripts: list[str] = []
    manifests: list[str] = []
    errors: list[str] = []

    def traversal_error(error: OSError) -> None:
        raise PolicyError(
            f"failed to inspect repository contents: {error}"
        ) from error

    try:
        for root, directory_names, file_names in os.walk(
            repository, topdown=True, onerror=traversal_error, followlinks=False
        ):
            root_path = Path(root)
            retained_directories: list[str] = []
            for name in sorted(directory_names):
                if name in IGNORED_DIRECTORY_NAMES:
                    continue

                absolute_path = root_path / name
                if absolute_path.is_symlink():
                    repository_path = absolute_path.relative_to(repository).as_posix()
                    errors.append(f"directory symlink is forbidden: {repository_path}")
                    continue

                retained_directories.append(name)

            directory_names[:] = retained_directories

            for name in sorted(file_names):
                folded_name = name.casefold()
                if folded_name not in {"build.rs", "cargo.toml"}:
                    continue

                repository_path = (root_path / name).relative_to(repository).as_posix()
                if folded_name == "build.rs":
                    build_scripts.append(repository_path)
                else:
                    manifests.append(repository_path)
    except OSError as error:
        traversal_error(error)

    return sorted(build_scripts), sorted(manifests), errors


def _sha256(path: Path) -> str:
    hasher = hashlib.sha256()
    try:
        with path.open("rb") as file:
            for chunk in iter(lambda: file.read(1024 * 1024), b""):
                hasher.update(chunk)
    except OSError as error:
        raise PolicyError(f"failed to hash {path}: {error}") from error
    return hasher.hexdigest()


def check_repository(
    repository: Path,
    exceptions_path: Path,
    *,
    allow_missing_approved_build_scripts: bool = False,
) -> tuple[int, int]:
    """Check a repository and return its manifest and build-script counts.

    Args:
        repository: Directory tree whose files will be inspected.
        exceptions_path: Trusted path-and-SHA-256 build-script exceptions.
        allow_missing_approved_build_scripts: Allow an exception's approved path
            to be absent when applying a current trusted policy to another
            selected branch. This does not relax checks on any file that is
            present.

    Returns:
        The number of checked manifests and approved build scripts.
    """
    repository = repository.resolve()
    if not repository.is_dir():
        raise PolicyError(f"repository is not a directory: {repository}")

    exceptions = load_exceptions(exceptions_path.resolve())
    build_scripts, manifests, errors = discover_policy_inputs(repository)

    for path in build_scripts:
        expected_digest = exceptions.get(path)
        if expected_digest is None:
            errors.append(f"unapproved build script: {path}")
            continue

        absolute_path = repository / path
        if absolute_path.is_symlink() or not absolute_path.is_file():
            errors.append(f"approved build script is not a regular file: {path}")
            continue

        actual_digest = _sha256(absolute_path)
        if actual_digest != expected_digest:
            errors.append(
                f"approved build script changed: {path}\n"
                f"  expected SHA-256: {expected_digest}\n"
                f"  actual SHA-256:   {actual_digest}"
            )

    if not allow_missing_approved_build_scripts:
        for path in sorted(exceptions.keys() - set(build_scripts)):
            errors.append(f"stale exception for missing build script: {path}")

    for path in manifests:
        absolute_path = repository / path
        if absolute_path.is_symlink() or not absolute_path.is_file():
            errors.append(f"Cargo manifest is not a regular file: {path}")
            continue

        try:
            with absolute_path.open("rb") as file:
                manifest = tomllib.load(file)
        except (OSError, tomllib.TOMLDecodeError) as error:
            errors.append(f"failed to parse {path}: {error}")
            continue

        package = manifest.get("package")
        if isinstance(package, dict) and "build" in package:
            errors.append(
                f"custom [package] build setting is forbidden: {path} "
                f"(build = {package['build']!r})"
            )

    if errors:
        formatted = "\n".join(f"- {error}" for error in errors)
        raise PolicyError(f"build-script policy violations:\n{formatted}")

    return len(manifests), len(build_scripts)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "--repository",
        type=Path,
        default=Path.cwd(),
        help="repository tree to inspect (default: current directory)",
    )
    parser.add_argument(
        "--exceptions",
        type=Path,
        default=DEFAULT_EXCEPTIONS,
        help="trusted build-script exceptions file",
    )
    parser.add_argument(
        "--allow-missing-approved-build-scripts",
        action="store_true",
        help=(
            "allow approved build scripts to be absent from the inspected checkout; "
            "intended for applying a current trusted policy to another branch"
        ),
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        manifest_count, build_script_count = check_repository(
            args.repository,
            args.exceptions,
            allow_missing_approved_build_scripts=(
                args.allow_missing_approved_build_scripts
            ),
        )
    except PolicyError as error:
        print(error, file=sys.stderr)
        return 1

    print(
        "Build-script policy passed: "
        f"checked {manifest_count} Cargo manifests and "
        f"{build_script_count} approved build scripts."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
