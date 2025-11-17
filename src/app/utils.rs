pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;

    let b = bytes as f64;

    if b < KB {
        format!("{:.2} B", b)
    } else if b < MB {
        format!("{:.2} KiB", b / KB)
    } else if b < GB {
        format!("{:.2} MiB", b / MB)
    } else if b < TB {
        format!("{:.2} GiB", b / GB)
    } else {
        format!("{:.2} TiB", b / TB)
    }
}
