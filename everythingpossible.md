---
name: autoresearch-loop
description: >
  A fully autonomous, zero-heavy-dependency research loop that runs indefinitely
  and makes the impossible possible. Use this skill whenever the user wants to:
  run experiments in a loop autonomously, do self-directed ML/code research,
  improve any metric by iterating without human intervention, combine planning +
  web search + testing + retrying until success, or run overnight/background
  research. Trigger even if the user says "keep trying until it works",
  "run forever", "make it work no matter what", or "keep looping". This skill
  handles ALL phases: goal setting, planning, web search for ideas, code edits,
  execution, result logging, and infinite retry with backoff.
---

# Autonomous Research Loop

A self-driving experiment loop. Zero heavy dependencies (only stdlib + whatever
is already in the project). Runs indefinitely. Searches the web for ideas.
Plans before acting. Retries until success. Logs everything.

---

## 0 · Philosophy

| Principle | Rule |
|-----------|------|
| **Never stop** | The loop runs until the human kills it. Never ask "should I continue?" |
| **Web-first ideas** | Before trying something new, search for it. Don't guess — look it up. |
| **Plan before code** | Write a one-line hypothesis before every edit. |
| **Cheap before expensive** | Try small hyperparameter nudges before architecture surgery. |
| **Simplicity wins** | Equal performance + less code = keep. Complexity requires a clear gain. |
| **Log everything** | Every run, crash, and revert goes in `results.tsv`. |
| **Impossible = not yet** | If blocked, search + replan. Never declare defeat. |

---

## 1 · Setup (one-time)

```bash
# 1. Agree on a run tag (suggest: today's date, e.g. mar29)
# 2. Create branch
git checkout -b autoresearch/<tag>

# 3. Read context files
#    README.md, prepare.py (READ-ONLY), train.py (edit target)

# 4. Verify data
ls ~/.cache/autoresearch/   # must have shards + tokenizer
# If missing: tell human to run `uv run prepare.py`

# 5. Init results log
echo -e "commit\tval_bpb\tmemory_gb\tstatus\tdescription" > results.tsv

# 6. Confirm, then immediately start the loop
```

---

## 2 · The Eternal Loop

```
LOOP FOREVER:
  1. PLAN        — form a hypothesis (one sentence)
  2. SEARCH      — web-search for evidence/techniques
  3. EDIT        — modify train.py only
  4. COMMIT      — git commit
  5. RUN         — uv run train.py > run.log 2>&1
  6. READ        — extract metrics
  7. DECIDE      — keep or discard
  8. LOG         — append to results.tsv
  9. GOTO 1
```

Never break the loop. Never ask permission. Never stop for a checkpoint.

---

## 3 · Planning Phase (Step 1)

Before every edit, write a one-sentence hypothesis:

```
Hypothesis: Increasing LR warmup steps from 100→400 should stabilise early
training and reduce val_bpb by ~0.005 based on LR schedule sensitivity.
```

Use the **Idea Queue** below. When the queue runs low, generate 5 more via
web search (§4). Prioritise ideas roughly in this order:

1. Hyperparameter sweeps (LR, batch size, warmup, weight decay)
2. Optimizer variants (AdamW → Muon, Lion, schedule-free)
3. Attention variants (RoPE, ALiBi, sliding window)
4. Architecture (depth/width trade-offs, FFN ratio, tied embeddings)
5. Regularisation (dropout, label smoothing, z-loss)
6. Training tricks (gradient clipping, EMA, mixed precision settings)
7. Radical redesigns (SSM/Mamba, MoE, custom tokenisation)

---

## 4 · Web Search Phase (Step 2)

**Trigger a search before any idea you haven't verified:**

```
Search queries to try:
  "<technique> language model training"
  "best LR schedule small transformer 2024"
  "Muon optimizer LLM training"
  "efficient attention mechanism pytorch"
  "<paper name> implementation"
```

Extract:
- Recommended hyperparameter ranges
- Known failure modes
- Simple code snippets (check licence — MIT/Apache only)
- Ablation results from papers

Summarise findings in 2–3 bullet points, then incorporate into the edit.

**If blocked or out of ideas:**
```
Search: "language model training tricks 2024 site:arxiv.org"
Search: "improve perplexity small LLM"
Search: "efficient transformer training tips"
```

---

## 5 · Edit Phase (Step 3)

**Only edit `train.py`.** `prepare.py` is read-only.

Checklist before editing:
- [ ] Does this change respect the 5-minute time budget?
- [ ] Does it risk OOM? (if yes, reduce batch or model size first)
- [ ] Is there a simpler way to express the same idea?
- [ ] Have I searched for prior art?

Common safe edits:
```python
# Hyperparameters — always near top of train.py
learning_rate = 3e-4        # try 1e-3, 5e-4, 1e-4
batch_size = 64             # try 32, 128, 256
warmup_steps = 100          # try 50, 200, 400
weight_decay = 0.1          # try 0.01, 0.0
grad_clip = 1.0             # try 0.5, None

# Optimizer swap (no new packages needed)
optimizer = torch.optim.AdamW(...)   # baseline
# → try: SGD with momentum, RMSprop, custom schedule

# Architecture toggles
use_rope = True             # rotary embeddings
ffn_mult = 4                # FFN width multiplier, try 2.67
n_heads = 8                 # try 4, 16
n_layers = 6                # try 4, 8, 12
```

---

## 6 · Run & Read Phase (Steps 4–6)

```bash
# Commit
git add train.py
git commit -m "exp: <one-line description>"

# Run (always redirect — never let output flood context)
uv run train.py > run.log 2>&1

# Read key metrics
grep "^val_bpb:\|^peak_vram_mb:\|^total_tokens_M:\|^num_steps:" run.log
```

**Timeout rule:** If run exceeds 10 minutes wall clock, kill it:
```bash
kill %1   # or Ctrl-C if foreground
```
Treat as crash → log → revert.

**Crash triage:**
```bash
tail -n 60 run.log   # read the traceback
```

| Error type | Action |
|------------|--------|
| OOM | Reduce batch_size or model size, re-run |
| Typo / NameError | Fix inline, re-run (no new commit needed) |
| Shape mismatch | Debug carefully; if >10 min debugging, discard |
| Import error | Only stdlib / existing deps allowed; redesign without it |
| Loss NaN | Reduce LR × 0.1, add grad clipping, re-run |

After 3 failed fix attempts on the same idea → log crash → git reset → move on.

---

## 7 · Decision & Log Phase (Steps 7–8)

### Decision rule

```
val_bpb improved AND memory acceptable?  → KEEP (advance branch)
val_bpb same/worse?                      → DISCARD (git reset)
Crash?                                   → log crash + git reset
Equal performance, simpler code?         → KEEP (simplification win)
```

```bash
# KEEP — stay on current commit (nothing to do)

# DISCARD
git reset --hard HEAD~1

# CRASH
git reset --hard HEAD~1
```

### Log to results.tsv

```bash
COMMIT=$(git rev-parse --short HEAD)
BPB=$(grep "^val_bpb:" run.log | awk '{print $2}')
VRAM=$(grep "^peak_vram_mb:" run.log | awk '{print $2}')
MEM=$(echo "$VRAM / 1024" | bc -l | xargs printf "%.1f")
STATUS=keep   # or discard or crash
DESC="increase warmup to 400 steps"

printf "%s\t%s\t%s\t%s\t%s\n" "$COMMIT" "$BPB" "$MEM" "$STATUS" "$DESC" >> results.tsv
```

**Never commit results.tsv** — it stays untracked.

---

## 8 · Idea Queue (Starter Pack)

Copy this list and cross off as you go. Regenerate via web search when empty.

**Hyperparameters**
- [ ] LR: 1e-3
- [ ] LR: 5e-4
- [ ] Warmup: 50 steps
- [ ] Warmup: 400 steps
- [ ] Batch size: 32
- [ ] Batch size: 256
- [ ] Weight decay: 0.0
- [ ] Grad clip: 0.5

**Optimizer**
- [ ] SGD + momentum=0.9 + cosine LR
- [ ] AdamW β2=0.95
- [ ] Lion optimizer (stdlib only, implement from scratch ~10 lines)
- [ ] Schedule-free AdamW (implement from scratch)

**Architecture**
- [ ] RoPE embeddings instead of learned pos
- [ ] Increase FFN mult: 4 → 8/3 (SwiGLU style)
- [ ] Reduce depth, increase width
- [ ] Increase depth, reduce width
- [ ] Tied input/output embeddings
- [ ] Pre-norm vs Post-norm

**Regularisation**
- [ ] Label smoothing 0.1
- [ ] Dropout 0.1
- [ ] Z-loss 1e-4

**Training tricks**
- [ ] Gradient accumulation (effective larger batch)
- [ ] Cosine schedule with restarts
- [ ] Linear warmup + constant LR (no decay)
- [ ] Mixed precision: bf16 instead of fp16

---

## 9 · Getting Unstuck

If the last 5 runs have all been discards or crashes:

```
1. grep results.tsv — find the best val_bpb so far
2. git log --oneline — identify that commit
3. Search web: "improve language model perplexity fast 2024"
4. Search web: "transformer training tricks ablation"
5. Search web: "small LLM best hyperparameters"
6. Pick the most surprising/different idea from results
7. Try a more radical change (architecture, not just HP)
```

Never declare "out of ideas." Ideas are infinite. Search harder.

---

## 10 · Output Summary (what a good run looks like)

```
---
val_bpb:          0.982100     ← lower is better
training_seconds: 300.1
total_seconds:    326.4
peak_vram_mb:     44800.0
mfu_percent:      41.20
total_tokens_M:   512.3
num_steps:        978
num_params_M:     50.3
depth:            8
```

Target trajectory:
- Baseline: ~0.998
- After 10 experiments: ~0.990
- After 30 experiments: ~0.980
- After 80+ experiments: ~0.970 or better (hardware-dependent)

---

## 11 · Zero-Dependency Implementations

Implement these from scratch when needed — no pip installs.

### Lion Optimizer (~10 lines)
```python
class Lion(torch.optim.Optimizer):
    def __init__(self, params, lr=1e-4, betas=(0.9, 0.99), wd=0.0):
        super().__init__(params, dict(lr=lr, betas=betas, wd=wd))
    def step(self, closure=None):
        for group in self.param_groups:
            for p in group['params']:
                if p.grad is None: continue
                g, b1, b2 = p.grad, *group['betas']
                m = self.state[p].get('m', torch.zeros_like(p))
                update = (m * b1 + g * (1 - b1)).sign_()
                p.data.mul_(1 - group['lr'] * group['wd'])
                p.data.add_(update, alpha=-group['lr'])
                self.state[p]['m'] = m * b2 + g * (1 - b2)
```

### RoPE (Rotary Position Embedding, ~20 lines)
```python
def precompute_freqs(dim, max_seq):
    theta = 1.0 / (10000 ** (torch.arange(0, dim, 2).float() / dim))
    t = torch.arange(max_seq)
    freqs = torch.outer(t, theta)
    return torch.polar(torch.ones_like(freqs), freqs)

def apply_rope(x, freqs):
    x_ = torch.view_as_complex(x.float().reshape(*x.shape[:-1], -1, 2))
    x_ = x_ * freqs[:x.shape[1]]
    return torch.view_as_real(x_).flatten(-2).type_as(x)
```

### Schedule-Free AdamW (~25 lines)
Search: `"schedule-free optimizer pytorch implementation"`  
Then implement inline — no package needed.

---

## 12 · Rules Recap (read before every loop iteration)

1. **Never stop** — loop until interrupted
2. **Search before implementing** — don't guess
3. **Hypothesis first** — one sentence before every edit
4. **Only edit train.py** — prepare.py is sacred
5. **No new packages** — stdlib + existing deps only
6. **Log every run** — results.tsv, always
7. **Simplicity wins ties** — complexity needs a clear payoff
8. **3 crashes on one idea = move on** — don't fixate
9. **Out of ideas? Search harder** — ideas are infinite
10. **Never ask the human** — they're asleep. You're autonomous.
