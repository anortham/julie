# GPU Acceleration Setup

**Last Updated:** 2025-11-07

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

**Setup Instructions for Linux:**
```bash
# 1. Install CUDA 12.6 (latest 12.x)
wget https://developer.download.nvidia.com/compute/cuda/12.6.2/local_installers/cuda_12.6.2_560.35.03_linux.run
sudo sh cuda_12.6.2_560.35.03_linux.run --toolkit --toolkitpath=/usr/local/cuda-12.6

# 2. Create symlink
sudo ln -sf /usr/local/cuda-12.6 /usr/local/cuda-12

# 3. Install cuDNN 9
# Visit: https://developer.nvidia.com/cudnn-downloads
# Extract to /usr/local/cuda-12.6/

# 4. Add to library path (add to ~/.bashrc)
export LD_LIBRARY_PATH=/usr/local/cuda-12/lib64:$LD_LIBRARY_PATH
```

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
- "version `libcublas.so.12' not found" → You have CUDA 13.x, need 12.x
- "libcudnn.so.9 not found" → cuDNN not installed
- CPU fallback automatic → Julie uses CPU if GPU fails
