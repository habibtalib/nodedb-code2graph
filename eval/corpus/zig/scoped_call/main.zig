fn helper() i32 {
    return 1;
}

pub fn run() i32 {
    return helper();
}
// Same-file call: `run` calls `helper`, both defined in main.zig.
// Proves free-call detection resolves via Tier-A same-file matching.
