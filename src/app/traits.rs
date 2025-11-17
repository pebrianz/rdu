pub trait GetPhysicalSize {
    fn get_physical_size(&self) -> std::io::Result<u64>;
}

