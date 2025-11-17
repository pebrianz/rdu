use super::{GetPhysicalSize, format_bytes};
use std::{
    io::Result,
    path::PathBuf,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

#[derive(Debug)]
pub struct FileDirectory {
    pub name: String,
    pub actual_size_bytes: AtomicU64,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub is_hardlink: bool,
    pub path: PathBuf,
    pub dirty: AtomicBool,
    pub parent: Mutex<Weak<FileDirectory>>,
    pub blocks: Option<u64>,
    pub hardlink_count: u64,
    pub entries: Mutex<Vec<Arc<FileDirectory>>>,
}

impl FileDirectory {
    pub fn array(&self) -> [String; 3] {
        [
            self.name.clone(),
            format_bytes(self.actual_size_bytes.load(Ordering::Relaxed)),
            self.get_type(),
        ]
    }
    pub fn get_type(&self) -> String {
        if self.is_hardlink {
            format!("hardlink({})", self.hardlink_count)
        } else if self.is_symlink {
            String::from("symlink")
        } else {
            String::from("-")
        }
    }
    pub fn actual_size_bytes(&self) -> u64 {
        if !self.dirty.load(Ordering::Relaxed) {
            self.actual_size_bytes.load(Ordering::Relaxed)
        } else {
            let total = AtomicU64::new(self.get_physical_size().unwrap());
            let entries = self.entries.lock().unwrap();
            for entry in &*entries {
                total.fetch_add(entry.actual_size_bytes(), Ordering::Relaxed);
            }
            let total_value = total.load(Ordering::Relaxed);
            self.actual_size_bytes.store(total_value, Ordering::Relaxed);
            self.dirty.store(false, Ordering::Relaxed);

            total_value
        }
    }
    pub fn add_entry(self: Arc<Self>, entry: Arc<FileDirectory>) -> Result<()> {
        let entry_size = entry.get_physical_size()?;
        entry.actual_size_bytes.store(entry_size, Ordering::Relaxed);
        self.entries.lock().unwrap().push(Arc::clone(&entry));
        self.prograte_dirty_up();
        Ok(())
    }
    fn prograte_dirty_up(&self) {
        self.dirty.store(true, Ordering::Relaxed);
        if let Some(parent) = self.parent.lock().unwrap().upgrade() {
            parent.prograte_dirty_up();
        }
    }
    pub fn sort_entries_by_size_desc(&self) {
        self.entries
            .lock()
            .unwrap()
            .sort_by(|a, b| b.actual_size_bytes().cmp(&a.actual_size_bytes()));
    }
    pub fn blocks(&self) -> u64 {
        if let Some(blocks) = self.blocks {
            blocks
        } else {
            0
        }
    }
}

#[cfg(unix)]
impl GetPhysicalSize for FileDirectory {
    fn get_physical_size(&self) -> Result<u64> {
        Ok(self.blocks() * 512)
    }
}

#[cfg(windows)]
impl GetPhysicalSize for FileDirectory {
    fn get_physical_size(&self) -> Result<u64> {
        use windows_sys::Win32::Foundation::GetLastError;
        use windows_sys::Win32::Storage::FileSystem::GetCompressedFileSizeW;
        use windows_sys::Win32::Storage::FileSystem::INVALID_FILE_SIZE;

        let wide: Vec<u16> = self
            .path
            .as_mut_os_str()
            .encode_wide()
            .chain(once(0))
            .collect();

        let mut high: u32 = 0;

        // SAFETY: Windows API call, safe if `path` is a valid null-terminated UTF-16 string
        let low = unsafe { GetCompressedFileSizeW(wide.as_ptr(), Some(&mut high)) };

        if low == INVALID_FILE_SIZE {
            let err = unsafe { GetLastError().0 };
            if err != 0 {
                return Err(Io::Error::from_raw_os_error(err as RawOsError));
            }
        };

        Ok((high as u64) << 32 | (low as u64))
    }
}
