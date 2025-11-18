# GPU Acceleration Setup

**Last Updated:** 2025-11-18

Platform-specific GPU acceleration setup for Julie.

## Platform-Specific Requirements

**Windows (DirectML):**
- ✅ **Works out of the box** with pre-built binaries
- Supports NVIDIA, AMD, and Intel GPUs
- No additional setup required

**Linux (CUDA):**
- ⚠️ **Requires CUDA 12.x + cuDNN 9**
- **CRITICAL**: CUDA 13.x is NOT compatible (symbol versioning differences)
- Pre-built ONNX Runtime binaries compiled against CUDA 12.x

---

## Linux Setup Instructions

### Option 1: WSL2 (Recommended for Windows Users)

**Prerequisites:**
- Windows 11 with WSL2 enabled
- NVIDIA GPU driver installed on Windows (R495 or later)

**Installation:**
```bash
# 1. Install CUDA Toolkit 12.6 for WSL2
wget https://developer.download.nvidia.com/compute/cuda/repos/wsl-ubuntu/x86_64/cuda-keyring_1.1-1_all.deb
sudo dpkg -i cuda-keyring_1.1-1_all.deb
sudo apt-get update
sudo apt-get install cuda-toolkit-12-6

# 2. Install cuDNN 9
sudo apt-get install libcudnn9-cuda-12 libcudnn9-dev-cuda-12

# 3. Configure shell environment (adjust for your shell)
# For bash: ~/.bashrc, for zsh: ~/.zshrc
echo 'export PATH=/usr/local/cuda-12.6/bin:$PATH' >> ~/.zshrc
echo 'export LD_LIBRARY_PATH=/usr/local/cuda-12.6/lib64:$LD_LIBRARY_PATH' >> ~/.zshrc
source ~/.zshrc

# 4. Verify installation
nvcc --version
nvidia-smi
```

### Option 2: Bare-Metal Linux

```bash
# 1. Install CUDA 12.6
wget https://developer.download.nvidia.com/compute/cuda/12.6.2/local_installers/cuda_12.6.2_560.35.03_linux.run
sudo sh cuda_12.6.2_560.35.03_linux.run --toolkit --toolkitpath=/usr/local/cuda-12.6

# 2. Create symlink
sudo ln -sf /usr/local/cuda-12.6 /usr/local/cuda-12

# 3. Install cuDNN 9
# Visit: https://developer.nvidia.com/cudnn-downloads
# Download and extract to /usr/local/cuda-12.6/

# 4. Configure shell environment
echo 'export PATH=/usr/local/cuda-12.6/bin:$PATH' >> ~/.bashrc
echo 'export LD_LIBRARY_PATH=/usr/local/cuda-12.6/lib64:$LD_LIBRARY_PATH' >> ~/.bashrc
source ~/.bashrc
```

---

## ⚠️ CRITICAL: MCP Server Configuration

**If running Julie as an MCP server (Claude Desktop, Cline, etc.)**, shell environment variables are **NOT** loaded. You **MUST** configure the environment in your MCP client config.

### Claude Desktop Configuration

Edit `~/.config/Claude/claude_desktop_config.json` (Linux) or `%APPDATA%\Claude\claude_desktop_config.json` (Windows):

**Production Configuration (Recommended):**
```json
{
  "mcpServers": {
    "julie": {
      "command": "/path/to/julie/target/release/julie-server",
      "env": {
        "LD_LIBRARY_PATH": "/path/to/julie/target/release:/usr/local/cuda-12.6/lib64"
      }
    }
  }
}
```

**Optional: Enable Debug Logging (if needed):**

If you need more detailed logs for troubleshooting (not normally required):
```json
{
  "env": {
    "RUST_LOG": "julie=debug",
    "LD_LIBRARY_PATH": "..."
  }
}
```

> ⚠️ **DO NOT USE `ort=trace`** - it generates 12GB+ logs per session and will fill your disk.
> - Default INFO-level logs are sufficient for troubleshooting
> - Use `julie=debug` only if you need more verbose Julie-specific logging
> - Check existing logs **first** before changing RUST_LOG

**IMPORTANT:** The `LD_LIBRARY_PATH` must include **BOTH**:
1. `/path/to/julie/target/release` - For ONNX Runtime shared libraries
2. `/usr/local/cuda-12.6/lib64` - For CUDA/cuDNN libraries

**Why both paths are required:**
- ONNX Runtime's CUDA provider (`libonnxruntime_providers_cuda.so`) depends on shared provider library (`libonnxruntime_providers_shared.so`)
- The `ort` crate's `copy-dylibs` feature creates symlinks in `target/release/` to libraries in `~/.cache/ort.pyke.io/`
- Without the release directory in `LD_LIBRARY_PATH`, you'll get: `Failed to load library libonnxruntime_providers_shared.so: cannot open shared object file`

**macOS (CPU-optimized):**
- CPU mode is **faster than CoreML** for BERT/transformer models
- CoreML only accelerates ~25% of operations
- No GPU setup needed

## Troubleshooting

**Check if GPU is being used:**
```bash
# Linux - watch GPU utilization
watch -n 0.5 nvidia-smi

# Check Julie logs
tail -f .julie/logs/julie.log.$(date +%Y-%m-%d) | grep -i "cuda\|gpu"
```

**Force CPU mode:**
```bash
export JULIE_FORCE_CPU=1
```

**Common Issues:**

### Error: `Failed to load library libonnxruntime_providers_shared.so`
**Symptom:** CUDA provider registration fails with:
```
Failed to load library libonnxruntime_providers_shared.so with error:
libonnxruntime_providers_shared.so: cannot open shared object file
```

**Solution:** Add `target/release` directory to `LD_LIBRARY_PATH` in your MCP config (see MCP Server Configuration section above).

**Why this happens:** ONNX Runtime shared libraries are symlinked in `target/release/` but the dynamic linker can't find them without the path being explicitly set.

### Error: GPU reports active but performance is slow (3-7 seconds per batch)
**Symptom:** Logs show `[GPU]` but batch processing takes 3-7 seconds (CPU performance) instead of 200-500ms (GPU performance).

**Diagnosis:**
```bash
# 1. Check default INFO-level logs (no RUST_LOG needed)
grep -E "ERROR|WARN|ort::" .julie/logs/julie.log.$(date +%Y-%m-%d) | tail -50

# 2. Watch GPU utilization - should spike during embedding batches
watch -n 0.5 nvidia-smi

# 3. Look for library loading errors around CUDA initialization
grep -A5 "CUDA libraries found" .julie/logs/julie.log.$(date +%Y-%m-%d) | tail -20
```

**Common cause:** ONNX Runtime silently falls back to CPU when `libonnxruntime_providers_shared.so` cannot be loaded. Check the `libonnxruntime_providers_shared.so` error section above.

### Error: `version 'libcublas.so.12' not found`
**Solution:** You have CUDA 13.x installed, need CUDA 12.x. Uninstall and install CUDA 12.6.

### Error: `libcudnn.so.9 not found`
**Solution:** cuDNN not installed. Run: `sudo apt-get install libcudnn9-cuda-12`

### Silent CPU Fallback
**Symptom:** No errors, but GPU not being used.

**Check:**
```bash
# Watch GPU utilization during embedding generation
watch -n 0.5 nvidia-smi

# Check Julie's CUDA detection
tail -f .julie/logs/julie.log.$(date +%Y-%m-%d) | grep -i "cuda\|gpu"
```

**Expected on success:**
- GPU memory usage increases (300-550 MiB for model)
- GPU utilization spikes during embedding batches
- Logs show: `Successfully registered CUDAExecutionProvider`
