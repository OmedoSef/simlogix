//! Moteur de simulation logique de SimLogix : modèle de circuit et événements discrets.

pub fn hello() -> String {
    "Hello, SimLogix!".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_says_hello() {
        assert_eq!(hello(), "Hello, SimLogix!");
    }
}
