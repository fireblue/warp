use std::io::Write;

use tempfile::NamedTempFile;

use super::*;
use crate::ssh_manager::model::HostSource;

fn fixture(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().expect("creating tempfile");
    f.write_all(content.as_bytes()).expect("writing fixture");
    f.flush().expect("flushing fixture");
    f
}

#[test]
fn extracts_basic_host_with_user_source() {
    let file = fixture(
        "\
Host bastion
    HostName 1.2.3.4
    User ubuntu
    Port 2222
    IdentityFile ~/.ssh/id_ed25519
",
    );
    let hosts = parse_all(file.path()).expect("parses");
    assert_eq!(hosts.len(), 1);
    let h = &hosts[0];
    assert_eq!(h.alias, "bastion");
    assert_eq!(h.hostname.as_deref(), Some("1.2.3.4"));
    assert_eq!(h.user.as_deref(), Some("ubuntu"));
    assert_eq!(h.port, Some(2222));
    assert!(h.identity_file.is_some());
    // The fixture is the only file we read, so its hosts are Warp-owned.
    assert_eq!(h.source, HostSource::Warp);
}

#[test]
fn skips_wildcard_patterns() {
    let file = fixture(
        "\
Host *
    User defaultuser

Host concrete
    HostName concrete.example.com
",
    );
    let hosts = parse_all(file.path()).expect("parses");
    let aliases: Vec<&str> = hosts.iter().map(|h| h.alias.as_str()).collect();
    assert_eq!(aliases, vec!["concrete"]);
}

#[test]
fn proxy_jump_collected() {
    let file = fixture(
        "\
Host private
    HostName 10.0.0.5
    ProxyJump bastion
",
    );
    let hosts = parse_all(file.path()).expect("parses");
    assert_eq!(hosts[0].proxy_jump.as_deref(), Some("bastion"));
}
