pub fn helper() -> i32 {
    1
}

pub struct Config {
    pub name: String,
}

pub fn run(c: Config) -> i32 {
    let base = helper();
    base
}
