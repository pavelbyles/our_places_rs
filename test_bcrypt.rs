fn main() {
    let valid = bcrypt::verify("testpassword", "$2y$05$jqKHniih1IUZ1t2cZ9eVnOG6md8Ou.GvT7sxxQFT3t91GQ5yHlH1W").unwrap();
    println!("VALID: {}", valid);
}
