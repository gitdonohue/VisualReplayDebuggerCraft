//! Runs `vrd-cli mcp` as a real process and talks to it the way an MCP client does.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

#[test]
fn it_answers_over_stdio_and_exits_when_the_client_closes_it() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata");
    let mut child = Command::new(env!("CARGO_BIN_EXE_vrd-cli"))
        .args(["mcp", "--root", root])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("starts");
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    fn send(stdin: &mut impl Write, message: &str) {
        writeln!(stdin, "{message}").unwrap();
        stdin.flush().unwrap();
    }
    fn receive(stdout: &mut impl BufRead) -> serde_json::Value {
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        serde_json::from_str(&line).expect("one JSON message per line")
    }
    let mut ask = |stdin: &mut std::process::ChildStdin, message: &str| {
        send(stdin, message);
        receive(&mut stdout)
    };

    let init = ask(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}"#,
    );
    assert_eq!(init["result"]["serverInfo"]["name"], "vrd-cli");

    // A notification gets no answer, so the next reply is for the next request.
    send(&mut stdin, r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);

    let info = ask(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"capture_info","arguments":{"path":"sample.vrd"}}}"#,
    );
    assert_eq!(info["id"], 2);
    assert_eq!(info["result"]["isError"], false);
    assert_eq!(info["result"]["structuredContent"]["entities"], 37);

    let garbage = ask(&mut stdin, "this is not json");
    assert_eq!(garbage["error"]["code"], -32700);

    drop(stdin);
    assert!(child.wait().unwrap().success());
}
