//! Counts words and lines of the text on stdin; answers JSON on stdout.
use std::io::Read;

fn main() {
    let mut text = String::new();
    std::io::stdin().read_to_string(&mut text).unwrap();
    let words = text.split_whitespace().count();
    let lines = text.lines().count();
    println!("{{\"words\":{words},\"lines\":{lines}}}");
}
