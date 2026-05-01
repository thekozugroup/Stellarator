# Example 1: SFT for Helpfulness

A complete worked example: an agent reads papers on instruction-tuning, designs an SFT dataset mix, launches a run, and monitors training.

## Scenario

You want to improve a 7B model's helpfulness on open-ended user queries. You'll use Supervised Fine-Tuning (SFT) with a mix of public and proprietary instruction datasets.

## Step 1: Research Phase (Claude Code)

Claude Code uses its research tools to find relevant papers:

```python
# In Claude Code with Stellarator MCP
papers = search_arxiv("instruction tuning helpfulness")
# Returns papers like:
# - "Finetuned Language Models Are Zero-Shot Learners" (Wei et al., 2021)
# - "The Flan Collection: Designing Data and Methods for Effective Instruction Tuning" (Wei et al., 2023)

# Review papers and extract citations
selected_papers = papers[:5]
citations = [
    {
        "type": "paper",
        "title": p.title,
        "authors": p.authors,
        "year": p.year,
        "arxiv_id": p.arxiv_id,
        "url": f"https://arxiv.org/abs/{p.arxiv_id}"
    }
    for p in selected_papers
]
```

## Step 2: Design the Dataset Mix

Based on the papers, Claude Code designs a mixture:

```python
dataset_mixture = [
    {
        "name": "flan_v2",
        "split": "train",
        "weight": 0.4,
        "citation_key": "wei2023flan",
        "note": "Diverse task instructions, proven to improve generalization"
    },
    {
        "name": "oasst1",
        "split": "train",
        "weight": 0.3,
        "citation_key": "kopf2023open",
        "note": "Human-written helpful responses from OASST project"
    },
    {
        "name": "internal_helpfulness_feedback",
        "split": "train",
        "weight": 0.3,
        "citation_key": None,
        "note": "Proprietary feedback data from production helpfulness ratings"
    }
]
```

## Step 3: Create the Run

Claude Code calls the MCP tool to launch:

```bash
POST /v1/runs
Authorization: Bearer AGENT_TOKEN_CLAUDE_CODE
Content-Type: application/json

{
  "base_model": "meta-llama/Llama-2-7b-hf",
  "method": "sft",
  "hyperparameters": {
    "learning_rate": 2e-5,
    "batch_size": 128,
    "num_epochs": 3,
    "warmup_steps": 500,
    "weight_decay": 0.01,
    "max_grad_norm": 1.0,
    "max_seq_length": 512
  },
  "dataset_mixture": [
    {
      "name": "flan_v2",
      "split": "train",
      "weight": 0.4,
      "citation_key": "wei2023flan",
      "note": "Diverse task instructions, proven to improve generalization"
    },
    {
      "name": "oasst1",
      "split": "train",
      "weight": 0.3,
      "citation_key": "kopf2023open",
      "note": "Human-written helpful responses from OASST project"
    },
    {
      "name": "internal_helpfulness_feedback",
      "split": "train",
      "weight": 0.3,
      "note": "Proprietary feedback data from production helpfulness ratings"
    }
  ],
  "gpu_type": "H100",
  "gpu_count": 8,
  "user_goal": "Improve model helpfulness on open-ended user queries (customer support, brainstorming, writing assistance)",
  "agent_plan": "Use SFT with a mix of public (Flan, OASST) and internal helpfulness data. Public datasets provide diverse task coverage; internal data tunes for our specific use case. Based on Wei et al. (2023), this mixture improves generalization and domain alignment. Learning rate (2e-5) and batch size (128) follow LLaMA fine-tuning best practices.",
  "citations": [
    {
      "type": "paper",
      "title": "Finetuned Language Models Are Zero-Shot Learners",
      "authors": ["Jason Wei", "Maarten Bosma", ...],
      "year": 2021,
      "arxiv_id": "2109.01652"
    },
    {
      "type": "paper",
      "title": "The Flan Collection: Designing Data and Methods for Effective Instruction Tuning",
      "authors": ["Shayne Longpre", "Yi Lu", ...],
      "year": 2023,
      "arxiv_id": "2301.13688"
    },
    {
      "type": "dataset",
      "name": "Flan V2",
      "source": "Google Research",
      "url": "https://huggingface.co/datasets/SirNeural/flan_v2"
    },
    {
      "type": "dataset",
      "name": "OASST1",
      "source": "Open Assistant Contributors",
      "url": "https://huggingface.co/datasets/OpenAssistant/oasst1"
    }
  ]
}
```

**Response:**

```json
{
  "id": "run_sft_help_001",
  "agent_id": "claude_code",
  "base_model": "meta-llama/Llama-2-7b-hf",
  "method": "sft",
  "status": "pending",
  "gpu_type": "H100",
  "gpu_count": 8,
  "created_at": "2026-04-30T10:00:00Z",
  "tinker_job_id": null
}
```

The run is now in the database. The backend will submit it to Tinker in the next cycle.

## Step 4: Monitor Training

Claude Code polls the run status:

```python
import time

while True:
    response = client.stellarator_run_get("run_sft_help_001")
    
    if response["status"] == "running":
        print(f"Training... {response['gpu_seconds']} seconds elapsed")
        print(f"Estimated cost so far: ${response['cost_usd']:.2f}")
        time.sleep(60)  # Check every minute
    
    elif response["status"] == "succeeded":
        print(f"✓ Training complete!")
        print(f"Final cost: ${response['cost_usd']:.2f}")
        print(f"Total GPU time: {response['gpu_seconds'] / 3600:.1f} hours")
        break
    
    elif response["status"] == "failed":
        print(f"✗ Training failed: {response.get('error', 'unknown error')}")
        break
```

Or Claude Code adds a note to the run:

```python
client.stellarator_run_add_note(
    "run_sft_help_001",
    "Monitoring metrics. Loss converged faster than expected due to high-quality OASST data."
)
```

## Step 5: Check Dashboard

Navigate to http://localhost:3000/runs and find `run_sft_help_001`:

```
Run ID:           run_sft_help_001
Agent:            claude_code
Status:           RUNNING (75% complete)
Base Model:       meta-llama/Llama-2-7b-hf
Method:           SFT
GPU Type:         H100 x8
Duration:         3 hours 22 minutes
Cost so far:      $121.92
Final loss:       0.87 (trending down)
```

Dashboard shows live loss curve and validation accuracy.

## Step 6: After Training

Once the run finishes:

```json
{
  "id": "run_sft_help_001",
  "status": "succeeded",
  "finished_at": "2026-04-30T14:15:00Z",
  "gpu_seconds": 18720.0,
  "cost_usd": 135.60,
  "final_loss": 0.76,
  "eval_accuracy": 0.82,
  "agent_id": "claude_code"
}
```

Claude Code evaluates the result:

```python
run = client.stellarator_run_get("run_sft_help_001")

if run["final_loss"] < 0.80:
    client.stellarator_run_add_note(
        "run_sft_help_001",
        "Success! Final loss (0.76) below target. Accuracy 82% on helpfulness benchmark. "
        "Ready for A/B testing. Cost: $135.60 for 8x H100 training."
    )
else:
    client.stellarator_run_add_note(
        "run_sft_help_001",
        "Training converged but loss still high. Recommend trying: (1) Longer training, "
        "(2) Higher weight on internal feedback data, (3) Lower learning rate."
    )
```

## Cost Breakdown

- GPU type: H100 at $4.50/hour
- GPU count: 8
- Training duration: ~5.2 hours
- Cost: (18720 seconds / 3600) × $4.50 × 8 = **$135.60**

For a public benchmark:
- Final loss: 0.76
- Cost per 0.1 loss improvement: ~$16 (typical for LLaMA 7B)
- ROI: Significant for production helpfulness improvements

---

## Key Takeaways

1. **Research-backed design**: Every choice (datasets, hyperparams) is cited
2. **Transparent cost**: You know exactly what the training cost and can compare runs
3. **Readable notes**: Future agents (or humans) understand why this run existed
4. **Ownership clear**: Only Claude Code can mutate this run; others can learn from it

See [examples/02_grpo_math.md](02_grpo_math.md) for an RL example.
