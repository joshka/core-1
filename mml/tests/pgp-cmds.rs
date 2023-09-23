#[cfg(feature = "pgp-commands")]
#[tokio::test]
async fn pgp_cmds() {
    use concat_with::concat_line;
    use mml::{
        pgp::{CmdsPgp, Pgp},
        MimeInterpreter, MmlCompiler,
    };
    use process::Cmd;

    env_logger::builder().is_test(true).init();

    let pgp = Pgp::Cmds(CmdsPgp {
        encrypt_cmd: Some(Cmd::from(
            "gpg --homedir ./tests/gpg-home -eqa <recipients>",
        )),
        encrypt_recipient_fmt: Some(CmdsPgp::default_encrypt_recipient_fmt()),
        encrypt_recipients_sep: Some(CmdsPgp::default_encrypt_recipients_sep()),
        decrypt_cmd: Some(Cmd::from("gpg --homedir ./tests/gpg-home -dq")),
        sign_cmd: Some(Cmd::from("gpg --homedir ./tests/gpg-home -saq")),
        verify_cmd: Some(Cmd::from("gpg --homedir ./tests/gpg-home --verify -q")),
    });

    let mml = concat_line!(
        "From: alice@localhost",
        "To: bob@localhost",
        "Subject: subject",
        "",
        "<#part type=text/plain encrypt=pgpmime sign=pgpmime>",
        "Encrypted and signed message!",
    );

    let mml_compile_res = MmlCompiler::new()
        .with_pgp(pgp.clone())
        .compile(mml)
        .unwrap();
    let msg_builder = mml_compile_res.to_msg_builder().await.unwrap();

    let mml = MimeInterpreter::new()
        .with_show_only_headers(["From", "To", "Subject"])
        .with_pgp(pgp.clone())
        .interpret_msg_builder(msg_builder)
        .await
        .unwrap();

    let expected_mml = concat_line!(
        "From: alice@localhost",
        "To: bob@localhost",
        "Subject: subject",
        "",
        "Encrypted and signed message!",
        ""
    );

    assert_eq!(mml, expected_mml);
}