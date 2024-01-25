# reed-solomon-simd

Reed-Solomon erasure coding, featuring:

- Up to 65535 original shards or 65535 recovery shards.
- `O(n log n)` complexity.
- Runtime selection of best SIMD implementation on both AArch64 (Neon) and x86(-64) (SSSE3 and AVX2) with
fallback to plain Rust.
- Entirely written in Rust.

## Quick introduction to **original shards** and **recovery shards**

- The data which is going to be protected by Reed-Solomon erasure coding
  is split into equal-sized **original shards**.
    - **`original_count`** is the number of original shards.
- Additional **recovery shards** of same size are then created
  which contain recovery data so that original data can be fully restored
  from any set of **`original_count`** shards, original or recovery.
    - **`recovery_count`** is the number of recovery shards.

Algorithm supports **any combination of 1 - 32768 original shards with 1 - 32768 recovery shards**.
Up to 65535 original or recovery shards is also possible with following limitations:

| `original_count` | `recovery_count` |
| ---------------- | ---------------- |
| `<= 2^16 - 2^n`  | `<= 2^n`         |
| `<= 61440`       | `<= 4096`        |
| `<= 57344`       | `<= 8192`        |
| `<= 49152`       | `<= 16384`       |
| **`<= 32768`**   | **`<= 32768`**   |
| `<= 16384`       | `<= 49152`       |
| `<= 8192`        | `<= 57344`       |
| `<= 4096`        | `<= 61440`       |
| `<= 2^n`         | `<= 2^16 - 2^n`  |

## Benchmarks

| Original : Recovery | Encode       | Decode (1% loss; 100% loss) |
| ------------------- | ------------ | --------------------------- |
| 32: 32              | 8.1620 GiB/s | 142.15 MiB/s ; 140.92 MiB/s |
| 64: 64              | 7.1111 GiB/s | 269.38 MiB/s ; 264.23 MiB/s |
| 128 : 128           | 6.0587 GiB/s | 464.81 MiB/s ; 461.00 MiB/s |
| 256 : 256           | 4.8129 GiB/s | 722.03 MiB/s ; 713.58 MiB/s |
| 512 : 512           | 4.4636 GiB/s | 933.61 MiB/s ; 951.46 MiB/s |
| 1024 : 1024         | 4.0020 GiB/s | 1.0406 GiB/s ; 1.0822 GiB/s |
| 2048 : 2048         | 3.6136 GiB/s | 1.0911 GiB/s ; 1.0717 GiB/s |
| 4096 : 4096         | 3.3064 GiB/s | 1021.3 MiB/s ; 1.0121 GiB/s |
| 8192 : 8192         | 2.4810 GiB/s | 826.11 MiB/s ; 848.25 MiB/s |
| 16384 : 16384       | 1.9838 GiB/s | 638.29 MiB/s ; 629.38 MiB/s |
| 32 768 : 32 768     | 1.5097 GiB/s | 633.63 MiB/s ; 620.33 MiB/s |
| 128 : 1 024         | 5.2813 GiB/s | 653.76 MiB/s ; 654.03 MiB/s |
| 1 000 : 100         | 4.5012 GiB/s | 752.63 MiB/s ; 749.68 MiB/s |
| 1 000 : 10 000      | 3.3454 GiB/s | 789.79 MiB/s ; 793.63 MiB/s |
| 8 192 : 57 344      | 2.0460 GiB/s | 659.14 MiB/s ; 646.76 MiB/s |
| 10 000 : 1 000      | 2.4339 GiB/s | 718.68 MiB/s ; 736.28 MiB/s |
| 57 344 : 8 192      | 1.7021 GiB/s | 614.19 MiB/s ; 616.58 MiB/s |

- Single core AVX2 on an AMD Ryzen 5 3600 (Zen 2, 2019).
- On an Apple Silicon M1 CPU throughput is about the same (+-10%).
- MiB/s and GiB/s are w.r.t the total amount of data,
  i.e. original shards + recovery shards.
    - For decoder this includes missing shards.
- Shards are 1024 bytes.
- Encode benchmark
    - Includes [`add_original_shard`][RSE::add_original_shard] and
      [`encode`][RSE::encode] of [`ReedSolomonEncoder`].
- Decode benchmark
    - Has two MiB/s values for 1% and 100% original shard loss, of maximum possible.
    - Provides minimum required amount of shards to decoder.
    - Includes [`add_original_shard`][RSD::add_original_shard],
      [`add_recovery_shard`][RSD::add_recovery_shard] and
      [`decode`][RSD::decode] of [`ReedSolomonDecoder`].


I invite you to clone [reed-solomon-simd] and run your own benchmark:
```sh
$ cargo bench main
```

## Applications

This crate implements the Reed-Solomon codes used by distributed systems, and cryptography, but not the Reed-Solomon codes with error location and correction suitable for local storage.

Reed-Solomon codes have classically provided two functions, error location and error correction. Implementation involves matrix arithmetic or other techniques with complexity worse than `O(length * shards)`.  As such, there are few shards in classical storage applications, and so they use small fields like `GF(2^8)`.

In cryptography and distributed systems, we often employs Lagrange polynomials aka Reed-Solomon for data distribution, but such uses need shards to be much larger, and they require larger fields like prime fields or `GF(2^16)`.  In these cases, encoding and decoding could be accomplished with FFTs or additive FFTs plus special field representations, instead of matrix-like arithmetic.  All this yields much faster codes with complexities like `O(length * log shards)`, but doing so sacrifices the error location and error correction capabilities. 

It turns out this trade off makes sense though because our errors have an adversarial nature in cryptography and distributed systems, meaning if errors occur then they could easily overwhelm location or correction anyways.  We can always detect the presence of errors using hashes of course, so these applications handle error detection to another layer of the protocol.

## Simple usage

1. Divide data into equal-sized original shards.
   Shard size must be multiple of 64 bytes.
2. Decide how many recovery shards you want.
3. Generate recovery shards with [`reed_solomon_simd::encode`].
4. When some original shards get lost, restore them with [`reed_solomon_simd::decode`].
    - You must provide at least as many shards as there were original shards in total,
      in any combination of original shards and recovery shards.

### Example

Divide data into 3 original shards of 64 bytes each and generate 5 recovery shards.
Assume then that original shards #0 and #2 are lost
and restore them by providing 1 original shard and 2 recovery shards.

```rust
let original = [
    b"Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do ",
    b"eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut e",
    b"nim ad minim veniam, quis nostrud exercitation ullamco laboris n",
];

let recovery = reed_solomon_simd::encode(
    3, // total number of original shards
    5, // total number of recovery shards
    original, // all original shards
)?;

let restored = reed_solomon_simd::decode(
    3, // total number of original shards
    5, // total number of recovery shards
    [  // provided original shards with indexes
        (1, &original[1]),
    ],
    [  // provided recovery shards with indexes
        (1, &recovery[1]),
        (4, &recovery[4]),
    ],
)?;

assert_eq!(restored[&0], original[0]);
assert_eq!(restored[&2], original[2]);
# Ok::<(), reed_solomon_simd::Error>(())
```

## Basic usage

[`ReedSolomonEncoder`] and [`ReedSolomonDecoder`] give more control
of the encoding/decoding process.

Here's the above example using these instead:

```rust
use reed_solomon_simd::{ReedSolomonDecoder, ReedSolomonEncoder};
use std::collections::HashMap;

let original = [
    b"Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do ",
    b"eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut e",
    b"nim ad minim veniam, quis nostrud exercitation ullamco laboris n",
];

let mut encoder = ReedSolomonEncoder::new(
    3, // total number of original shards
    5, // total number of recovery shards
    64, // shard size in bytes
)?;

for original in original {
    encoder.add_original_shard(original)?;
}

let result = encoder.encode()?;
let recovery: Vec<_> = result.recovery_iter().collect();

let mut decoder = ReedSolomonDecoder::new(
    3, // total number of original shards
    5, // total number of recovery shards
    64, // shard size in bytes
)?;

decoder.add_original_shard(1, original[1])?;
decoder.add_recovery_shard(1, recovery[1])?;
decoder.add_recovery_shard(4, recovery[4])?;

let result = decoder.decode()?;
let restored: HashMap<_, _> = result.restored_original_iter().collect();

assert_eq!(restored[&0], original[0]);
assert_eq!(restored[&2], original[2]);
# Ok::<(), reed_solomon_simd::Error>(())
```

## Advanced usage

See [`rate`][mod:rate] module for advanced encoding/decoding
using chosen [`Engine`] and [`Rate`].

## Benchmarks against other crates

Use `cargo run --release --example quick-comparison`
to run few simple benchmarks against [`reed-solomon-16`], [`reed-solomon-erasure`]
and [`reed-solomon-novelpoly`] crates.

This crate is the fastest in all cases on my AMD Ryzen 5 3600, except in the
case of decoding with about 42 or fewer recovery shards.
There's also a one-time initialization (< 10 ms) for computing tables
which can dominate at really small data amounts.

[`reed-solomon-16`]: https://crates.io/crates/reed-solomon-16
[`reed-solomon-erasure`]: https://crates.io/crates/reed-solomon-erasure
[`reed-solomon-novelpoly`]: https://crates.io/crates/reed-solomon-novelpoly

## Running tests

Some larger tests are marked `#[ignore]` and are not run with `cargo test`.
Use `cargo test -- --ignored` to run those.

## Safety

The only use of `unsafe` in this crate is to allow for target specific optimizations in [`Ssse3`], [`Avx2`] and [`Neon`].

## Credits

This crate is a fork Markus Laire's [`reed-solomon-16`] crate, which in turn
is based on [Leopard-RS] by Christopher A. Taylor.

[Leopard-RS]: https://github.com/catid/leopard
[reed-solomon-simd]: https://github.com/AndersTrier/reed-solomon-simd

[`Naive`]: https://docs.rs/reed-solomon-simd/2.1.0/reed_solomon_simd/engine/struct.Naive.html
[`NoSimd`]: https://docs.rs/reed-solomon-simd/2.1.0/reed_solomon_simd/engine/struct.NoSimd.html
[`Ssse3`]: https://docs.rs/reed-solomon-simd/2.1.0/reed_solomon_simd/engine/struct.Ssse3.html
[`Avx2`]: https://docs.rs/reed-solomon-simd/2.1.0/reed_solomon_simd/engine/struct.Avx2.html
[`Neon`]: https://docs.rs/reed-solomon-simd/2.1.0/reed_solomon_simd/engine/struct.Neon.html

[`ReedSolomonEncoder`]: https://docs.rs/reed-solomon-simd/2.1.0/reed_solomon_simd/struct.ReedSolomonEncoder.html
[RSE::add_original_shard]: https://docs.rs/reed-solomon-simd/2.1.0/reed_solomon_simd/struct.ReedSolomonEncoder.html#method.add_original_shard
[RSE::encode]: https://docs.rs/reed-solomon-simd/2.1.0/reed_solomon_simd/struct.ReedSolomonEncoder.html#method.encode

[`ReedSolomonDecoder`]: https://docs.rs/reed-solomon-simd/2.1.0/reed_solomon_simd/struct.ReedSolomonDecoder.html
[RSD::add_original_shard]: https://docs.rs/reed-solomon-simd/2.1.0/reed_solomon_simd/struct.ReedSolomonDecoder.html#method.add_original_shard
[RSD::add_recovery_shard]: https://docs.rs/reed-solomon-simd/2.1.0/reed_solomon_simd/struct.ReedSolomonDecoder.html#method.add_recovery_shard
[RSD::decode]: https://docs.rs/reed-solomon-simd/2.1.0/reed_solomon_simd/struct.ReedSolomonDecoder.html#method.decode

[`Engine`]: https://docs.rs/reed-solomon-simd/2.1.0/reed_solomon_simd/engine/trait.Engine.html
[`Rate`]: https://docs.rs/reed-solomon-simd/2.1.0/reed_solomon_simd/rate/trait.Rate.html

[mod:rate]: https://docs.rs/reed-solomon-simd/2.1.0/reed_solomon_simd/rate/index.html

[`reed_solomon_simd::encode`]: https://docs.rs/reed-solomon-simd/2.1.0/reed_solomon_simd/fn.encode.html
[`reed_solomon_simd::decode`]: https://docs.rs/reed-solomon-simd/2.1.0/reed_solomon_simd/fn.decode.html
