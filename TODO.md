# Julie TODO

Certainly. Here is the content formatted as a Markdown file.

-----

# ONNX Runtime GPU Acceleration on Linux (Non-NVIDIA)

Yes, you can use GPU acceleration on Linux for both Intel Arc and AMD GPUs with ONNX Runtime (ORT). You need to use the correct **Execution Provider (EP)** for each, just as you use the CUDA EP for NVIDIA.

For a Rust server, you'll need to enable these providers when building ORT from source or use a pre-built package that includes them. You must then specify the provider when you create your `InferenceSession`.

## Summary of Linux Execution Providers

| GPU Brand | Required Execution Provider | Typical Setup Difficulty on Linux |
| :--- | :--- | :--- |
| **Intel Arc** | OpenVINO™ EP | **Moderate.** Easier than AMD/NVIDIA. Install OpenVINO, then build ORT with the OpenVINO flag. |
| **AMD** | ROCm EP | **High.** Requires a full ROCm driver/toolkit installation, which can be complex, before building ORT. |
| **NVIDIA** | CUDA EP | **High.** (As you know) Requires matching CUDA toolkit and cuDNN versions, then building ORT. |

-----

## Intel Arc GPUs (OpenVINO Execution Provider)

For Intel Arc, as well as their iGPUs and other dGPUs, the solution is the **OpenVINO™ Execution Provider**.

  * **What it is:** OpenVINO is Intel's toolkit for optimizing and deploying AI inference. The OpenVINO EP allows ORT to hand off the computation to your Arc GPU.

  * **How to get it:**

      * **Recommended (Python):** The easiest way to test is often via the Python package: `pip install onnxruntime-openvino`. This package bundles the necessary OpenVINO libraries.
      * **For Rust (Building from Source):** When building ORT from source to use in your Rust project, you will need to enable the OpenVINO EP during the CMake configuration. This typically involves passing a flag like `--use_openvino`. You will also need to have the OpenVINO toolkit installed on your system so the build process can find its headers and libraries.

  * **In your Code:** When creating your session, you'll need to tell ORT to use the OpenVINO provider and target the GPU.

      * In Rust (using the `ort` crate), it would look something like this, making sure to specify the GPU device type:

    <!-- end list -->

    ```rust
    // Example conceptual code for Rust
    use ort::{Environment, Session, tensor::OrtOwnedTensor};

    let environment = Environment::builder().build()?;

    let session = environment
        .builder()?
        .with_optimization_level(ort::GraphOptimizationLevel::Level3)?
        // Tell ORT to use the OpenVINO provider
        .with_execution_providers([
            ort::ExecutionProvider::openvino()
                .with_device_type("GPU") // Crucial step! "CPU" is often the default
                .build(),
        ])?
        .with_model_from_file("your_model.onnx")?;

    // ... proceed with your inference ...
    ```

-----

## AMD GPUs (ROCm Execution Provider)

For AMD GPUs, acceleration on Linux is provided by the **ROCm Execution Provider**. (You may also see references to the **MIGraphX EP**, which is built on top of ROCm).

  * **What it is:** ROCm is AMD's open-source software platform for GPU computing, analogous to NVIDIA's CUDA.

  * **How to get it:** This is generally more involved, as you've found with CUDA.

    1.  **Install ROCm:** You must first install the AMD ROCm drivers and libraries on your Linux system. This process is highly dependent on your distribution and GPU model. This is the most complex step.
    2.  **Get the ORT Package:**
          * **Python:** There is a PyPI package: `pip install onnxruntime-rocm`.
          * **For Rust (Building from Source):** This is the more likely path for your server. You will need to build ORT from source, passing the `--use_rocm` flag to CMake. The build script must be able to locate your ROCm installation.

  * **In your Code:** Similar to the others, you explicitly request the ROCm provider.

      * In Rust, this would be:

    <!-- end list -->

    ```rust
    // Example conceptual code for Rust
    use ort::{Environment, Session};

    let environment = Environment::builder().build()?;

    let session = environment
        .builder()?
        .with_optimization_level(ort::GraphOptimizationLevel::Level3)?
        // Tell ORT to use the ROCm provider
        .with_execution_providers([
            ort::ExecutionProvider::rocm().build(),
        ])?
        .with_model_from_file("your_model.onnx")?;

    // ... proceed with your inference ...
    ```
---

**Last Updated:** 2025-10-28 (Evening)
**Status:** All FTS5 issues FIXED ✅, tests passing (1177/1179), production validated, monitoring phase active 🔬