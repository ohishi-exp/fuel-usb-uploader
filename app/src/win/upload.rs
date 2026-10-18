//! `POST <endpoint>` (Basic 認証、body は CSV のバイト列)。

use fuel_usb_core::outcome::{self, Outcome};
use fuel_usb_core::REQUEST_TIMEOUT;
use reqwest::blocking::Client;
use reqwest::header::CONTENT_TYPE;
use reqwest::redirect::Policy;

use super::credential::Credential;

pub fn client() -> reqwest::Result<Client> {
    Client::builder()
        .timeout(REQUEST_TIMEOUT)
        // 転送先で Authorization を付け直させない
        .redirect(Policy::none())
        .user_agent(concat!("fuel-usb-uploader/", env!("CARGO_PKG_VERSION")))
        .build()
}

pub fn send(client: &Client, endpoint: &str, cred: &Credential, body: &[u8]) -> Outcome {
    let resp = client
        .post(endpoint)
        .basic_auth(&cred.username, Some(cred.password.as_str()))
        .header(CONTENT_TYPE, "text/csv")
        .body(body.to_vec())
        .send();
    let Ok(resp) = resp else {
        return Outcome::network();
    };
    let status = resp.status().as_u16();
    match resp.text() {
        Ok(text) => outcome::interpret(status, &text),
        // 200 の本文が途中で切れたなら、取り込めたか分からない → やり直す (同じ内容は既存として数えられる)
        Err(_) if status == 200 => Outcome::network(),
        Err(_) => outcome::interpret(status, ""),
    }
}
