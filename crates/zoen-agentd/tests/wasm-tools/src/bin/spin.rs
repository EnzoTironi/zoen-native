//! Never finishes: the fuel limit has to stop it.
fn main() {
    let mut x: u64 = 1;
    loop {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
        std::hint::black_box(x);
    }
}
