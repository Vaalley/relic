# Relic CLI Performance Budget Benchmark

This directory contains the benchmarking harness used to measure the execution times of key operations in the Relic CLI against the defined performance budgets.

## Purpose

The benchmark ensures that the performance of the Relic command-line interface stays within the budgets established in [PLAN.md](../../PLAN.md#8-performance-size--reliability-budgets). This prevents regressions in:
- Cold scan times for large ROM libraries.
- Incremental rescan times when no changes are made.
- Game listing query latency.
- Full-text search (FTS) query latency.

## Usage

To execute the benchmark, run the PowerShell script from the repository root:

```powershell
pwsh -File tools/bench/run.ps1 [-Files <int>] [-KeepArtifacts]
```

### Parameters
* `-Files` (default: `10000`): The target number of synthetic ROM files to generate for the library benchmark.
* `-KeepArtifacts` (switch): If set, the generated synthetic ROM files and SQLite databases will not be deleted upon completion, allowing for manual debugging or inspection.

### Example (Quick test)
```powershell
pwsh -File tools/bench/run.ps1 -Files 1200
```

## Performance Budgets

The benchmark measures and evaluates performance against budgets scaled to the library size ($Files):

| Operation | Scale Factor / Base Budget | Budget ($Files = 10,000) | Notes |
|---|---|---|---|
| **Cold Initial Scan** | 30s * (Files / 10,000) | 30.0s | Scales linearly with library size |
| **Incremental Rescan** | 2s * (Files / 10,000) | 2.0s | Scales linearly; assumes no changes |
| **Games Query** | Flat 100ms | 100.0ms | List games for a single system |
| **Search Query** | Flat 100ms | 100.0ms | Full-text search (FTS) query |

### CLI vs. Engine Overhead Note
The base query budget defined in [PLAN.md](../../PLAN.md) is **30ms**. However, that budget is defined specifically for the *in-process engine* (i.e. direct SQLite queries inside a running shell application).

Because this benchmark runs at the **CLI process level**, it includes additional overhead from:
1. OS process spawning and teardown.
2. Binary startup and argument parsing.
3. Database file opening and connection pool initialization.

To account for this CLI process overhead, the budget used for the CLI-level check is increased to **100ms**.

## Warning: Temporary Artifacts
All generated files, ROM libraries, and databases are created dynamically in a temporary directory on each run. 
* These synthetic files are used **only** for the duration of the benchmark.
* They are automatically cleaned up when the script exits (unless `-KeepArtifacts` is specified).
* Do **not** commit any of these temporary folders, mock ROMs, or test databases to the repository.
