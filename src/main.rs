mod app;

use app::{App, ThreadPool};
use std::{
    env,
    sync::{Arc, Mutex, Weak},
    thread::available_parallelism,
};

fn main() -> std::io::Result<()> {
    let thread_count = available_parallelism()?.get();
    let thread_pool = ThreadPool::new(thread_count * 2);

    let current_dir_path = env::current_dir()?;
    let mut current_dir_dev: Option<u64> = None;

    #[cfg(unix)]
    {
        use std::{fs, os::unix::fs::MetadataExt};
        let metadata = fs::metadata(&current_dir_path)?;
        current_dir_dev = Some(metadata.dev());
    }

    let current_dir_name = env::current_exe()?
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();

    let mut app = App::new(
        Arc::clone(&thread_pool),
        Arc::clone(&thread_pool).scan_dir(
            current_dir_dev,
            current_dir_name,
            current_dir_path,
            Mutex::new(Weak::new()),
        )?,
    );

    let mut terminal = ratatui::init();
    let app_result = app.run(&mut terminal);

    ratatui::restore();
    app_result
}
