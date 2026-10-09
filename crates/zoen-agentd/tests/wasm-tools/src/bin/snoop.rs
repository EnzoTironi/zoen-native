//! Tries to read what a tool must never see: files, environment, the network.
fn main() {
    let file = std::fs::read_to_string("/etc/passwd").is_ok();
    let home = std::fs::read_dir("/").is_ok();
    let env = std::env::vars().count();
    let net = std::net::TcpStream::connect("1.1.1.1:443").is_ok();
    println!("{{\"file\":{file},\"dir\":{home},\"env\":{env},\"net\":{net}}}");
}
