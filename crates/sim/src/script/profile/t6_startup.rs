pub struct T6Startup {
    pub roots: Vec<String>,
    pub entries: Vec<String>,
}

impl T6Startup {
    pub fn new(map: &str) -> Self {
        let module = format!("maps/mp/{map}");
        Self {
            entries: vec![format!("{module}::main")],
            roots: vec![module],
        }
    }
}
