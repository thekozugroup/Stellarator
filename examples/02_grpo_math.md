# Example 2: GRPO for Math Reasoning

A complete example: an agent designs and launches a reinforcement learning run to improve math reasoning using Group Relative Policy Optimization (GRPO).

## Scenario

You want to improve a model's accuracy on mathematical reasoning benchmarks (MATH, GSM8K). You'll use GRPO, a sample-efficient RL method that learns from model-generated solutions compared within groups.

## Step 1: Literature Review

OpenAI Chat agent searches for recent GRPO papers:

```python
papers = search_hf_papers("group relative policy optimization")
# Returns papers on GRPO and related methods

relevant_papers = [
    "Group Relative Policy Optimization for Math Reasoning",
    "Let's Verify Step by Step (process reward models)",
    "Outcome Reward Models for Guided Policy Optimization"
]
```

## Step 2: Design RL Configuration

Based on papers, the agent designs hyperparameters for GRPO:

```python
dataset_mixture = [
    {
        "name": "gsm8k",
        "split": "train",
        "weight": 0.6,
        "citation_key": "cobbe2021train",
        "note": "Grade-school math word problems; ~7.5K training examples"
    },
    {
        "name": "math",
        "split": "train",
        "weight": 0.4,
        "citation_key": "hendrycks2021measuring",
        "note": "Harder competition-level problems; ~7.5K examples"
    }
]

hyperparameters = {
    "method": "grpo",
    "learning_rate": 5e-6,
    "batch_size": 64,
    "num_generations_per_prompt": 8,  # Sample 8 solutions per problem
    "num_train_epochs": 2,
    "temperature": 0.8,  # Higher temp for diverse reasoning paths
    "top_p": 0.95,
    "max_seq_length": 1024,
    "reward_model": "process",  # Use process reward model (step-by-step correctness)
    "group_size": 8,  # Group 8 samples for relative comparison
}
```

## Step 3: Launch via OpenAI Chat

OpenAI agent sends a POST to Stellarator's chat endpoint:

```bash
curl -X POST http://localhost:8000/v1/chat/sessions/sess_math_001/messages \
  -H "Authorization: Bearer AGENT_TOKEN_OPENAI" \
  -H "Content-Type: application/json" \
  -d '{
    "role": "user",
    "content": "Launch a GRPO run to improve math reasoning. Use GSM8K + MATH, process reward model, 8 samples per prompt."
  }'
```

The OpenAI agent's response is stored in the chat history, and the backend interprets it to create a run:

```json
{
  "id": "run_grpo_math_001",
  "agent_id": "openai",
  "base_model": "meta-llama/Llama-2-13b-hf",
  "method": "grpo",
  "hyperparameters": {
    "learning_rate": 5e-6,
    "batch_size": 64,
    "num_generations_per_prompt": 8,
    "temperature": 0.8,
    "top_p": 0.95,
    "max_seq_length": 1024,
    "reward_model": "process",
    "group_size": 8
  },
  "dataset_mixture": [
    {
      "name": "gsm8k",
      "split": "train",
      "weight": 0.6,
      "citation_key": "cobbe2021train"
    },
    {
      "name": "math",
      "split": "train",
      "weight": 0.4,
      "citation_key": "hendrycks2021measuring"
    }
  ],
  "gpu_type": "A100",
  "gpu_count": 4,
  "user_goal": "Improve model accuracy on mathematical reasoning (MATH and GSM8K benchmarks) using RL",
  "agent_plan": "Use GRPO with a process reward model to learn from step-by-step reasoning quality. Generate 8 diverse solutions per prompt to maximize learning signal. Base hyperparameters on recent work (OpenAI, DeepSeek) showing GRPO converges faster than PPO on math tasks. GSM8K (60%) for curriculum (easier); MATH (40%) for harder examples.",
  "citations": [
    {
      "type": "paper",
      "title": "Let's Verify Step by Step",
      "authors": ["Nils Hendrycks", "Steven Basart", ...],
      "year": 2023,
      "arxiv_id": "2305.20050"
    },
    {
      "type": "paper",
      "title": "Group Relative Policy Optimization for Natural Language Generation",
      "authors": ["OpenAI Researchers"],
      "year": 2024,
      "arxiv_id": "2402.xxxxx"
    },
    {
      "type": "dataset",
      "name": "GSM8K",
      "source": "OpenAI",
      "url": "https://huggingface.co/datasets/openai/gsm8k"
    },
    {
      "type": "dataset",
      "name": "MATH",
      "source": "UC Berkeley",
      "url": "https://huggingface.co/datasets/hendrycks/math"
    }
  ]
}
```

## Step 4: Real-Time Monitoring

Both the OpenAI agent and human operators monitor the run via the dashboard at `/runs`:

```
Run ID:             run_grpo_math_001
Agent:              openai
Status:             RUNNING (42% complete)
Base Model:         meta-llama/Llama-2-13b-hf
Method:             GRPO
GPU Type:           A100 x4
Duration:           8 hours 15 minutes
Cost so far:        $90.24
Reward (avg):       0.72 (trending up)
Reasoning steps:    4.2 on average
```

The dashboard plots:
- Reward curve: Shows agent learning (should increase monotonically)
- Loss curve: Should decrease or stabilize
- Token efficiency: Samples per GPU-second

## Step 5: Add Notes During Training

Codex agent monitors and adds observations:

```bash
curl -X PUT http://localhost:8000/v1/runs/run_grpo_math_001/notes \
  -H "Authorization: Bearer AGENT_TOKEN_CODEX" \
  -H "Content-Type: application/json" \
  -d '{
    "note": "Reward curve is increasing steadily. Reward went from 0.45 → 0.72 in first 8 hours. No signs of mode collapse (variance in solutions remains high). Proceeding to completion."
  }'
```

## Step 6: Post-Training Analysis

After the run finishes:

```json
{
  "id": "run_grpo_math_001",
  "status": "succeeded",
  "finished_at": "2026-04-30T22:30:00Z",
  "gpu_seconds": 90000.0,
  "cost_usd": 440.00,
  "final_reward": 0.81,
  "eval_accuracy_gsm8k": 0.78,
  "eval_accuracy_math": 0.62,
  "reasoning_quality_score": 8.2,
  "agent_id": "openai"
}
```

OpenAI agent summarizes:

```bash
curl -X PUT http://localhost:8000/v1/runs/run_grpo_math_001/notes \
  -H "Authorization: Bearer AGENT_TOKEN_OPENAI" \
  -d '{
    "note": "GRPO training complete. Final reward: 0.81. GSM8K accuracy: 78% (↑12% from baseline 66%). MATH accuracy: 62% (↑8% from baseline 54%). Process reward model effectively guided learning. 25 hours A100 training, cost $440. Ready for evaluation on held-out test set."
  }'
```

## Cost Breakdown

- GPU type: A100 at $2.20/hour
- GPU count: 4
- Training duration: 25 hours
- Cost: (90000 seconds / 3600) × $2.20 × 4 = **$440.00**

Per 1% improvement in GSM8K accuracy: ~$40

---

## Comparison with Example 1

| Aspect | SFT (Example 1) | GRPO (Example 2) |
|--------|-----------------|-----------------|
| Method | Supervised Fine-Tuning | Reinforcement Learning |
| Datasets | Instruction mix (diverse) | Benchmark-focused (focused) |
| Cost | $135 (8×H100, 5h) | $440 (4×A100, 25h) |
| Final metric | Loss 0.76 | Reward 0.81, Accuracy ↑10% |
| Use case | General helpfulness | Specific benchmark improvement |

---

## Why GRPO Works for Math

1. **Reasoning diversity**: Generating 8 solutions per prompt captures different reasoning paths
2. **Process reward signals**: Step-by-step correctness is more informative than binary outcome
3. **Sample efficiency**: GRPO learns from fewer gradient steps than PPO
4. **Benchmark alignment**: Directly optimizes the metric you care about (accuracy on MATH/GSM8K)

---

## Next Steps

- See [examples/03_concurrent_sweep.md](03_concurrent_sweep.md) for launching 6 concurrent runs and comparing
- See [docs/cost.md](../docs/cost.md) for cost optimization strategies
