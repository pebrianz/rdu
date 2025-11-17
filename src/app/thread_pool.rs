use super::FileDirectory;
use std::{
    collections::HashSet,
    fs,
    io::Result,
    path::PathBuf,
    sync::{
        Arc, Mutex, RwLock, Weak,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        mpsc,
    },
    thread,
};

type Job = Box<dyn FnOnce() -> Result<()> + Send + 'static>;

pub struct ThreadPool {
    sender: mpsc::Sender<Job>,
    inode_map: Arc<Mutex<HashSet<u64>>>,
    pub path_in_progress: Arc<Mutex<String>>,
    pub total_files: Arc<AtomicU32>,
    pub active_count: Arc<AtomicU32>,
}

impl ThreadPool {
    pub fn new(size: usize) -> Arc<Self> {
        let (tx, rx) = mpsc::channel::<Job>();
        let receiver = Arc::new(Mutex::new(rx));
        let active_count = Arc::new(AtomicU32::new(0));

        for _ in 0..size {
            let receiver = Arc::clone(&receiver);
            let active_count = Arc::clone(&active_count);

            thread::spawn(move || {
                loop {
                    let job = receiver.lock().unwrap().recv();
                    match job {
                        Ok(job) => {
                            active_count.fetch_add(1, Ordering::Relaxed);
                            match job() {
                                Ok(ok) => ok,
                                Err(_) => {}
                            }
                            active_count.fetch_sub(1, Ordering::Relaxed);
                        }
                        Err(_) => break,
                    }
                }
            });
        }

        Arc::new(Self {
            active_count,
            inode_map: Arc::new(Mutex::new(HashSet::new())),
            total_files: Arc::new(AtomicU32::new(0)),
            path_in_progress: Arc::new(Mutex::new(String::from(""))),
            sender: tx,
        })
    }

    pub fn scan_dir(
        self: Arc<Self>,
        root_dev: Option<u64>,
        name: String,
        path: PathBuf,
        parent: Mutex<Weak<FileDirectory>>,
    ) -> Result<Arc<FileDirectory>> {
        {
            let mut path_in_progress = self.path_in_progress.lock().unwrap();
            *path_in_progress = path.to_string_lossy().into_owned();
        }

        let metadata = fs::metadata(&path)?;

        let mut blocks: Option<u64> = None;
        let mut nlink = 1;

        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            blocks = Some(metadata.blocks());
            nlink = metadata.nlink();
        }

        let directory = Arc::new(FileDirectory {
            actual_size_bytes: AtomicU64::new(0),
            blocks,
            is_hardlink: false,
            dirty: AtomicBool::new(false),
            is_symlink: false,
            entries: Mutex::new(Vec::new()),
            hardlink_count: nlink,
            is_dir: true,
            name,
            parent,
            path,
        });

        let directory_clone = Arc::clone(&directory);

        Arc::clone(&self).execute(move || {
            let directory = Arc::clone(&directory_clone);
            let inode_map = Arc::clone(&self.inode_map);

            for entry in fs::read_dir(&directory.path)? {
                let entry = entry?;
                let metadata = entry.metadata()?;
                let name = entry.file_name().to_string_lossy().into_owned();
                let path = entry.path();

                let mut blocks: Option<u64> = None;
                let mut dev: Option<u64> = None;
                let mut nlink = 1;
                let mut inode: Option<u64> = None;

                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    blocks = Some(metadata.blocks());
                    dev = Some(metadata.dev());
                    inode = Some(metadata.ino());
                    nlink = metadata.nlink();
                }

                if dev != root_dev {
                    continue;
                }

                if let Some(inode) = inode {
                    let mut inode_map = inode_map.lock().unwrap();
                    if !inode_map.contains(&inode) {
                        inode_map.insert(inode);
                    } else {
                        continue;
                    }
                }

                if metadata.is_file() | metadata.is_symlink() {
                    let file = Arc::new(FileDirectory {
                        actual_size_bytes: AtomicU64::new(0),
                        blocks,
                        hardlink_count: nlink,
                        is_hardlink: if nlink > 1 { true } else { false },
                        is_symlink: metadata.is_symlink(),
                        dirty: AtomicBool::new(false),
                        entries: Mutex::new(Vec::new()),
                        is_dir: false,
                        name,
                        parent: Mutex::new(Arc::downgrade(&directory)),
                        path,
                    });

                    Arc::clone(&directory).add_entry(Arc::clone(&file))?;
                    Arc::clone(&self.total_files).fetch_add(1, Ordering::Relaxed);
                } else if metadata.is_dir() {
                    let entry_dir = Arc::clone(&self).scan_dir(
                        root_dev,
                        name,
                        path,
                        Mutex::new(Arc::downgrade(&directory)),
                    )?;
                    Arc::clone(&directory).add_entry(Arc::clone(&entry_dir))?;
                }
            }
            Ok(())
        });
        Ok(directory)
    }

    pub fn execute<F>(&self, f: F)
    where
        F: FnOnce() -> Result<()> + Send + 'static,
    {
        self.sender.send(Box::new(f)).unwrap();
    }
}
