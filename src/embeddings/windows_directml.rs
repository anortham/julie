use std::cmp::Reverse;

use anyhow::Result;

#[cfg(target_os = "windows")]
use anyhow::Context;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_DESC1, DXGI_ADAPTER_FLAG_SOFTWARE,
    DXGI_ERROR_NOT_FOUND,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectMlAdapterInfo {
    pub index: i32,
    pub name: String,
    pub is_software: bool,
    pub is_discrete: bool,
    pub dedicated_video_memory: u64,
    pub is_remote: bool,
    pub is_virtual: bool,
}

impl DirectMlAdapterInfo {
    fn is_eligible(&self) -> bool {
        !(self.is_software
            || self.is_remote
            || self.is_virtual
            || adapter_name_looks_remote_or_virtual(&self.name))
    }
}

pub fn select_best_adapter(adapters: &[DirectMlAdapterInfo]) -> Option<DirectMlAdapterInfo> {
    adapters
        .iter()
        .filter(|adapter| adapter.is_eligible())
        .min_by_key(|adapter| {
            (
                !adapter.is_discrete,
                Reverse(adapter.dedicated_video_memory),
                adapter.index,
            )
        })
        .cloned()
}

#[cfg(target_os = "windows")]
pub fn choose_directml_adapter() -> Result<Option<DirectMlAdapterInfo>> {
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }
        .context("failed to create DXGI factory for DirectML adapter enumeration")?;

    let mut adapters = Vec::new();
    let mut index = 0u32;

    loop {
        let adapter = match unsafe { factory.EnumAdapters1(index) } {
            Ok(adapter) => adapter,
            Err(err) if err.code() == DXGI_ERROR_NOT_FOUND => break,
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("failed to enumerate DXGI adapter at index {index}"));
            }
        };
        let desc = unsafe { adapter.GetDesc1() }.with_context(|| {
            format!("failed to read DXGI adapter description for index {index}")
        })?;
        adapters.push(adapter_info_from_desc(index as i32, &desc));
        index = index.saturating_add(1);
    }

    Ok(select_best_adapter(&adapters))
}

#[cfg(not(target_os = "windows"))]
pub fn choose_directml_adapter() -> Result<Option<DirectMlAdapterInfo>> {
    Ok(None)
}

pub fn directml_device_label(adapter: &DirectMlAdapterInfo) -> String {
    format!("DirectML (adapter {}: {})", adapter.index, adapter.name)
}

fn adapter_name_looks_remote_or_virtual(name: &str) -> bool {
    let normalized = name.trim().to_ascii_lowercase();

    normalized.contains("remote desktop")
        || normalized.contains("remote display")
        || normalized.contains("remote adapter")
        || normalized.contains("rdp")
        || normalized.contains("display mirror")
        || normalized.contains("mirroring driver")
        || normalized.contains("indirect")
        || normalized.contains("virtual")
}

#[cfg(target_os = "windows")]
fn adapter_info_from_desc(index: i32, desc: &DXGI_ADAPTER_DESC1) -> DirectMlAdapterInfo {
    let name = decode_description(&desc.Description);
    let normalized = name.to_ascii_lowercase();
    let dedicated_video_memory = desc.DedicatedVideoMemory as u64;

    DirectMlAdapterInfo {
        index,
        name,
        is_software: (desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0,
        is_discrete: dedicated_video_memory > 0,
        dedicated_video_memory,
        is_remote: normalized.contains("remote desktop")
            || normalized.contains("remote display")
            || normalized.contains("rdp"),
        is_virtual: normalized.contains("virtual")
            || normalized.contains("display mirror")
            || normalized.contains("indirect"),
    }
}

#[cfg(target_os = "windows")]
fn decode_description(raw: &[u16; 128]) -> String {
    let end = raw.iter().position(|ch| *ch == 0).unwrap_or(raw.len());
    String::from_utf16_lossy(&raw[..end]).trim().to_string()
}
