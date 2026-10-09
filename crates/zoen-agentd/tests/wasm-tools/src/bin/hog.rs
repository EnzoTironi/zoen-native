//! Grabs memory until something stops it: the memory cap has to.
fn main() {
    let mut keep: Vec<Vec<u8>> = Vec::new();
    loop {
        keep.push(vec![7u8; 8 * 1024 * 1024]);
        println!("{} MiB", keep.len() * 8);
    }
}
