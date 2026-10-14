//! Thin wrapper over the Windows Performance Data Helper (PDH). Counter paths are added with
//! `PdhAddEnglishCounterW`, so they work on any system language.

use windows::core::PCWSTR;
use windows::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
    PdhGetFormattedCounterValue, PdhOpenQueryW, PDH_FMT_COUNTERVALUE, PDH_FMT_COUNTERVALUE_ITEM_W,
    PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA,
};

const ERROR_SUCCESS: u32 = 0;

#[derive(Clone, Copy)]
pub(crate) struct Counter(PDH_HCOUNTER);

pub(crate) struct Pdh {
    query: PDH_HQUERY,
    #[cfg(test)]
    pub(crate) array_fixture: Option<Vec<(String, f64)>>,
}

// SAFETY: PDH handles are plain identifiers with no thread affinity. A collector, with its query
// and counters, is moved to its worker thread once and used from there only.
unsafe impl Send for Pdh {}
unsafe impl Send for Counter {}

fn describe(code: u32) -> String {
    match code {
        0x800007D5 => "contador sem dados ainda".to_string(),
        0xC0000BB8 => "contador não existe neste Windows".to_string(),
        0xC0000BB9 => "contador não existe neste Windows".to_string(),
        0xC0000BC0 => "contador não existe neste Windows".to_string(),
        other => format!("erro PDH 0x{other:08X}"),
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

impl Pdh {
    pub(crate) fn new() -> Result<Self, String> {
        let mut query = PDH_HQUERY::default();
        // SAFETY: `query` is a valid out pointer; a null data source means the local machine.
        let status = unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut query) };
        if status != ERROR_SUCCESS {
            return Err(describe(status));
        }
        Ok(Self {
            query,
            #[cfg(test)]
            array_fixture: None,
        })
    }

    /// Adds a counter by its English path, for example `\Processor Information(_Total)\Processor Frequency`.
    pub(crate) fn add(&mut self, path: &str) -> Result<Counter, String> {
        let path = wide(path);
        let mut counter = PDH_HCOUNTER::default();
        // SAFETY: `path` is NUL terminated and outlives the call; `counter` is a valid out pointer.
        let status =
            unsafe { PdhAddEnglishCounterW(self.query, PCWSTR(path.as_ptr()), 0, &mut counter) };
        if status != ERROR_SUCCESS {
            return Err(describe(status));
        }
        Ok(Counter(counter))
    }

    /// Takes a new sample of every counter. Rate counters need two samples before they have a value.
    pub(crate) fn collect(&self) -> Result<(), String> {
        // SAFETY: the query handle is valid for the lifetime of `self`.
        let status = unsafe { PdhCollectQueryData(self.query) };
        if status != ERROR_SUCCESS {
            return Err(describe(status));
        }
        Ok(())
    }

    pub(crate) fn value(&self, counter: Counter) -> Result<f64, String> {
        let mut value = PDH_FMT_COUNTERVALUE::default();
        // SAFETY: `counter` belongs to this query; `value` is a valid out pointer.
        let status =
            unsafe { PdhGetFormattedCounterValue(counter.0, PDH_FMT_DOUBLE, None, &mut value) };
        if status != ERROR_SUCCESS {
            return Err(describe(status));
        }
        if value.CStatus != 0 && value.CStatus != 1 {
            return Err(describe(value.CStatus));
        }
        // SAFETY: PDH_FMT_DOUBLE was requested, so the double member is the initialized one.
        Ok(unsafe { value.Anonymous.doubleValue })
    }

    /// Values of a wildcard counter, one per instance, as `(instance name, value)`.
    pub(crate) fn array(&self, counter: Counter) -> Result<Vec<(String, f64)>, String> {
        #[cfg(test)]
        if let Some(values) = &self.array_fixture {
            return Ok(values.clone());
        }
        let mut size = 0u32;
        let mut count = 0u32;
        // SAFETY: first call with a null buffer only asks for the required size.
        let status = unsafe {
            PdhGetFormattedCounterArrayW(counter.0, PDH_FMT_DOUBLE, &mut size, &mut count, None)
        };
        if status != PDH_MORE_DATA {
            return Err(describe(status));
        }
        // The buffer holds the item array followed by the instance name strings.
        let words = (size as usize).div_ceil(std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>());
        let mut buffer = vec![PDH_FMT_COUNTERVALUE_ITEM_W::default(); words.max(1)];
        // SAFETY: `buffer` is at least `size` bytes and suitably aligned for the item type.
        let status = unsafe {
            PdhGetFormattedCounterArrayW(
                counter.0,
                PDH_FMT_DOUBLE,
                &mut size,
                &mut count,
                Some(buffer.as_mut_ptr()),
            )
        };
        if status != ERROR_SUCCESS {
            return Err(describe(status));
        }
        let mut out = Vec::with_capacity(count as usize);
        for item in buffer.iter().take(count as usize) {
            // SAFETY: PDH wrote a NUL terminated name inside `buffer` for each of the `count` items.
            let name = unsafe { item.szName.to_string() }.unwrap_or_default();
            if item.FmtValue.CStatus == 0 || item.FmtValue.CStatus == 1 {
                // SAFETY: PDH_FMT_DOUBLE was requested.
                out.push((name, unsafe { item.FmtValue.Anonymous.doubleValue }));
            }
        }
        Ok(out)
    }
}

impl Drop for Pdh {
    fn drop(&mut self) {
        // SAFETY: the handle was opened by `new` and is closed exactly once.
        unsafe {
            PdhCloseQuery(self.query);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_known_counter() {
        let mut pdh = Pdh::new().expect("open query");
        let counter = pdh
            .add(r"\Processor Information(_Total)\% Processor Time")
            .expect("add counter");
        pdh.collect().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        pdh.collect().unwrap();
        let v = pdh.value(counter).expect("value");
        assert!((0.0..=100.0 * 4.0).contains(&v), "unexpected value {v}");
    }

    #[test]
    fn unknown_counter_is_an_error_not_a_panic() {
        let mut pdh = Pdh::new().unwrap();
        assert!(pdh.add(r"\Does Not Exist(_Total)\Nothing").is_err());
    }

    #[test]
    fn wildcard_counter_returns_instances() {
        let mut pdh = Pdh::new().unwrap();
        let counter = pdh.add(r"\PhysicalDisk(*)\Disk Read Bytes/sec").unwrap();
        pdh.collect().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        pdh.collect().unwrap();
        assert!(!pdh.array(counter).unwrap().is_empty());
    }
}
