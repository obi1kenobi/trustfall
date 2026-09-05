# Development shell for targets whose builds need a standard CPython, such as the
# `pytrustfall` bindings. Host Pythons that are free-threaded or RustPython-based make
# pyo3's build script fail with:
#   "RustPython only supports targeting abi3t, it does not allow targeting other Python ABIs"
#
# Usage:
#   nix-shell                     # then: cargo check -p pytrustfall
#   nix-shell --run 'maturin develop --uv'
#
# Cargo and the rest of the host toolchain stay on PATH; this shell only guarantees
# a pyo3-compatible Python and the maturin build frontend.
{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  packages = with pkgs; [
    python3
    maturin
    uv
  ];

  shellHook = ''
    export PYO3_PYTHON="$(command -v python3)"
    echo "pytrustfall dev shell: using PYO3_PYTHON=$PYO3_PYTHON ($($PYO3_PYTHON --version 2>&1))"
  '';
}
