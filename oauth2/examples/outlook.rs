use pimalaya_oauth2::{AuthorizationCodeGrant, RefreshAccessToken};
use std::env;

pub fn main() {
    let port = env::var("PORT")
        .unwrap_or(String::from("8080"))
        .parse::<u16>()
        .expect("Invalid port.");
    let client_id = env::var("CLIENT_ID").expect("Missing the CLIENT_ID environment variable.");
    let client_secret =
        env::var("CLIENT_SECRET").expect("Missing the CLIENT_SECRET environment variable.");

    let builder = AuthorizationCodeGrant::new(
        client_id,
        client_secret,
        "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
        "https://login.microsoftonline.com/common/oauth2/v2.0/token",
    )
    .unwrap()
    .with_redirect_port(port)
    .with_pkce()
    // for managing emails
    .with_scope("https://outlook.office.com/IMAP.AccessAsUser.All")
    // for sending emails
    .with_scope("https://outlook.office.com/SMTP.Send")
    // for refresh token
    .with_scope("offline_access");

    let client = builder.get_client().unwrap();
    let (redirect_url, csrf_token) = builder.get_redirect_url(&client);

    println!("Go to: {}", redirect_url.to_string());

    let (access_token, refresh_token) = builder.wait_for_redirection(&client, csrf_token).unwrap();

    println!("access token: {:?}", access_token);
    println!("refresh token: {:?}", refresh_token);

    if let Some(refresh_token) = refresh_token {
        let (access_token, refresh_token) = RefreshAccessToken::new()
            .refresh_access_token(&client, refresh_token)
            .unwrap();

        println!("new access token: {:?}", access_token);
        println!("new refresh token: {:?}", refresh_token);
    }
}
