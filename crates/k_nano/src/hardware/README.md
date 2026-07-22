# Hardware Auto-Detection and Cognitive Adaptation

## Overview

This subsystem implements hardware-aware AI for Intel Xeon, AMD EPYC, AMD Ryzen, and Intel Client processors in the Neural-OS-Core ecosystem. The system reads silicon topology during boot and uses AI (hermes + k-ai) to recalibrate the kernel, maximizing core allocation, L3 cache utilization, and SIMD instruction throughput.

## Architecture

### 1. Hardware Discovery (k-nano::hardware::xeon)

Low-level CPUID and ACPI (SRAT/SLIT) parsing for Intel Xeon processors.

**Key Components:**
- `XeonTopologyReport`: Complete hardware topology report
- `CpuFlags`: CPU instruction flags (AVX2, AVX-512, AMX, BMI2)
- `CacheInfo`: Cache hierarchy per socket (L1, L2, L3)
- `SocketInfo`: Physical socket information with NUMA nodes
- `NumaNode`: NUMA node topology with memory affinity

**Detection Functions:**
- `discover_xeon_topology()`: Main discovery entry point
- `detect_cpu_flags()`: CPUID-based instruction detection
- `detect_cache_topology()`: CPUID leaf 4 cache parsing
- `detect_topology()`: CPUID leaf 0xB topology enumeration
- `classify_xeon_generation()`: Classify as Old/Modern/Latest Xeon

### 2. Cognitive Adaptation (hermes::adaptation)

Hermes (Meta-Cognitive Supervisor) receives hardware topology and generates execution policies.

**Key Components:**
- `ExecutionStrategy`: OldXeon, ModernXeon, Fallback
- `AdaptationPolicy`: Complete policy with socket isolation, core pinning, SIMD dispatch, MoE sizing
- `SocketIsolationPolicy`: NUMA socket isolation for dual-socket systems
- `CorePinningPolicy`: Thread-to-core assignment with cognitive cells
- `SimdDispatchPolicy`: SIMD kernel selection (AVX2/AVX-512/AMX)
- `MoESizingPolicy`: Mixture of Experts sizing to fit in L3 cache

**Adaptation Functions:**
- `cognitive_adaptation()`: Main adaptation entry point
- `generate_strategy()`: Select execution strategy based on hardware
- `adapt_to_hardware()`: Generate complete adaptation policy

### 3. SIMD Kernels (k-ai::arch::x86_64)

Dynamic dispatching for AVX2 and AVX-512 kernels with 64-byte alignment.

**Key Components:**
- `bitwise_add_avx2()`: AVX2 kernel (256-bit, 128 weights/cycle)
- `bitwise_add_avx512()`: AVX-512 kernel (512-bit, 256 weights/cycle)
- `bitwise_add_scalar()`: Scalar fallback
- `AlignedBuffer<T>`: 64-byte aligned data structures

**Dispatch Functions:**
- `dispatch_bitnet_kernel()`: Runtime hardware detection
- `dispatch_bitnet_kernel_with_policy()`: Policy-based dispatch
- `safe_bitwise_add()`: Safe wrapper with bounds checking

### 4. Boot Integration (k-nano::hardware::boot_integration)

Unified boot flow integrating all components.

**Key Components:**
- `BootHardwareState`: Boot-time hardware state tracking
- `boot_hardware_discovery()`: Early boot hardware discovery
- `finalize_boot_integration()`: Complete boot integration

## Boot Flow

```
1. Early Boot (k-nano)
   └─> boot_hardware_discovery()
       └─> discover_xeon_topology()
           ├─> detect_cpu_flags() [CPUID]
           ├─> detect_cache_topology() [CPUID leaf 4]
           ├─> detect_topology() [CPUID leaf 0xB]
           └─> parse_srat() / parse_slit() [ACPI]

2. Mid Boot (hermes)
   └─> cognitive_adaptation(topology_report)
       └─> adapt_to_hardware()
           ├─> generate_strategy()
           ├─> generate_socket_isolation_policy()
           ├─> generate_core_pinning_policy()
           ├─> generate_simd_dispatch_policy()
           └─> generate_moe_sizing_policy()

3. Late Boot (k-ai)
   └─> dispatch_bitnet_kernel_with_policy()
       ├─> bitwise_add_avx512() [if AVX-512]
       ├─> bitwise_add_avx2() [if AVX2]
       └─> bitwise_add_scalar() [fallback]

4. Runtime
   └─> AI operations use hardware-optimized kernels
       └─> NUMA-aware memory allocation
       └─> Core-pinned cognitive cells
```

## Execution Strategies

### Old Xeon (E5 v3/v4 - Haswell/Broadwell)

**Characteristics:**
- AVX2 (256-bit SIMD)
- Octa-Channel DDR4
- QPI interconnect
- Limited L3 cache (~45MB per socket)

**Policies:**
- **Socket Isolation**: Strict isolation between sockets
  - Socket 0: hermes, jarbas, VFS (Memory L0-L7)
  - Socket 1: cortex, k-ai
  - Limited inter-socket traffic via async MPMC messages
- **Core Pinning**: Core pinning with spin-loop for critical threads
- **MoE Sizing**: 4 experts per socket, 80% L3 utilization (~35MB per expert)
- **SIMD Dispatch**: AVX2 kernels (128 weights/cycle)

### Modern Xeon (Sapphire/Emerald/Granite Rapids)

**Characteristics:**
- AVX-512 (512-bit SIMD)
- AMX (Advanced Matrix Extensions) on latest
- DDR4/DDR5
- UPI interconnect
- Large L3 cache (60MB+ per socket)
- High core count (28-56 cores per socket)

**Policies:**
- **Socket Isolation**: Relaxed isolation (UPI is faster than QPI)
  - Socket 0: hermes, jarbas, VFS
  - Socket 1: cortex, k-ai
  - Higher inter-socket traffic limit
- **Core Pinning**: Massive cellular distribution
  - Cognitive Cells A.2: Dedicated threads per core
  - Core pinning without spin-loop (better scheduling)
- **MoE Sizing**: 4-16 experts per socket, 75% L3 utilization
- **SIMD Dispatch**: AVX-512/AMX kernels (256 weights/cycle)

## Usage Example

```rust
// In early boot (k-nano)
use k_nano::hardware::boot_integration;

// Discover hardware topology
let topology = boot_integration::boot_hardware_discovery();

// In mid boot (hermes)
use hermes::adaptation;

// Generate adaptation policy
let policy = adaptation::cognitive_adaptation(&topology);

// Apply policies
match policy.strategy {
    ExecutionStrategy::OldXeon => {
        // Apply socket isolation
        // Configure core pinning
        // Set MoE expert sizes
    }
    ExecutionStrategy::ModernXeon => {
        // Apply relaxed NUMA policy
        // Configure massive cellular distribution
        // Set larger MoE expert sizes
    }
    _ => {
        // Apply fallback policies
    }
}

// In late boot (k-ai)
use k_ai::arch::x86_64;

// Select SIMD kernel based on policy
let kernel = x86_64::dispatch_bitnet_kernel_with_policy(
    policy.simd_dispatch.use_avx512,
    policy.simd_dispatch.use_avx2,
);

// Use kernel for AI operations
unsafe {
    kernel(a_ptr, b_ptr, output_ptr, len);
}
```

## Performance Characteristics

### Old Xeon (AVX2)
- **SIMD Width**: 256 bits
- **Weights/Cycle**: 128
- **Interconnect**: QPI (limited bandwidth)
- **Cache**: ~45MB L3 per socket
- **Optimization**: In-cache execution, socket isolation

### Modern Xeon (AVX-512)
- **SIMD Width**: 512 bits
- **Weights/Cycle**: 256
- **Interconnect**: UPI (high bandwidth)
- **Cache**: 60MB+ L3 per socket
- **Optimization**: Massive parallelism, cellular distribution

### Latest Xeon (AMX)
- **SIMD Width**: 512 bits (tiles)
- **Weights/Cycle**: 512
- **Interconnect**: UPI
- **Cache**: 60MB+ L3 per socket
- **Optimization**: Matrix extensions, maximum throughput

## Compatibility

**Supported Hardware:**
- Intel Xeon E5 v3/v4 (Haswell/Broadwell)
- Intel Xeon Scalable 1st/2nd Gen (Skylake/Cascade Lake)
- Intel Xeon Scalable 3rd/4th Gen (Ice Lake/Sapphire Rapids)
- Intel Xeon Scalable 5th Gen (Emerald Rapids)
- Intel Xeon 6 (Granite Rapids)

**Fallback:**
- Systems without AVX2 use scalar fallback
- Non-Xeon systems use fallback policies
- Virtual machines with limited CPUID exposure

## Memory Alignment

All SIMD operations use 64-byte aligned data structures to match x86 cache line size:

```rust
#[repr(align(64))]
pub struct AlignedBuffer<T> {
    pub data: [T; 16], // 64 bytes for T = i32
}
```

This ensures:
- No cache line splitting
- Optimal memory bandwidth
- Zero-load penalty for aligned loads

## NUMA Awareness

The system is NUMA-aware for dual-socket configurations:

**Old Xeon (QPI):**
- Strict socket isolation
- Limited inter-socket traffic
- Memory allocated locally to socket

**Modern Xeon (UPI):**
- Relaxed socket isolation
- Higher inter-socket traffic allowed
- UPI provides sufficient bandwidth

## AMD EPYC Server Support

### Hardware Discovery (k-nano::hardware::epyc)

Low-level CPUID and ACPI (SRAT/SLIT) parsing for AMD EPYC server processors.

**Key Components:**
- `EpycTopologyReport`: Complete EPYC hardware topology report
- `EpycGeneration`: EPYC generation classification (Naples/Rome/Milan/Genoa/Bergamo/Turin)
- `EpycSocketInfo`: Socket information with CCDs and memory channels
- `EpycCcdInfo`: CCD (Core Complex Die) information with 3D V-Cache detection
- `EpycNumaNode`: NUMA node topology for complex multi-CCD layouts
- `EpycCpuFlags`: CPU instruction flags (AVX2, BMI2, CLZERO, RDPRU)

**Detection Functions:**
- `discover_epyc_topology()`: Main EPYC discovery entry point
- `is_epyc()`: Detect if CPU is AMD EPYC via CPUID
- `classify_epyc_generation()`: Classify EPYC generation (Zen 1-5)
- `detect_epyc_ccd_topology()`: Detect CCD count and 3D V-Cache
- `detect_epyc_socket_count()`: Detect multi-socket configuration
- `epyc_has_3d_vcache()`: Check for 3D V-Cache (Genoa-X/Bergamo-X)

### Cognitive Adaptation (hermes::adaptation::epyc)

Hermes generates execution policies for EPYC server processors.

**Key Components:**
- `EpycExecutionStrategy`: Naples/Rome/Milan/Genoa/Bergamo/GenoaX/BergamoX/Turin
- `EpycAdaptationPolicy`: Complete policy with NUMA, CCD, SIMD, MoE, memory
- `EpycNumaPolicy`: NUMA-aware allocation for complex multi-CCD topology
- `EpycCcdPolicy`: CCD allocation with 3D V-Cache priority
- `EpycSimdDispatchPolicy`: AVX2 dispatch (EPYC does not support AVX-512)
- `EpycMoeSizingPolicy`: MoE sizing with 3D V-Cache optimization
- `EpycMemoryPolicy`: DDR4/DDR5 memory channel optimization

**Adaptation Functions:**
- `epyc_cognitive_adaptation()`: Main EPYC adaptation entry point
- `generate_epyc_strategy()`: Select EPYC execution strategy
- `adapt_to_epyc_hardware()`: Generate complete EPYC adaptation policy

### Boot Integration (k-nano::hardware::epyc_boot)

Unified boot flow for EPYC systems.

**Key Components:**
- `EpycBootState`: EPYC boot-time hardware state tracking
- `boot_epyc_hardware_discovery()`: Early boot EPYC discovery
- `finalize_epyc_boot_integration()`: Complete EPYC boot integration

### EPYC Execution Strategies

#### EPYC Naples (Zen 1)
- **Characteristics**: DDR4, AVX2, 32 cores max
- **Policies**: NUMA-aware allocation, AVX2 kernels, baseline performance

#### EPYC Rome (Zen 2)
- **Characteristics**: DDR4, AVX2, 64 cores max, improved NUMA
- **Policies**: NUMA optimization, AVX2 kernels, improved throughput

#### EPYC Milan (Zen 3)
- **Characteristics**: DDR4, AVX2, 64 cores max, better latency
- **Policies**: NUMA-aware, AVX2 kernels, latency optimization

#### EPYC Genoa (Zen 4)
- **Characteristics**: DDR5, AVX2, 96 cores max, high core count
- **Policies**: Massive parallelism, DDR5 optimization, NUMA distribution

#### EPYC Bergamo (Zen 4c)
- **Characteristics**: DDR5, AVX2, 128 cores max, dense cores
- **Policies**: Maximum throughput, all CCDs utilization, dense core optimization

#### EPYC Genoa-X/Bergamo-X (Zen 4 + 3D V-Cache)
- **Characteristics**: DDR5, AVX2, 3D V-Cache (96MB+ per CCD)
- **Policies**: 3D V-Cache CCD priority, in-cache execution, V-Cache optimization

#### EPYC Turin (Zen 5)
- **Characteristics**: DDR5, AVX2, next-generation
- **Policies**: Maximum performance, next-gen optimizations

### EPYC Performance Characteristics

#### Zen 1/2/3 (DDR4)
- **SIMD Width**: 256 bits (AVX2)
- **Weights/Cycle**: 128
- **Memory**: DDR4, 8 channels
- **Cache**: 32MB L3 per CCD
- **Optimization**: NUMA-aware allocation

#### Zen 4 (DDR5)
- **SIMD Width**: 256 bits (AVX2)
- **Weights/Cycle**: 128
- **Memory**: DDR5, 8 channels
- **Cache**: 32MB L3 per CCD
- **Optimization**: High core count, massive parallelism

#### Zen 4 + 3D V-Cache
- **SIMD Width**: 256 bits (AVX2)
- **Weights/Cycle**: 128
- **Memory**: DDR5, 8 channels
- **Cache**: 96MB+ L3 per CCD (3D V-Cache)
- **Optimization**: In-cache execution, V-Cache bandwidth (~4 TB/s)

### EPYC Compatibility

**Supported Hardware:**
- EPYC 7001 (Naples) - Zen 1
- EPYC 7002 (Rome) - Zen 2
- EPYC 7003 (Milan) - Zen 3
- EPYC 7004 (Genoa/Bergamo) - Zen 4/Zen 4c
- EPYC 9004 (Genoa-X/Bergamo-X) - Zen 4 + 3D V-Cache
- EPYC 8004 (Turin) - Zen 5

**Fallback:**
- Systems without AVX2 use SSE4.2 or scalar fallback
- Non-EPYC AMD systems use client adaptation
- Virtual machines with limited CPUID exposure

## AMD Ryzen Client Support

### Hardware Discovery (k-nano::hardware::topology)

Low-level CPUID and ACPI (MADT/PPTT) parsing for AMD Ryzen and Intel Client processors.

**Key Components:**
- `ClientTopologyReport`: Complete client hardware topology report
- `ClientGeneration`: AMD 3D V-Cache, AI Max, Intel Hybrid, Legacy
- `IntelHybridInfo`: P-Core/E-Core/LPE-Core detection
- `AmdCcdInfo`: AMD CCD topology with 3D V-Cache detection
- `MemoryInfo`: Memory bus type and bandwidth

**Detection Functions:**
- `discover_client_topology()`: Main client discovery entry point
- `detect_intel_hybrid()`: Intel Hybrid detection via CPUID 0x1A
- `detect_amd_ccd()`: AMD asymmetric CCD detection via CPUID 0x8000001D
- `detect_memory_topology()`: Memory topology detection

### Cognitive Adaptation (hermes::adaptation::client)

Hermes generates execution policies for client processors.

**Key Components:**
- `ClientExecutionStrategy`: AMD 3D V-Cache, AI Max, Intel Hybrid, Legacy
- `ClientAdaptationPolicy`: Complete policy with core pinning, SIMD, MoE, memory
- `ClientCorePinningPolicy`: P-Core/E-Core/CCD assignment
- `ClientSimdDispatchPolicy`: AVX-512/AVX2/SSE4.2 selection
- `ClientMoeSizingPolicy`: In-cache sizing for 3D V-Cache

### Client Execution Strategies

#### AMD Ryzen 3D V-Cache
- **Policies**: Pin AI to 3D V-Cache CCD, in-cache execution, Hermes on standard CCD

#### AMD Ryzen AI / AI Max
- **Policies**: Ultra-throughput mode, unified memory, massive model loading

#### Intel Hybrid Modern
- **Policies**: P-Cores for AI, E-Cores for supervision, LPE-Cores for I/O

#### Intel Hybrid Legacy
- **Policies**: P-Cores for AI, E-Cores for supervision

#### Legacy CPUs
- **Policies**: Micro-kernel, extreme compression, reduced SIMD block size

## SIMD Kernel Dispatch

### Supported SIMD Kernels (k-ai::arch::x86_64)

**AVX-512 (512-bit):**
- `bitwise_add_avx512()`: 256 weights/cycle
- Target features: +avx512f,+avx512bw,+avx512vnni
- Uses `_mm512_and_si512` for ternary weight unpacking
- 64-byte aligned memory

**AVX2 (256-bit):**
- `bitwise_add_avx2()`: 128 weights/cycle
- Target feature: +avx2
- Used by AMD EPYC, Ryzen, Intel client
- 64-byte aligned memory

**SSE4.2 (128-bit):**
- `bitwise_add_sse42()`: 64 weights/cycle
- Target feature: +sse4.2
- Fallback for legacy i3/i5 CPUs
- 64-byte aligned memory

**Scalar:**
- `bitwise_add_scalar()`: Fallback for very old CPUs

### Dispatch Priority

1. Full AVX-512 (F + BW + VNNI) - Modern Xeon/Core Ultra
2. AVX2 - Ryzen/Intel client/EPYC
3. SSE4.2 - Legacy i3/i5 (minimum requirement)
4. Scalar - Fallback for very old CPUs

## Future Enhancements

- [ ] Full ACPI SRAT/SLIT parsing (currently stubbed)
- [ ] Intel Thread Director (ITD) integration
- [ ] Dynamic policy adjustment based on workload
- [ ] Power-aware adaptation
- [ ] Thermal-aware core allocation
