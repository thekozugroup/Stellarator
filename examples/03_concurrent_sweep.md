# Example 3: Concurrent Hyperparameter Sweep

A complete example: launch 6 concurrent training runs in parallel, sweep over hyperparameters, and compare results side-by-side.

## Scenario

You want to find the optimal learning rate and batch size for SFT on a domain-specific dataset. You'll run 6 configurations concurrently and compare loss curves, final metrics, and cost-efficiency.

## Hyperparameter Grid

```python
learning_rates = [1e-5, 2e-5]
batch_sizes = [64, 128]
# 2 × 2 = 4 configs

# Plus 2 additional runs with different seeds for robustness
# Total: 6 runs
```

## Step 1: Design the Sweep

Claude Code designs the sweep matrix:

```python
configs = [
    # Config A: Conservative LR, small batch
    {
        "name": "sweep_lr1e5_bs64",
        "learning_rate": 1e-5,
        "batch_size": 64,
        "seed": 42
    },
    # Config B: Conservative LR, large batch
    {
        "name": "sweep_lr1e5_bs128",
        "learning_rate": 1e-5,
        "batch_size": 128,
        "seed": 42
    },
    # Config C: Aggressive LR, small batch
    {
        "name": "sweep_lr2e5_bs64",
        "learning_rate": 2e-5,
        "batch_size": 64,
        "seed": 42
    },
    # Config D: Aggressive LR, large batch
    {
        "name": "sweep_lr2e5_bs128",
        "learning_rate": 2e-5,
        "batch_size": 128,
        "seed": 42
    },
    # Config E: Best config from prior sweeps, seed 123
    {
        "name": "sweep_best_seed123",
        "learning_rate": 1.5e-5,
        "batch_size": 96,
        "seed": 123
    },
    # Config F: Same as E, seed 456 (robustness check)
    {
        "name": "sweep_best_seed456",
        "learning_rate": 1.5e-5,
        "batch_size": 96,
        "seed": 456
    }
]
```

## Step 2: Launch All 6 Runs

Claude Code launches all runs concurrently. Each POST is independent:

```bash
# Run A
curl -X POST http://localhost:8000/v1/runs \
  -H "Authorization: Bearer AGENT_TOKEN_CLAUDE_CODE" \
  -H "Content-Type: application/json" \
  -d '{
    "base_model": "meta-llama/Llama-2-7b-hf",
    "method": "sft",
    "hyperparameters": {
      "learning_rate": 1e-5,
      "batch_size": 64,
      "num_epochs": 3,
      "seed": 42
    },
    "dataset_mixture": [...],
    "gpu_type": "H100",
    "gpu_count": 8,
    "user_goal": "Hyperparameter sweep: find optimal learning_rate and batch_size",
    "agent_plan": "Sweep 2×2 learning rates (1e-5, 2e-5) × batch sizes (64, 128) + seed robustness. Config A: baseline conservative. Justified by LLaMA paper recommendations.",
    "citations": [...]
  }' \
  > run_a.json

# Run B (same setup, lr=1e-5, bs=128)
# Run C (same setup, lr=2e-5, bs=64)
# ... launch all 6 in parallel
```

**Responses (one per run):**

```json
{
  "id": "run_sweep_a",
  "agent_id": "claude_code",
  "hyperparameters": {"learning_rate": 1e-5, "batch_size": 64},
  "status": "pending",
  "created_at": "2026-04-30T10:00:00Z"
}

{
  "id": "run_sweep_b",
  "agent_id": "claude_code",
  "hyperparameters": {"learning_rate": 1e-5, "batch_size": 128},
  "status": "pending",
  "created_at": "2026-04-30T10:00:01Z"
}

// ... 4 more similar responses
```

## Step 3: Monitor All Runs in Dashboard

Navigate to http://localhost:3000/runs?sweep=true (or tag runs with a "sweep" label):

```
Run            | LR     | BS  | Status   | Duration | Loss  | Cost
─────────────────────────────────────────────────────────────────
run_sweep_a    | 1e-5   | 64  | RUNNING  | 3h 12m   | 0.94  | $115
run_sweep_b    | 1e-5   | 128 | RUNNING  | 3h 15m   | 0.89  | $118
run_sweep_c    | 2e-5   | 64  | RUNNING  | 3h 10m   | 0.81  | $113
run_sweep_d    | 2e-5   | 128 | RUNNING  | 3h 08m   | 0.78  | $111
run_sweep_e    | 1.5e-5 | 96  | RUNNING  | 3h 14m   | 0.82  | $116
run_sweep_f    | 1.5e-5 | 96  | RUNNING  | 3h 11m   | 0.83  | $114
```

## Step 4: Compare Runs Side-by-Side

When all runs complete, navigate to http://localhost:3000/runs/compare and select all 6:

```
FINAL METRICS COMPARISON

Run            | Final Loss | Epochs | Time   | Cost  | Notes
───────────────────────────────────────────────────────────────
run_sweep_a    | 0.89       | 3      | 5.3h   | $133  | Slow convergence
run_sweep_b    | 0.83       | 3      | 5.1h   | $128  | Better, smaller variance
run_sweep_c    | 0.76       | 3      | 5.2h   | $130  | Good convergence
run_sweep_d    | 0.74       | 3      | 5.0h   | $125  | Best loss, but potential instability
run_sweep_e    | 0.77       | 3      | 5.1h   | $128  | Consistent, middle ground
run_sweep_f    | 0.78       | 3      | 5.3h   | $133  | Consistent, seed 456 confirms run_e

WINNER: run_sweep_d (loss 0.74) but run_sweep_e/f more stable. Recommend run_e for production.
```

## Step 5: Plot Loss Curves

The dashboard renders loss curves for all 6 runs overlaid:

```
Loss over time (step):

  1.0 │  AAAA     ┌─────────────────────────────┐
      │  B  B     │ run_sweep_a (1e-5, 64)  ✗   │
      │   C  \    │ run_sweep_b (1e-5, 128) ✓   │
  0.8 │    D__\   │ run_sweep_c (2e-5, 64)  ○   │
      │      \E─E │ run_sweep_d (2e-5, 128) ⚠   │
      │        \_F│ run_sweep_e (1.5e-5, 96)✓  │
  0.6 │          │ run_sweep_f (1.5e-5, 96)✓  │
      │          └─────────────────────────────┘
      └─────────────────────────────────────
        100    200    300    400    500
                Training steps

Legend:
✗ = Did not converge well
✓ = Recommended
○ = Interesting edge case
⚠ = Converged well but may be unstable (single seed)
```

## Step 6: Aggregate Results

Claude Code compiles a summary:

```python
sweep_results = {
    "objective": "Find optimal LR and batch size for domain SFT",
    "total_cost": 134 + 128 + 130 + 125 + 128 + 133,  # ~$778 total
    "best_config": {
        "id": "run_sweep_e",
        "learning_rate": 1.5e-5,
        "batch_size": 96,
        "final_loss": 0.77,
        "loss_std_across_seeds": 0.01,  # Low variance across E and F
        "cost_per_loss_point": 778 / (1.0 - 0.77)  # ~$33.8 per 0.1 loss improvement
    },
    "recommendations": [
        "Use lr=1.5e-5, batch_size=96 for production training",
        "Learning rate 2e-5 was too aggressive (unstable for single seed)",
        "Learning rate 1e-5 was too conservative (slower convergence)",
        "Batch size 96 balances gradient stability and training speed"
    ]
}

client.stellarator_run_add_note(
    "run_sweep_a",
    f"Sweep complete. Recommended config: lr=1.5e-5, bs=96. Total cost $778. Details in sweep report."
)
```

## Step 7: Launch Production Run with Winning Config

Based on the sweep, launch a larger production run:

```bash
curl -X POST http://localhost:8000/v1/runs \
  -H "Authorization: Bearer AGENT_TOKEN_CLAUDE_CODE" \
  -H "Content-Type: application/json" \
  -d '{
    "base_model": "meta-llama/Llama-2-7b-hf",
    "method": "sft",
    "hyperparameters": {
      "learning_rate": 1.5e-5,
      "batch_size": 96,
      "num_epochs": 5,  # Longer training
      "seed": 99  # Fresh seed
    },
    "dataset_mixture": [...],
    "gpu_type": "H100",
    "gpu_count": 8,
    "user_goal": "Production SFT with hyperparameters selected from concurrent sweep",
    "agent_plan": "From sweep of 6 configs, lr=1.5e-5 and bs=96 showed best convergence (loss 0.77) with low variance across seeds. This production run uses same hyperparams but 5 epochs (vs 3 in sweep) for final push.",
    "citations": [
      {"type": "run", "id": "run_sweep_e", "note": "Hyperparameter source"},
      {"type": "run", "id": "run_sweep_f", "note": "Seed robustness confirmation"}
    ]
  }'
```

## Cost Summary

```
Sweep phase:     6 runs × ~$128 each  =  $768 (exploration)
Production run:  1 run × ~$160        =  $160 (exploitation)
────────────────────────────────────────────────────────
Total:                                   $928

Cost-to-value: $928 to determine optimal hyperparams + 
               achieve 0.77 final loss (vs ~0.85 without sweep)
```

---

## Key Takeaways

1. **Concurrent execution**: All 6 runs train in parallel on different GPUs; total wall time ≈ 5 hours (not 6×5=30 hours)
2. **Reproducibility**: Sweep includes multiple seeds to check variance
3. **Transparent comparison**: Dashboard makes it easy to see which config wins
4. **Audit trail**: Each run cites the prior sweep runs, linking the logic chain
5. **Cost-aware**: You know exactly what the exploration cost, enabling ROI analysis

---

## Next Steps

- See [docs/cost.md](../docs/cost.md) for strategies to reduce sweep cost (e.g., earlier stopping)
- See [docs/ownership.md](../docs/ownership.md) to understand how multiple agents can collaborate on a sweep
