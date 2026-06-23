# Voice latency: is a Rust + ONNX rewrite worth it?

**Question (user):** "Should we change the voice server to Rust so it's faster? I
think ONNX is already ready."

**Short answer: No — not for latency.** The per-turn cost is the two 1.7B model
inferences, not the Python around them. A Rust host runs the *same* math. And the
Qwen3 ASR/TTS models are **not ONNX-exportable today** without significant surgery.
The real fix is **GPU**.

## What "ONNX is ready" actually refers to

The repo's ONNX usage is `regent-embed` (ADR-013): the **384-dim all-MiniLM**
sentence embedder via `fastembed`/`ort`. That's a small, standard, export-friendly
model. It says nothing about the 1.7B speech models.

## Evidence — the Qwen3 speech models can't be exported cheaply

Probed the installed packages (`qwen_asr`, `qwen_tts`, `optimum`, `onnxruntime`):

- **Not `transformers.PreTrainedModel`.** Both `Qwen3ASRModel` and `Qwen3TTSModel`
  have MRO `… <- builtins.object` — they're **custom inference wrappers**
  (`qwen_asr/inference/qwen3_asr.py`, `qwen_tts/inference/qwen3_tts_model.py`), not
  standard HF architectures.
- **`optimum` can't export them.** `optimum.exporters.onnx` only handles *registered
  transformers architectures*; custom wrappers aren't in the registry (and the
  exporter submodule wasn't even importable in this env).
- **Custom, multi-stage pipelines.** TTS exposes a bespoke `generate_custom_voice`
  (custom autoregressive decode + an audio codec/vocoder stage); ASR has its own
  feature-extraction + decode path. Both lean on **SoX/torchaudio** audio I/O.

To export this to ONNX you would have to, by hand: split each model into
encoder / decoder-LM / vocoder subgraphs; write `torch.onnx.export` with correct
dynamic axes per subgraph; reimplement the autoregressive loop + KV-cache + sampling
in host code (ONNX is a single forward step); reimplement audio feature extraction +
the codec; and fix any ops that don't export (custom attention/RoPE, the codec). That
is research-grade reverse-engineering of two 1.7B pipelines.

## Even if exported, the win is small on the language axis

ONNX Runtime CPU ≈ PyTorch CPU for fp32. The real speedup is **int8 quantization** —
and that's available **in PyTorch** (`torch.ao.quantization`, bitsandbytes) with **no
export**. So the quantization win is reachable without ONNX or Rust.

## Recommendation (priority order)

1. **GPU.** CUDA torch build for the RTX → 5–30×. Already auto-detected; see the
   server README. This is the fix.
2. **CPU, no rewrite:** background warm-up (shipped), int8 quantization in PyTorch,
   short replies, sentence-streamed TTS.
3. **ONNX/Rust:** revisit **only if** upstream ships ONNX-exportable checkpoints (or a
   GGUF/llama.cpp-style runtime appears for these models). Until then it's high effort,
   ~zero latency payoff over GPU.
