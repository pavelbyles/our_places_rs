fn main() {
    let hash = "$2y$12$N9uYmGgJd285q5sYd21OceA16Wq.4T2pG9jWc9qH9yQp0d3/QnLwe"; // example from htpasswd
    let valid = bcrypt::verify("testpassword", hash).unwrap();
    println!("Valid: {}", valid);
}
