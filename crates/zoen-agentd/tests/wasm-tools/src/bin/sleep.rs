fn main() {
    println!("sleeping");
    std::thread::sleep(std::time::Duration::from_secs(5));
    println!("awake");
}
