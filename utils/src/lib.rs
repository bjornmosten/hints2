pub fn read_file(path: &str) -> std::io::Result<String> {
    std::fs::read_to_string(path)
}
