use super::*;

#[test]
fn owner_drop_terminates_and_reaps_its_child() {
    let child = OwnedChild::spawn(Command::new("sleep").arg("60")).unwrap();
    let pid = child.0.id();
    drop(child);
    let status = Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(!status.success(), "owned child must not outlive its owner");
}

#[test]
fn command_input_is_captured_without_shell_expansion() {
    let output = capture(
        &mut Command::new("cat"),
        Some(b"synthetic; $(not-a-command)\n"),
    )
    .unwrap();
    assert_eq!(output, b"synthetic; $(not-a-command)\n");
}
