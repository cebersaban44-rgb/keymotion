//! Graphics card temperature, the way Task Manager reads it: the display
//! driver's performance data (`D3DKMTQueryAdapterInfo`,
//! `KMTQAITYPE_ADAPTERPERFDATA`). Needs no administrator rights and no
//! driver of our own; cards and drivers that don't report it (WDDM before
//! 2.4, most integrated graphics) give nothing. The processor's
//! temperature has no such source and is not read.

/// The hottest graphics card, in °C; `None` when no card reports a
/// temperature.
pub fn gpu_temperature() -> Option<f32> {
    sys::temperatures().into_iter().reduce(f32::max)
}

/// A reported value in tenths of a degree, as °C; 0 means "not reported".
pub fn from_deci(value: u32) -> Option<f32> {
    // Above 150 °C the value is not a temperature.
    (value > 0 && value < 1_500).then(|| value as f32 / 10.0)
}

#[cfg(windows)]
mod sys {
    use super::from_deci;
    use windows::Wdk::Graphics::Direct3D::{
        D3DKMTCloseAdapter, D3DKMTEnumAdapters2, D3DKMTQueryAdapterInfo, D3DKMT_ADAPTERINFO, D3DKMT_ADAPTER_PERFDATA,
        D3DKMT_CLOSEADAPTER, D3DKMT_ENUMADAPTERS2, D3DKMT_QUERYADAPTERINFO, KMTQAITYPE_ADAPTERPERFDATA,
    };

    pub fn temperatures() -> Vec<f32> {
        let mut list = D3DKMT_ENUMADAPTERS2::default();
        // SAFETY: with no array the call only reports how many adapters there are.
        if unsafe { D3DKMTEnumAdapters2(&mut list) }.is_err() || list.NumAdapters == 0 {
            return Vec::new();
        }
        let mut adapters = vec![D3DKMT_ADAPTERINFO::default(); list.NumAdapters as usize];
        list.pAdapters = adapters.as_mut_ptr();
        // SAFETY: an array of `NumAdapters` entries to fill; each adapter
        // handle it opens is closed below.
        if unsafe { D3DKMTEnumAdapters2(&mut list) }.is_err() {
            return Vec::new();
        }
        let count = (list.NumAdapters as usize).min(adapters.len());
        let mut out = Vec::new();
        for a in &adapters[..count] {
            let mut perf = D3DKMT_ADAPTER_PERFDATA::default();
            let mut query = D3DKMT_QUERYADAPTERINFO {
                hAdapter: a.hAdapter,
                Type: KMTQAITYPE_ADAPTERPERFDATA,
                pPrivateDriverData: (&raw mut perf).cast(),
                PrivateDriverDataSize: size_of::<D3DKMT_ADAPTER_PERFDATA>() as u32,
            };
            // SAFETY: `perf` is the buffer named in `query`, with its size.
            if unsafe { D3DKMTQueryAdapterInfo(&mut query) }.is_ok() {
                if let Some(t) = from_deci(perf.Temperature) {
                    out.push(t);
                }
            }
            let close = D3DKMT_CLOSEADAPTER { hAdapter: a.hAdapter };
            // SAFETY: a handle D3DKMTEnumAdapters2 opened, closed once.
            let _ = unsafe { D3DKMTCloseAdapter(&close) };
        }
        out
    }
}

#[cfg(not(windows))]
mod sys {
    pub fn temperatures() -> Vec<f32> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tenths_of_a_degree() {
        assert_eq!(from_deci(0), None);
        assert_eq!(from_deci(655), Some(65.5));
        assert_eq!(from_deci(40_000), None);
    }

    /// By hand: `cargo test -p kb-winsys -- --ignored --nocapture gpu_temp`.
    #[test]
    #[ignore]
    fn print_the_gpu_temperature() {
        println!("GPU temperature: {:?}", gpu_temperature());
    }
}
