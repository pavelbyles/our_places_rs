pub fn get_port(default_port: u16) -> u16 {
    static ENV_PORT_VAR: &str = "PORT";

    match std::env::var(ENV_PORT_VAR) {
        Ok(p) => match p.parse::<u16>() {
            Ok(n) => n,
            Err(_e) => default_port,
        },
        Err(_e) => default_port,
    }
}

#[cfg(test)]
mod tests {
    use crate::sys;

    #[test]
    fn get_port_test() {
        let expected_result: u16 = 8080;
        let default_port: u16 = 8080;
        assert_eq!(sys::get_port(default_port), expected_result);
    }
}
