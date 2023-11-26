use std::iter::zip;

#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

use crate::engine::{
    self, fwht,
    tables::{self, Mul128, Multiply128lutT, Skew},
    Engine, GfElement, ShardsRefMut, GF_MODULUS, GF_ORDER,
};

// ======================================================================
// Avx2 - PUBLIC

/// Optimized [`Engine`] using AVX2 instructions.
///
/// [`Avx2`] is an optimized engine that follows the same algorithm as
/// [`NoSimd`] but takes advantage of the x86 AVX2 SIMD instructions.
///
/// [`NoSimd`]: crate::engine::NoSimd
#[derive(Clone)]
pub struct Avx2 {
    mul128: &'static Mul128,
    skew: &'static Skew,
}

impl Avx2 {
    /// Creates new [`Avx2`], initializing all [tables]
    /// needed for encoding or decoding.
    ///
    /// Currently only difference between encoding/decoding is
    /// [`LogWalsh`] (128 kiB) which is only needed for decoding.
    ///
    /// [`LogWalsh`]: crate::engine::tables::LogWalsh
    pub fn new() -> Self {
        let mul128 = tables::initialize_mul128();
        let skew = tables::initialize_skew();

        Self { mul128, skew }
    }
}

impl Engine for Avx2 {
    fn fft(
        &self,
        data: &mut ShardsRefMut,
        pos: usize,
        size: usize,
        truncated_size: usize,
        skew_delta: usize,
    ) {
        unsafe {
            self.fft_private_avx2(data, pos, size, truncated_size, skew_delta);
        }
    }

    fn fwht(data: &mut [GfElement; GF_ORDER], truncated_size: usize) {
        unsafe {
            Self::fwht_private_avx2(data, truncated_size);
        }
    }

    fn ifft(
        &self,
        data: &mut ShardsRefMut,
        pos: usize,
        size: usize,
        truncated_size: usize,
        skew_delta: usize,
    ) {
        unsafe {
            self.ifft_private_avx2(data, pos, size, truncated_size, skew_delta);
        }
    }

    fn mul(&self, x: &mut [u8], log_m: GfElement) {
        unsafe {
            self.mul_avx2(x, log_m);
        }
    }

    fn xor(x: &mut [u8], y: &[u8]) {
        let x: &mut [u64] = bytemuck::cast_slice_mut(x);
        let y: &[u64] = bytemuck::cast_slice(y);

        for (x64, y64) in zip(x.iter_mut(), y.iter()) {
            *x64 ^= y64;
        }
    }

    fn eval_poly(erasures: &mut [GfElement; GF_ORDER], truncated_size: usize) {
        unsafe { Self::eval_poly_avx2(erasures, truncated_size) }
    }
}

// ======================================================================
// Avx2 - IMPL Default

impl Default for Avx2 {
    fn default() -> Self {
        Self::new()
    }
}

// ======================================================================
// Avx2 - PRIVATE
//
//

impl Avx2 {
    #[target_feature(enable = "avx2")]
    unsafe fn mul_avx2(&self, x: &mut [u8], log_m: GfElement) {
        let lut = &self.mul128[log_m as usize];

        for chunk in x.chunks_exact_mut(64) {
            let x_ptr = chunk.as_mut_ptr() as *mut __m256i;
            unsafe {
                let x_lo = _mm256_loadu_si256(x_ptr);
                let x_hi = _mm256_loadu_si256(x_ptr.add(1));
                let (prod_lo, prod_hi) = Self::mul_256(x_lo, x_hi, lut);
                _mm256_storeu_si256(x_ptr, prod_lo);
                _mm256_storeu_si256(x_ptr.add(1), prod_hi);
            }
        }
    }

    // Impelemntation of LEO_MUL_256
    #[inline(always)]
    fn mul_256(value_lo: __m256i, value_hi: __m256i, lut: &Multiply128lutT) -> (__m256i, __m256i) {
        let mut prod_lo: __m256i;
        let mut prod_hi: __m256i;

        unsafe {
            let t0_lo = _mm256_broadcastsi128_si256(_mm_loadu_si128(
                &lut.lo[0] as *const u128 as *const __m128i,
            ));
            let t1_lo = _mm256_broadcastsi128_si256(_mm_loadu_si128(
                &lut.lo[1] as *const u128 as *const __m128i,
            ));
            let t2_lo = _mm256_broadcastsi128_si256(_mm_loadu_si128(
                &lut.lo[2] as *const u128 as *const __m128i,
            ));
            let t3_lo = _mm256_broadcastsi128_si256(_mm_loadu_si128(
                &lut.lo[3] as *const u128 as *const __m128i,
            ));

            let t0_hi = _mm256_broadcastsi128_si256(_mm_loadu_si128(
                &lut.hi[0] as *const u128 as *const __m128i,
            ));
            let t1_hi = _mm256_broadcastsi128_si256(_mm_loadu_si128(
                &lut.hi[1] as *const u128 as *const __m128i,
            ));
            let t2_hi = _mm256_broadcastsi128_si256(_mm_loadu_si128(
                &lut.hi[2] as *const u128 as *const __m128i,
            ));
            let t3_hi = _mm256_broadcastsi128_si256(_mm_loadu_si128(
                &lut.hi[3] as *const u128 as *const __m128i,
            ));

            let clr_mask = _mm256_set1_epi8(0x0f);

            let data_0 = _mm256_and_si256(value_lo, clr_mask);
            prod_lo = _mm256_shuffle_epi8(t0_lo, data_0);
            prod_hi = _mm256_shuffle_epi8(t0_hi, data_0);

            let data_1 = _mm256_and_si256(_mm256_srli_epi64(value_lo, 4), clr_mask);
            prod_lo = _mm256_xor_si256(prod_lo, _mm256_shuffle_epi8(t1_lo, data_1));
            prod_hi = _mm256_xor_si256(prod_hi, _mm256_shuffle_epi8(t1_hi, data_1));

            let data_0 = _mm256_and_si256(value_hi, clr_mask);
            prod_lo = _mm256_xor_si256(prod_lo, _mm256_shuffle_epi8(t2_lo, data_0));
            prod_hi = _mm256_xor_si256(prod_hi, _mm256_shuffle_epi8(t2_hi, data_0));

            let data_1 = _mm256_and_si256(_mm256_srli_epi64(value_hi, 4), clr_mask);
            prod_lo = _mm256_xor_si256(prod_lo, _mm256_shuffle_epi8(t3_lo, data_1));
            prod_hi = _mm256_xor_si256(prod_hi, _mm256_shuffle_epi8(t3_hi, data_1));
        }

        (prod_lo, prod_hi)
    }

    //// {x_lo, x_hi} ^= {y_lo, y_hi} * log_m
    // Implementation of LEO_MULADD_256
    #[inline(always)]
    fn muladd_256(
        mut x_lo: __m256i,
        mut x_hi: __m256i,
        y_lo: __m256i,
        y_hi: __m256i,
        lut: &Multiply128lutT,
    ) -> (__m256i, __m256i) {
        let (prod_lo, prod_hi) = Self::mul_256(y_lo, y_hi, lut);
        unsafe {
            x_lo = _mm256_xor_si256(x_lo, prod_lo);
            x_hi = _mm256_xor_si256(x_hi, prod_hi);
        }
        (x_lo, x_hi)
    }
}

// ======================================================================
// Avx2 - PRIVATE - FWHT (fast Walsh-Hadamard transform)

impl Avx2 {
    #[target_feature(enable = "avx2")]
    unsafe fn fwht_private_avx2(data: &mut [GfElement; GF_ORDER], truncated_size: usize) {
        fwht::fwht(data, truncated_size)
    }
}

// ======================================================================
// Avx2 - PRIVATE - FFT (fast Fourier transform)

impl Avx2 {
    // Implementation of LEO_FFTB_256
    #[inline(always)]
    fn fftb_256(&self, x: &mut [u8; 64], y: &mut [u8; 64], log_m: GfElement) {
        let lut = &self.mul128[log_m as usize];
        let x_ptr = x.as_mut_ptr() as *mut __m256i;
        let y_ptr = y.as_mut_ptr() as *mut __m256i;
        unsafe {
            let mut x_lo = _mm256_loadu_si256(x_ptr);
            let mut x_hi = _mm256_loadu_si256(x_ptr.add(1));

            let mut y_lo = _mm256_loadu_si256(y_ptr);
            let mut y_hi = _mm256_loadu_si256(y_ptr.add(1));

            (x_lo, x_hi) = Self::muladd_256(x_lo, x_hi, y_lo, y_hi, lut);

            _mm256_storeu_si256(x_ptr, x_lo);
            _mm256_storeu_si256(x_ptr.add(1), x_hi);

            y_lo = _mm256_xor_si256(y_lo, x_lo);
            y_hi = _mm256_xor_si256(y_hi, x_hi);

            _mm256_storeu_si256(y_ptr, y_lo);
            _mm256_storeu_si256(y_ptr.add(1), y_hi);
        }
    }

    // Partial butterfly, caller must do `GF_MODULUS` check with `xor`.
    #[inline(always)]
    fn fft_butterfly_partial(&self, x: &mut [u8], y: &mut [u8], log_m: GfElement) {
        // While we wait for array_chunks/slice_as_chunks (#74985) to become stable,
        // we have to try_into().unwrap() (which cannot fail in this case)
        for (x_chunk, y_chunk) in zip(x.chunks_exact_mut(64), y.chunks_exact_mut(64)) {
            self.fftb_256(
                x_chunk.try_into().unwrap(),
                y_chunk.try_into().unwrap(),
                log_m,
            );
        }
    }

    #[inline(always)]
    fn fft_butterfly_two_layers(
        &self,
        data: &mut ShardsRefMut,
        pos: usize,
        dist: usize,
        log_m01: GfElement,
        log_m23: GfElement,
        log_m02: GfElement,
    ) {
        let (s0, s1, s2, s3) = data.dist4_mut(pos, dist);

        // FIRST LAYER

        if log_m02 == GF_MODULUS {
            Self::xor(s2, s0);
            Self::xor(s3, s1);
        } else {
            self.fft_butterfly_partial(s0, s2, log_m02);
            self.fft_butterfly_partial(s1, s3, log_m02);
        }

        // SECOND LAYER

        if log_m01 == GF_MODULUS {
            Self::xor(s1, s0);
        } else {
            self.fft_butterfly_partial(s0, s1, log_m01);
        }

        if log_m23 == GF_MODULUS {
            Self::xor(s3, s2);
        } else {
            self.fft_butterfly_partial(s2, s3, log_m23);
        }
    }

    #[target_feature(enable = "avx2")]
    unsafe fn fft_private_avx2(
        &self,
        data: &mut ShardsRefMut,
        pos: usize,
        size: usize,
        truncated_size: usize,
        skew_delta: usize,
    ) {
        // Drop unsafe privileges
        self.fft_private(data, pos, size, truncated_size, skew_delta);
    }

    #[inline(always)]
    fn fft_private(
        &self,
        data: &mut ShardsRefMut,
        pos: usize,
        size: usize,
        truncated_size: usize,
        skew_delta: usize,
    ) {
        // TWO LAYERS AT TIME

        let mut dist4 = size;
        let mut dist = size >> 2;
        while dist != 0 {
            let mut r = 0;
            while r < truncated_size {
                let base = r + dist + skew_delta - 1;

                let log_m01 = self.skew[base];
                let log_m02 = self.skew[base + dist];
                let log_m23 = self.skew[base + dist * 2];

                for i in r..r + dist {
                    self.fft_butterfly_two_layers(data, pos + i, dist, log_m01, log_m23, log_m02)
                }

                r += dist4;
            }
            dist4 = dist;
            dist >>= 2;
        }

        // FINAL ODD LAYER

        if dist4 == 2 {
            let mut r = 0;
            while r < truncated_size {
                let log_m = self.skew[r + skew_delta];

                let (x, y) = data.dist2_mut(pos + r, 1);

                if log_m == GF_MODULUS {
                    Self::xor(y, x);
                } else {
                    self.fft_butterfly_partial(x, y, log_m)
                }

                r += 2;
            }
        }
    }
}

// ======================================================================
// Avx2 - PRIVATE - IFFT (inverse fast Fourier transform)

impl Avx2 {
    // Implementation of LEO_IFFTB_256
    #[inline(always)]
    fn ifftb_256(&self, x: &mut [u8; 64], y: &mut [u8; 64], log_m: GfElement) {
        let lut = &self.mul128[log_m as usize];
        let x_ptr = x.as_mut_ptr() as *mut __m256i;
        let y_ptr = y.as_mut_ptr() as *mut __m256i;

        unsafe {
            let mut x_lo = _mm256_loadu_si256(x_ptr);
            let mut x_hi = _mm256_loadu_si256(x_ptr.add(1));

            let mut y_lo = _mm256_loadu_si256(y_ptr);
            let mut y_hi = _mm256_loadu_si256(y_ptr.add(1));

            y_lo = _mm256_xor_si256(y_lo, x_lo);
            y_hi = _mm256_xor_si256(y_hi, x_hi);

            _mm256_storeu_si256(y_ptr, y_lo);
            _mm256_storeu_si256(y_ptr.add(1), y_hi);

            (x_lo, x_hi) = Self::muladd_256(x_lo, x_hi, y_lo, y_hi, lut);

            _mm256_storeu_si256(x_ptr, x_lo);
            _mm256_storeu_si256(x_ptr.add(1), x_hi);
        }
    }

    #[inline(always)]
    fn ifft_butterfly_partial(&self, x: &mut [u8], y: &mut [u8], log_m: GfElement) {
        // While we wait for array_chunks/slice_as_chunks (#74985) to become stable,
        // we'll have to try_into() to array
        for (x_chunk, y_chunk) in zip(x.chunks_exact_mut(64), y.chunks_exact_mut(64)) {
            self.ifftb_256(
                x_chunk.try_into().unwrap(),
                y_chunk.try_into().unwrap(),
                log_m,
            );
        }
    }

    #[inline(always)]
    fn ifft_butterfly_two_layers(
        &self,
        data: &mut ShardsRefMut,
        pos: usize,
        dist: usize,
        log_m01: GfElement,
        log_m23: GfElement,
        log_m02: GfElement,
    ) {
        let (s0, s1, s2, s3) = data.dist4_mut(pos, dist);

        // FIRST LAYER

        if log_m01 == GF_MODULUS {
            Self::xor(s1, s0);
        } else {
            self.ifft_butterfly_partial(s0, s1, log_m01);
        }

        if log_m23 == GF_MODULUS {
            Self::xor(s3, s2);
        } else {
            self.ifft_butterfly_partial(s2, s3, log_m23);
        }

        // SECOND LAYER

        if log_m02 == GF_MODULUS {
            Self::xor(s2, s0);
            Self::xor(s3, s1);
        } else {
            self.ifft_butterfly_partial(s0, s2, log_m02);
            self.ifft_butterfly_partial(s1, s3, log_m02);
        }
    }

    #[target_feature(enable = "avx2")]
    unsafe fn ifft_private_avx2(
        &self,
        data: &mut ShardsRefMut,
        pos: usize,
        size: usize,
        truncated_size: usize,
        skew_delta: usize,
    ) {
        // Drop unsafe privileges
        self.ifft_private(data, pos, size, truncated_size, skew_delta)
    }

    #[inline(always)]
    fn ifft_private(
        &self,
        data: &mut ShardsRefMut,
        pos: usize,
        size: usize,
        truncated_size: usize,
        skew_delta: usize,
    ) {
        // TWO LAYERS AT TIME

        let mut dist = 1;
        let mut dist4 = 4;
        while dist4 <= size {
            let mut r = 0;
            while r < truncated_size {
                let base = r + dist + skew_delta - 1;

                let log_m01 = self.skew[base];
                let log_m02 = self.skew[base + dist];
                let log_m23 = self.skew[base + dist * 2];

                for i in r..r + dist {
                    self.ifft_butterfly_two_layers(data, pos + i, dist, log_m01, log_m23, log_m02)
                }

                r += dist4;
            }
            dist = dist4;
            dist4 <<= 2;
        }

        // FINAL ODD LAYER

        if dist < size {
            let log_m = self.skew[dist + skew_delta - 1];
            if log_m == GF_MODULUS {
                Self::xor_within(data, pos + dist, pos, dist);
            } else {
                let (mut a, mut b) = data.split_at_mut(pos + dist);
                for i in 0..dist {
                    self.ifft_butterfly_partial(
                        &mut a[pos + i], // data[pos + i]
                        &mut b[i],       // data[pos + i + dist]
                        log_m,
                    );
                }
            }
        }
    }
}

// ======================================================================
// Avx2 - PRIVATE - Evaluate polynomial

impl Avx2 {
    #[target_feature(enable = "avx2")]
    unsafe fn eval_poly_avx2(erasures: &mut [GfElement; GF_ORDER], truncated_size: usize) {
        engine::eval_poly::<Self>(erasures, truncated_size)
    }
}

// ======================================================================
// TESTS

// Engines are tested indirectly via roundtrip tests of HighRate and LowRate.
