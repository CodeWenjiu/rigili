use std::path::{Path, PathBuf};

const SOFTWARE_ICD: [&str; 4] = ["lvp", "dzn", "swiftshader", "llvmpipe"];
const VENDOR_ICD: [(u16, &str); 9] = [
    (0x8086, "intel"),
    (0x1002, "radeon"),
    (0x10de, "nvidia"),
    (0x13b5, "panfrost"),
    (0x5143, "freedreno"),
    (0x14e4, "broadcom"),
    (0x1010, "powervr"),
    (0x1af4, "virtio"),
    (0x1ae0, "gfxstream"),
];

pub(crate) fn prefer_vulkan() {
    if std::env::var_os("RIGILI_ALLOW_GL").is_some() {
        return;
    }
    let Some(vendors) = pci_vendors() else {
        return;
    };
    let hardware = icd_paths()
        .into_iter()
        .filter(|path| is_hardware(path, &vendors))
        .collect::<Vec<_>>();
    if hardware.is_empty() {
        return;
    }
    let list = hardware
        .iter()
        .map(|path| path.to_string_lossy())
        .collect::<Vec<_>>()
        .join(":");

    // GPUI's wgpu instance requests Vulkan + GL, and bringing up the GL backend
    // loads Mesa's LLVM shader compiler. Both variables must be set before the
    // first GPU thread starts; a hardware Vulkan driver makes GL unnecessary.
    unsafe {
        std::env::set_var("VK_ICD_FILENAMES", list);
        std::env::set_var("__EGL_VENDOR_LIBRARY_FILENAMES", "/nonexistent");
    }
}

fn pci_vendors() -> Option<Vec<u16>> {
    let mut vendors = Vec::new();
    for entry in std::fs::read_dir("/sys/class/drm").ok()?.flatten() {
        let path = entry.path().join("device/vendor");
        let Ok(value) = std::fs::read_to_string(path) else {
            continue;
        };
        let value = value.trim().trim_start_matches("0x");
        if let Ok(vendor) = u16::from_str_radix(value, 16)
            && !vendors.contains(&vendor)
        {
            vendors.push(vendor);
        }
    }
    (!vendors.is_empty()).then_some(vendors)
}

fn icd_paths() -> Vec<PathBuf> {
    if let Some(value) = std::env::var_os("VK_ICD_FILENAMES") {
        return std::env::split_paths(&value)
            .filter(|path| path.is_file())
            .collect();
    }

    let mut dirs = vec![
        PathBuf::from("/run/opengl-driver/share/vulkan/icd.d"),
        PathBuf::from("/usr/share/vulkan/icd.d"),
        PathBuf::from("/etc/vulkan/icd.d"),
    ];
    if let Some(value) = std::env::var_os("XDG_DATA_DIRS") {
        dirs.extend(std::env::split_paths(&value).map(|dir| dir.join("vulkan/icd.d")));
    }

    dirs.into_iter()
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect()
}

fn is_hardware(path: &Path, vendors: &[u16]) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let name = name.to_ascii_lowercase();
    if SOFTWARE_ICD.iter().any(|driver| name.contains(driver)) {
        return false;
    }
    VENDOR_ICD
        .iter()
        .any(|(vendor, driver)| vendors.contains(vendor) && name.contains(driver))
}
