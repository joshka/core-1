#[cfg(not(feature = "pgp-commands"))]
fn main() {
    use std::process::exit;

    eprintln!("Cargo feature pgp-commands is missing.");
    eprintln!("Please re-run the command with --features pgp-commands.");
    exit(-1)
}

#[cfg(feature = "pgp-commands")]
#[tokio::main]
async fn main() {
    use mml::{CmdsPgp, MmlCompiler, Pgp};
    use process::Cmd;

    env_logger::builder().is_test(true).init();

    let mml = include_str!("./pgp.eml");
    let mime = MmlCompiler::new()
        .with_pgp(Pgp::Cmds(CmdsPgp {
            encrypt_cmd: Some(Cmd::from(
                "gpg --homedir ./tests/gpg-home -eqa <recipients>",
            )),
            encrypt_recipient_fmt: Some(CmdsPgp::default_encrypt_recipient_fmt()),
            encrypt_recipients_sep: Some(CmdsPgp::default_encrypt_recipients_sep()),
            decrypt_cmd: Some(Cmd::from("gpg --homedir ./tests/gpg-home -dq")),
            sign_cmd: Some(Cmd::from("gpg --homedir ./tests/gpg-home -saq")),
            verify_cmd: Some(Cmd::from("gpg --homedir ./tests/gpg-home --verify -q")),
        }))
        .compile(&mml)
        .await
        .unwrap()
        .write_to_string()
        .unwrap();

    println!("================================");
    println!("MML MESSAGE");
    println!("================================");
    println!();
    println!("{mml}");

    println!("================================");
    println!("COMPILED MIME MESSAGE");
    println!("================================");
    println!();
    println!("{mime}");
}