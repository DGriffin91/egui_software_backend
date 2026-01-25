use crate::math::vec4::{Vec4, vec4};

#[cfg(target_arch = "x86_64")]
pub(crate) mod avx2;
#[cfg(target_arch = "aarch64")]
pub(crate) mod neon;
#[cfg(target_arch = "x86_64")]
pub(crate) mod sse41;

/// An available to run on this processor implementation
///
/// This enum doesn't implement `SelectedImpl` by choice, thus forcing compiler monomorphisation
/// a the call tree up to the closest [`dispatch_simd_impl!`]
///
/// This enum values can only be created from [`available_instrs()`], so there is no way to select
/// a non supported by the current processor implementation
#[derive(Clone, Copy)]
pub(crate) enum AvailableImpl {
    Generic(GenericImpl),
    #[cfg(target_arch = "x86_64")]
    Sse41(sse41::Sse41Impl),
    #[cfg(target_arch = "x86_64")]
    Avx2(avx2::Avx2Impl),
    #[cfg(target_arch = "aarch64")]
    Neon(neon::NeonImpl),
}

/// Non empty, order from most performant to least one list of supported by the current processor
/// implementations
pub(crate) fn available_instrs() -> &'static [AvailableImpl] {
    const GENERIC: GenericImpl = GenericImpl;
    #[cfg(all(target_arch = "x86_64", feature = "std"))]
    if std::arch::is_x86_feature_detected!("sse4.1") {
        const SSE41: sse41::Sse41Impl = unsafe { sse41::Sse41Impl::new() };
        if std::arch::is_x86_feature_detected!("avx2") {
            const AVX2: avx2::Avx2Impl = unsafe { avx2::Avx2Impl::new() };
            return &[
                AvailableImpl::Avx2(AVX2),
                AvailableImpl::Sse41(SSE41),
                AvailableImpl::Generic(GENERIC),
            ];
        } else {
            return &[AvailableImpl::Sse41(SSE41), AvailableImpl::Generic(GENERIC)];
        }
    }

    #[cfg(all(target_arch = "aarch64", feature = "std"))]
    if std::arch::is_aarch64_feature_detected!("neon") {
        const NEON: neon::NeonImp = unsafe { neon::NeonImpl::new() };
        return &[AvailableImpl::Neon(NEON), AvailableImpl::Generic(GENERIC)];
    }

    &[AvailableImpl::Generic(GENERIC)]
}

impl Default for AvailableImpl {
    /// Returns the most performant available implementation
    fn default() -> Self {
        *available_instrs().first().expect("at least one impl")
    }
}

impl core::fmt::Display for AvailableImpl {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Generic(_) => f.write_str("Generic"),
            #[cfg(target_arch = "x86_64")]
            Self::Sse41(_) => f.write_str("Sse41"),
            #[cfg(target_arch = "x86_64")]
            Self::Avx2(_) => f.write_str("Avx2"),
            #[cfg(target_arch = "aarch64")]
            Self::Neon(_) => f.write_str("Neon"),
        }
    }
}

impl core::fmt::Debug for AvailableImpl {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(&self, f)
    }
}

#[doc(hidden)]
#[macro_export]
macro_rules! dispatch_simd_impl {
    (|$simd_impl:ident| $body:expr) => {
        $crate::dispatch_simd_impl!($crate::color::AvailableImpl::default(), |$simd_impl| $body)
    };
    ($available_impl:expr, |$simd_impl:ident| $body:expr) => {
        match $available_impl {
            $crate::color::AvailableImpl::Generic(i) => {
                (|$simd_impl: $crate::color::GenericImpl| $body)(i)
            }
            #[cfg(target_arch = "x86_64")]
            $crate::color::AvailableImpl::Sse41(i) => {
                (|$simd_impl: $crate::color::sse41::Sse41Impl| $body)(i)
            }
            #[cfg(target_arch = "x86_64")]
            $crate::color::AvailableImpl::Avx2(i) => {
                (|$simd_impl: $crate::color::avx2::Avx2Impl| $body)(i)
            }
            #[cfg(target_arch = "aarch64")]
            $crate::color::AvailableImpl::Neon(i) => {
                (|$simd_impl: $crate::color::neon::NeonImpl| $body)(i)
            }
        }
    };
}

/// This trait holds performance sensitive methods that often can be writing in more architecture
/// dependent specialized versions
///
/// All methods of this trait contains a generic implementation
pub(crate) trait SelectedImpl: Copy + Sync + Send + 'static {
    fn egui_blend_u8_slice(self, src: &[[u8; 4]], dst: &mut [[u8; 4]]) {
        for (pixel, src) in dst.iter_mut().zip(src) {
            *pixel = self.egui_blend_u8(*src, *pixel);
        }
    }
    /// dst[i] = blend(src * tint_fn(), dst[i]) // As unorm
    /// blend fn is (ONE, ONE_MINUS_SRC_ALPHA)
    fn egui_blend_u8_slice_one_src_tinted_fn(
        self,
        src: [u8; 4],
        mut tint_fn: impl FnMut() -> [u8; 4],
        dst: &mut [[u8; 4]],
    ) {
        for pixel in dst.iter_mut() {
            *pixel = self.egui_blend_u8(self.unorm_mult4x4(tint_fn(), src), *pixel);
        }
    }

    /// dst[i] = blend(src[i] * tint, dst[i]) // As unorm
    /// blend fn is (ONE, ONE_MINUS_SRC_ALPHA)
    fn egui_blend_u8_slice_tinted(self, src: &[[u8; 4]], tint: [u8; 4], dst: &mut [[u8; 4]]) {
        for (pixel, tex_color) in dst.iter_mut().zip(src) {
            *pixel = self.egui_blend_u8(self.unorm_mult4x4(tint, *tex_color), *pixel);
        }
    }

    /// dst[i] = blend(src, dst[i])
    /// blend fn is (ONE, ONE_MINUS_SRC_ALPHA)
    fn egui_blend_u8_slice_one_src(self, src: [u8; 4], dst: &mut [[u8; 4]]) {
        for pixel in dst {
            *pixel = self.egui_blend_u8(src, *pixel);
        }
    }

    // https://www.lgfae.com/posts/2025-09-01-AlphaBlendWithSIMD.html
    /// blend fn is (ONE, ONE_MINUS_SRC_ALPHA)
    #[inline(always)]
    fn egui_blend_u8(self, src: [u8; 4], mut dst: [u8; 4]) -> [u8; 4] {
        let a = src[3];
        if a == 255 {
            return src;
        }
        if a != 0 {
            let alpha = a as u64;
            let alpha_compl = 0xFF ^ alpha;
            let dst64 = as_color16(u32::from_le_bytes(dst));

            let res16 = dst64 * alpha_compl + 0x0080008000800080;
            let res8 = res16 + ((res16 >> 8) & 0x00FF00FF00FF00FF);

            // transform the result back to 32 bytes
            let res = (res8 >> 8) & 0x00FF00FF00FF00FF;
            let res = (res | (res >> 8)) & 0x0000FFFF0000FFFF;
            let res = res | (res >> 16);
            dst = u32::to_le_bytes((res & 0x00000000FFFFFFFF) as u32);
        }

        [
            dst[0].saturating_add(src[0]),
            dst[1].saturating_add(src[1]),
            dst[2].saturating_add(src[2]),
            dst[3].saturating_add(src[3]),
        ]
    }

    #[inline(always)]
    fn unorm_mult4x4(self, a: [u8; 4], b: [u8; 4]) -> [u8; 4] {
        [
            unorm_mult(a[0] as u32, b[0] as u32) as u8,
            unorm_mult(a[1] as u32, b[1] as u32) as u8,
            unorm_mult(a[2] as u32, b[2] as u32) as u8,
            unorm_mult(a[3] as u32, b[3] as u32) as u8,
        ]
    }
}
#[derive(Clone, Copy)]
pub(crate) struct GenericImpl;

impl SelectedImpl for GenericImpl {}

#[inline(always)]
pub fn vec4_to_u8x4(v: &Vec4) -> [u8; 4] {
    let v = v.clamp(Vec4::ZERO, Vec4::ONE) * 255.0 + 0.5;
    [v.x as u8, v.y as u8, v.z as u8, v.w as u8]
}

#[inline(always)]
pub fn u8x4_to_vec4(v: &[u8; 4]) -> Vec4 {
    vec4(
        v[0] as f32 / 255.0,
        v[1] as f32 / 255.0,
        v[2] as f32 / 255.0,
        v[3] as f32 / 255.0,
    )
}

/// transforms 4 bytes RGBA into 8 bytes 0R0G0B0A
#[inline(always)]
pub fn as_color16(color: u32) -> u64 {
    let x = color as u64;
    let x = ((x & 0xFFFF0000) << 16) | (x & 0xFFFF);
    ((x & 0x0000FF000000FF00) << 8) | (x & 0x000000FF000000FF)
}

#[inline(always)]
// Jerry R. Van Aken - Alpha Blending with No Division Operations https://arxiv.org/pdf/2202.02864
// Input should be 0..255, is multiplied as if it were 0..1f
pub fn unorm_mult(mut a: u32, b: u32) -> u32 {
    a *= b;
    a += 0x80;
    a += a >> 8;
    a >> 8
}

#[inline(always)]
pub fn swizzle_rgba_bgra(a: [u8; 4]) -> [u8; 4] {
    [a[2], a[1], a[0], a[3]]
}
