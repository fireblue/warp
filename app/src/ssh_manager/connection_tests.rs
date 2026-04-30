use std::path::PathBuf;

use super::*;

#[test]
fn builds_expected_argv() {
    let cfg = PathBuf::from("/home/u/.warp/ssh_config");
    let cmd = build(&cfg, "prod-api-1");
    assert_eq!(cmd.program, PathBuf::from("ssh"));
    let args: Vec<&str> = cmd.args.iter().map(|a| a.to_str().unwrap()).collect();
    assert_eq!(
        args,
        vec!["-F", "/home/u/.warp/ssh_config", "-t", "prod-api-1"]
    );
}
